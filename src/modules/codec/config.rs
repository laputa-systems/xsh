use super::{Decoded, EncodeLimits, Format, Output, decode_context, runtime_error};
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use deser::de::DeserializeDriver;
use deser::{Atom, Error, ErrorKind, Event};

struct DecodeGuard {
    format: Format,
    scalar_bytes: usize,
}

impl deser::de::Layer for DecodeGuard {
    fn event<'de>(&mut self, event: deser::de::LayerEvent<'_, 'de>, next: &mut deser::de::Next<'_, 'de>) -> Result<(), Error> {
        if self.format == Format::Yaml && deser_yaml::take_tag(next.state_mut()).is_some() {
            return Err(Error::new(ErrorKind::UnsupportedType, "custom YAML tags are not representable"));
        }
        if let Event::Atom(Atom::Ext(value)) = event.event()
            && value.downcast_ref::<deser::ext::Datetime>().is_some()
        {
            return Err(Error::new(ErrorKind::UnsupportedType, "calendar values are not representable"));
        }
        super::charge_scalar_bytes(&mut self.scalar_bytes, event.event())?;
        next.emit(event)
    }
}

const YAML_ALIAS_LIMIT: usize = 10_000;

pub(super) fn yaml_config() -> deser_yaml::DeserializerConfig {
    deser_yaml::DeserializerConfig::builder()
        .version(deser_yaml::Version::V1_2)
        .required_version(Some(deser_yaml::Version::V1_2))
        .reject_merge_conflicts(true)
        .alias_limit(YAML_ALIAS_LIMIT)
        .context(decode_context(Format::Yaml)).build()
}

pub(super) fn yaml_sequence_config() -> deser_yaml::DeserializerConfig {
    let mut config = yaml_config();
    let mut context = config.context().clone();
    context.set(deser_yaml::AliasBudget::new(YAML_ALIAS_LIMIT));
    config.set_context(context);
    config
}

pub(super) fn setup_decode(format: Format, driver: &mut DeserializeDriver<'_, '_>) {
    driver.push_layer(DecodeGuard { format, scalar_bytes: 0 });
}

pub(super) fn setup_encode(format: Format, driver: &mut deser::ser::SerializeDriver<'_>) {
    driver.push_layer(EncodeLimits::default());
    driver.push_layer(EncodeGuard { format });
}

pub(crate) fn decode(format: Format, text: &str, span: Span) -> Result<Value, RuntimeError> {
    if text.len() > super::INPUT_LIMIT {
        return Err(runtime_error(Error::new(ErrorKind::LimitExceeded, "codec input exceeds 64 MiB"), format.identifier(), span));
    }
    let result = match format {
        Format::Yaml => {
            let config = yaml_config();
            let mut parser = deser_yaml::Deserializer::from_str_with_config(text, config);
            if parser.is_end() { return Ok(Value::Null); }
            parser.deserialize_with::<Decoded, _>(|driver| setup_decode(format, driver))
                .and_then(|value| parser.end().map(|()| value))
        }
        Format::Toml => {
            let config = deser_toml::DeserializerConfig::builder().context(decode_context(format)).build();
            deser_toml::Deserializer::from_str_with_config(text, config)
                .deserialize_with::<Decoded, _>(|driver| setup_decode(format, driver))
        }
        Format::Ini => {
            let config = deser_ini::DeserializerConfig::builder()
                .allow_no_value(false).lowercase_names(false)
                .context(decode_context(format)).build();
            deser_ini::Deserializer::from_str_with_config(text, config)
                .deserialize_with::<Decoded, _>(|driver| setup_decode(format, driver))
        }
        _ => unreachable!("configuration dispatch requires a configuration format"),
    };
    result.map(|value| value.0).map_err(|error| decode_error(error, format, span))
}

pub(crate) fn drive<'de>(
    format: Format,
    text: &'de str,
    driver: &mut DeserializeDriver<'_, 'de>,
    span: Span,
) -> Result<(), RuntimeError> {
    if format != Format::Ini {
        return Err(super::failure(
            "UnsupportedFormat",
            format.identifier(),
            "typed lexical decoding is not supported for this format",
            span,
        ));
    }
    if text.len() > super::INPUT_LIMIT {
        return Err(runtime_error(Error::new(ErrorKind::LimitExceeded, "codec input exceeds 64 MiB"), format.identifier(), span));
    }
    let config = deser_ini::DeserializerConfig::builder()
        .allow_no_value(false)
        .lowercase_names(false)
        .context(decode_context(format))
        .build();
    setup_decode(format, driver);
    deser_ini::Deserializer::from_str_with_config(text, config)
        .drive(driver)
        .map_err(|error| decode_error(error, format, span))
}

fn decode_error(error: Error, format: Format, span: Span) -> RuntimeError {
    let variant = match format {
        Format::Toml => match error.attachment::<deser_toml::SyntaxDetail>() {
            Some(deser_toml::SyntaxDetail::DuplicateKey) => Some("DuplicateKey"),
            Some(deser_toml::SyntaxDetail::NumberOverflow) => Some("Overflow"),
            None => None,
        },
        Format::Ini => error.attachment::<deser_ini::SyntaxDetail>().map(|detail| match detail {
            deser_ini::SyntaxDetail::DuplicateKey => "DuplicateKey",
        }),
        _ => None,
    };
    let error = runtime_error(error, format.identifier(), span);
    match variant {
        Some(variant) => RuntimeError::structured("CodecError", variant, error.payload, error.facets, error.message).with_span(span),
        None => error,
    }
}

struct EncodeGuard {
    format: Format,
}

impl deser::ser::Layer for EncodeGuard {
    fn event(&mut self, event: Event<'_>, next: &mut deser::ser::Next<'_>) -> Result<(), Error> {
        if next.state().is_map_key() && !matches!(&event, Event::Atom(Atom::Str(_) | Atom::Lexical(_))) {
            return Err(Error::new(ErrorKind::UnsupportedType, "configuration keys must be strings"));
        }
        match &event {
            Event::Atom(Atom::F64(value)) if !value.is_finite() => {
                return Err(Error::new(ErrorKind::UnsupportedType, "configuration numbers must be finite"));
            }
            Event::Atom(Atom::F32(value)) if !value.is_finite() => {
                return Err(Error::new(ErrorKind::UnsupportedType, "configuration numbers must be finite"));
            }
            Event::Atom(Atom::Null | Atom::Bytes(_)) if self.format == Format::Toml => {
                return Err(Error::new(ErrorKind::UnsupportedType, "TOML cannot represent Null or Bytes"));
            }
            Event::Atom(atom) if self.format == Format::Ini && !matches!(atom, Atom::Str(_) | Atom::Lexical(_)) => {
                return Err(Error::new(ErrorKind::UnsupportedType, "INI fields must be strings"));
            }
            Event::SeqStart(_) if self.format == Format::Ini => {
                return Err(Error::new(ErrorKind::UnsupportedType, "INI cannot represent sequences"));
            }
            Event::MapStart(_) if self.format == Format::Ini && next.state().depth() > 2 => {
                return Err(Error::new(ErrorKind::UnsupportedType, "INI supports globals and one level of sections"));
            }
            Event::Atom(Atom::Ext(_)) => {
                return Err(Error::new(ErrorKind::UnsupportedType, "configuration extensions are not representable"));
            }
            _ => {}
        }
        next.emit(event)
    }
}

pub(crate) fn encode<T: deser::Serialize + ?Sized>(format: Format, value: &T, span: Span) -> Result<String, RuntimeError> {
    let mut output = Output::new();
    let setup = |driver: &mut deser::ser::SerializeDriver<'_>| {
        setup_encode(format, driver);
    };
    let result = match format {
        Format::Yaml => deser_yaml::SerializerConfig::new().writer(&mut output).write_with(value, setup),
        Format::Toml => deser_toml::SerializerConfig::new().writer(&mut output).write_with(value, setup),
        Format::Ini => deser_ini::SerializerConfig::new().writer(&mut output).write_with(value, setup),
        _ => unreachable!("configuration dispatch requires a configuration format"),
    };
    output.finish(result).map_err(|error| runtime_error(error, format.identifier(), span))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    #[test]
    fn configuration_backends_enforce_textual_contracts_and_roundtrip_values() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for (format, input) in [
            (Format::Yaml, "name: worker\nratio: 1.0\nflags: [true, false]\n"),
            (Format::Toml, "name = \"worker\"\nratio = 1.0\nflags = [true, false]\n"),
            (Format::Ini, "Host = worker\nhost = api\n[Server]\nport = \"8080\" ; comment\n"),
        ] {
            let value = decode(format, input, span()).unwrap();
            let borrowed = crate::runtime::eval::borrowed_codec_value(format, &value, span());
            let encoded = encode(format, &borrowed, span()).unwrap();
            assert_eq!(decode(format, &encoded, span()).unwrap(), value);
        }
        for (format, input, variant) in [
            (Format::Yaml, "%YAML 1.1\n---\nflag: yes\n", "Syntax"),
            (Format::Yaml, "!custom value", "UnsupportedValue"),
            (Format::Yaml, "a: !custom [1]", "UnsupportedValue"),
            (Format::Yaml, "1: value", "UnsupportedValue"),
            (Format::Yaml, "!!timestamp 2026-10-26", "UnsupportedValue"),
            (Format::Yaml, "a: 1\na: 2", "DuplicateKey"),
            (Format::Yaml, "base: &base {a: 1}\napp: {<<: *base, a: 2}", "DuplicateKey"),
            (Format::Yaml, "--- 1\n--- 2\n", "Syntax"),
            (Format::Toml, "when = 2026-10-26\n", "UnsupportedValue"),
            (Format::Toml, "a = 1e400\n", "Overflow"),
            (Format::Ini, "a=1\na=2", "DuplicateKey"),
            (Format::Ini, "server=root\n[server]\na=b", "DuplicateKey"),
            (Format::Ini, "bare-key", "Syntax"),
        ] {
            let error = decode(format, input, span()).unwrap_err();
            assert_eq!(error.variant, variant, "{format:?}: {input}");
        }
        assert!(decode(Format::Yaml, "%YAML 1.2\n---\na: &a {x: 1}\napp: {<<: *a, y: 2}\n", span()).is_ok());
        assert!(decode(Format::Toml, "table = {\n x = 1, # TOML 1.1\n}\n", span()).is_ok());
    }

    #[test]
    fn yaml_alias_expansion_budget_rejects_only_after_10000_replayed_scalar_events() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let document = |count: usize| format!("base: &base value\ncopies: [{}]\n", vec!["*base"; count].join(","));
        let value = decode(Format::Yaml, &document(10_000), span()).unwrap();
        let Value::Record(fields) = value else { panic!("YAML record"); };
        let Value::List(copies) = fields.get("copies").unwrap() else { panic!("YAML copies"); };
        assert_eq!(copies.len(), 10_000);
        assert_eq!(decode(Format::Yaml, &document(10_001), span()).unwrap_err().variant, "Limit");
    }

    #[test]
    fn configuration_decoders_retain_coordinates_and_declared_limits() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let error = decode(Format::Toml, "a = 1\na = 2\n", span()).unwrap_err();
        assert_eq!(error.family, "CodecError");
        assert_eq!(error.variant, "DuplicateKey");
        assert_eq!(error.payload.get("line"), Some(&Value::Int(2)));
        assert_eq!(error.payload.get("offset"), Some(&Value::Int(6)));
        let error = decode(Format::Toml, "a = 18446744073709551616\n", span()).unwrap_err();
        assert_eq!(error.variant, "Overflow");
        assert_eq!(error.payload.get("column"), Some(&Value::Int(5)));
        for (format, depth) in [(Format::Yaml, 128), (Format::Toml, 127)] {
            let input = |depth: usize| format!("{}{}0{}", if format == Format::Toml { "a = " } else { "" }, "[".repeat(depth), "]".repeat(depth));
            assert!(decode(format, &input(depth), span()).is_ok());
            assert_eq!(decode(format, &input(depth + 1), span()).unwrap_err().variant, "Limit");
        }
    }

    #[test]
    fn yaml_aliases_cannot_amplify_scalar_storage_past_the_document_budget() {
        let input = format!("base: &base {}\ncopies: [{}]\n", "x".repeat(1024 * 1024), vec!["*base"; 128].join(","));
        assert_eq!(decode(Format::Yaml, &input, span()).unwrap_err().variant, "Limit");
    }

    #[test]
    fn configuration_encode_guards_reject_silent_loss_and_accept_yaml_bytes() {
        for format in [Format::Ini, Format::Toml] {
            assert_eq!(encode(format, &std::collections::BTreeMap::from([("a", Option::<i64>::None)]), span()).unwrap_err().variant, "UnsupportedValue");
            let bytes = Value::Bytes(b"raw".to_vec());
            let borrowed = crate::runtime::eval::borrowed_codec_value(format, &bytes, span());
            assert_eq!(encode(format, &borrowed, span()).unwrap_err().variant, "UnsupportedValue");
        }
        let nested = std::collections::BTreeMap::from([("section", std::collections::BTreeMap::from([("nested", std::collections::BTreeMap::from([("key", "value")]))]))]);
        assert_eq!(encode(Format::Ini, &nested, span()).unwrap_err().variant, "UnsupportedValue");
        let bytes = Value::Bytes(b"\0\xff".to_vec());
        let borrowed = crate::runtime::eval::borrowed_codec_value(Format::Yaml, &bytes, span());
        let encoded = encode(Format::Yaml, &borrowed, span()).unwrap();
        assert!(matches!(decode(Format::Yaml, &encoded, span()).unwrap(), Value::Bytes(value) if value.as_slice() == b"\0\xff"));
    }
}
