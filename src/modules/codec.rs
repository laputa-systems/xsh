pub(crate) mod sequence;
pub(crate) mod config;
pub(crate) mod table;
use crate::runtime::value::{FloatValue, RecordMap, RuntimeError, Value};
use crate::source::Span;
use crate::symbol::Name;
use deser::{Atom, Deserialize, Error, ErrorKind, State};
use deser::de::{DeserializeDriver, Sink, SinkHandle};
use std::sync::Arc;
use crate::map_key::MapKey;
use std::collections::BTreeMap;

pub(crate) mod binary;

pub(crate) const INPUT_LIMIT: usize = 64 * 1024 * 1024;
pub(crate) const OUTPUT_LIMIT: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Format { Json, Yaml, Toml, Ini, Csv, Tsv, Cbor, Msgpack }

#[derive(Clone, Copy)]
enum FormatKind { Text, Binary }

struct FormatInfo {
    format: Format,
    identifier: &'static str,
    kind: FormatKind,
    sequence: bool,
    lexical: bool,
    bytes: bool,
}

const FORMATS: &[FormatInfo] = &[
    FormatInfo { format: Format::Json, identifier: "json", kind: FormatKind::Text, sequence: true, lexical: false, bytes: false },
    FormatInfo { format: Format::Yaml, identifier: "yaml", kind: FormatKind::Text, sequence: true, lexical: false, bytes: true },
    FormatInfo { format: Format::Toml, identifier: "toml", kind: FormatKind::Text, sequence: false, lexical: false, bytes: false },
    FormatInfo { format: Format::Ini, identifier: "ini", kind: FormatKind::Text, sequence: false, lexical: true, bytes: false },
    FormatInfo { format: Format::Csv, identifier: "csv", kind: FormatKind::Text, sequence: true, lexical: true, bytes: false },
    FormatInfo { format: Format::Tsv, identifier: "tsv", kind: FormatKind::Text, sequence: true, lexical: true, bytes: false },
    FormatInfo { format: Format::Cbor, identifier: "cbor", kind: FormatKind::Binary, sequence: true, lexical: false, bytes: true },
    FormatInfo { format: Format::Msgpack, identifier: "msgpack", kind: FormatKind::Binary, sequence: true, lexical: false, bytes: true },
];

impl Format {
    fn info(self) -> &'static FormatInfo { FORMATS.iter().find(|info| info.format == self).expect("codec capability definition") }
    pub(crate) fn identifier(self) -> &'static str { self.info().identifier }
    pub(crate) fn is_binary(self) -> bool { matches!(self.info().kind, FormatKind::Binary) }
    pub(crate) fn supports_bytes(self) -> bool { self.info().bytes }
    pub(crate) fn supports_sequence(self) -> bool { self.info().sequence }
    pub(crate) fn supports_lexical(self) -> bool { self.info().lexical }
    pub(crate) fn parse(identifier: &str, span: Span) -> Result<Self, RuntimeError> {
        FORMATS.iter().find(|info| info.identifier == identifier).map(|info| info.format)
            .ok_or_else(|| failure("UnsupportedFormat", identifier, format!("unsupported codec format `{identifier}`"), span))
    }
}

pub(crate) fn decode_context(format: Format) -> deser::Context {
    let mut context = deser::Context::with(deser::de::Limits::builder().max_depth(128)
        .max_len(16 * 1024 * 1024).max_items(1_000_000).max_events(4_000_000).build());
    context.set(format);
    context.set(deser::de::DuplicateKeys::Error);
    context
}

pub(crate) fn failure(variant: &'static str, format: &str, message: impl Into<String>, span: Span) -> RuntimeError {
    codec_error(variant, format, message.into(), None, None, None, span)
}

fn codec_error(variant: &'static str, format: &str, message: String, line: Option<usize>, column: Option<usize>, offset: Option<usize>, span: Span) -> RuntimeError {
    let coordinate = |value: Option<usize>| value.map(|v| Value::Int(v as i64)).unwrap_or(Value::Null);
    let payload = RecordMap::from([
        ("message".into(), Value::Str(message.as_str().into())),
        ("format".into(), Value::Str(format.into())),
        ("input_path".into(), Value::Null),
        ("line".into(), coordinate(line)),
        ("column".into(), coordinate(column)),
        ("offset".into(), coordinate(offset)),
        ("value_path".into(), Value::Null),
    ]);
    let facets = xsh_registry::errors::CODEC_ERROR_VARIANTS.iter()
        .find(|v| v.name == variant).expect("codec error variant").facets
        .iter().map(|f| f.name().to_string()).collect();
    RuntimeError::structured("CodecError", variant, payload, facets, message).with_span(span)
}

pub(crate) fn runtime_error(error: Error, format: &str, span: Span) -> RuntimeError {
    let variant = match error.kind() {
        ErrorKind::Syntax | ErrorKind::EndOfFile => "Syntax",
        ErrorKind::LimitExceeded => "Limit",
        ErrorKind::OutOfRange => "Overflow",
        ErrorKind::DuplicateKey => "DuplicateKey",
        ErrorKind::InvalidState | ErrorKind::Configuration => panic!("codec invariant: {error}"),
        _ => "UnsupportedValue",
    };
    codec_error(variant, format, error.message().to_string(), error.line(), error.column(), error.offset(), span)
}

pub(crate) fn decode_json(text: &str, span: Span) -> Result<Value, RuntimeError> {
    if text.len() > INPUT_LIMIT {
        return Err(runtime_error(Error::new(ErrorKind::LimitExceeded, "codec input exceeds 64 MiB"), "json", span));
    }
    deser_json::DeserializerConfig::builder().context(decode_context(Format::Json)).build()
        .from_str::<Decoded>(text).map(|v| v.0).map_err(|e| runtime_error(e, "json", span))
}

pub(crate) fn decode_json_bytes(bytes: &[u8], span: Span) -> Result<Value, RuntimeError> {
    if bytes.len() > INPUT_LIMIT {
        return Err(runtime_error(Error::new(ErrorKind::LimitExceeded, "codec input exceeds 64 MiB"), "json", span));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| {
        let error = runtime_error(Error::with_offset(ErrorKind::Syntax, "invalid UTF-8", error.valid_up_to()), "json", span);
        RuntimeError::structured("CodecError", "Encoding", error.payload, error.facets, error.message).with_span(span)
    })?;
    decode_json(text, span)
}

pub(crate) fn drive_text<'de>(
    format: Format,
    text: &'de str,
    driver: &mut DeserializeDriver<'_, 'de>,
    span: Span,
) -> Result<(), RuntimeError> {
    match format {
        Format::Ini => config::drive(format, text, driver, span),
        Format::Csv | Format::Tsv => {
            table::drive(format, text, table::DecodeOptions::defaults(format), driver, span)
        }
        _ => Err(failure(
            "UnsupportedFormat",
            format.identifier(),
            "typed lexical decoding is not supported for this format",
            span,
        )),
    }
}

pub(crate) fn decode_table(format: Format, text: &str, span: Span) -> Result<Value, RuntimeError> {
    let mut output = None;
    let mut driver = DeserializeDriver::from_fn(|state| Decoded::deserialize_into(&mut output, state));
    drive_text(format, text, &mut driver, span)?;
    drop(driver);
    output
        .map(|decoded: Decoded| decoded.0)
        .ok_or_else(|| failure("Syntax", format.identifier(), "table input produced no value", span))
}

pub(crate) fn encode_table<T: deser::Serialize + ?Sized>(
    format: Format,
    value: &T,
    span: Span,
) -> Result<String, RuntimeError> {
    table::encode(format, value, table::EncodeOptions::defaults(format), span)
}

pub(crate) struct Decoded(pub(crate) Value);
struct DecodeSink<'a> {
    out: &'a mut Option<Decoded>,
    value: Option<Value>,
    key: Option<MapKey>,
    slot: Option<Decoded>,
}

fn decoded_float(value: f64, format: Format) -> Result<Value, Error> {
    if !value.is_finite() { return Err(Error::new(ErrorKind::OutOfRange, "non-finite number")); }
    if format == Format::Json && value.fract() == 0.0 && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) && (value as i64) as f64 == value {
        Ok(Value::Int(value as i64))
    } else { Ok(Value::Float(FloatValue::new(value))) }
}

pub(crate) fn charge_scalar_bytes(total: &mut usize, event: &deser::Event<'_>) -> Result<(), Error> {
    let bytes = match event {
        deser::Event::Atom(Atom::Str(value) | Atom::Lexical(value)) => value.len(),
        deser::Event::Atom(Atom::Bytes(value)) => value.len(),
        _ => 0,
    };
    *total = total
        .checked_add(bytes)
        .filter(|total| *total <= OUTPUT_LIMIT)
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "codec scalar payload exceeds 128 MiB"))?;
    Ok(())
}

impl DecodeSink<'_> {
    fn flush(&mut self) -> Result<(), Error> {
        if let Some(Decoded(child)) = self.slot.take() {
            match self.value.as_mut() {
                Some(Value::List(items)) => items.push(child),
                Some(Value::Record(fields)) => {
                    let MapKey::Str(key) = self.key.take().expect("decoded object key") else { return Err(Error::new(ErrorKind::UnsupportedType, "textual object keys must be strings")); };
                    if fields.get(&key).is_some() {
                        return Err(Error::new(ErrorKind::DuplicateKey, format!("duplicate key `{key}`")));
                    }
                    fields.insert(key, child);
                }
                Some(Value::Map(fields)) => {
                    let key = self.key.take().expect("decoded map key");
                    if fields.insert(key, child).is_some() { return Err(Error::new(ErrorKind::DuplicateKey, "duplicate map key")); }
                }
                _ => return Err(Error::new(ErrorKind::InvalidState, "decoded child without container")),
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for Decoded {
    fn deserialize_into<'out>(out: &'out mut Option<Self>, state: &mut State) -> SinkHandle<'out, 'de> {
        SinkHandle::arena(DecodeSink { out, value: None, key: None, slot: None }, state)
    }
}

impl<'de> Sink<'de> for DecodeSink<'_> {
    fn atom(&mut self, atom: Atom, state: &mut State) -> Result<(), Error> {
        let format = *state.get::<Format>().expect("codec decode format");
        let atom = match atom { Atom::Implicit(value) => value.value().to_atom(), other => other };
        self.value = Some(match atom {
            Atom::Null => Value::Null,
            Atom::Bool(v) => Value::Bool(v),
            Atom::I64(v) => Value::Int(v),
            Atom::U64(v) => Value::Int(i64::try_from(v).map_err(|_| Error::new(ErrorKind::OutOfRange, "integer exceeds i64"))?),
            Atom::F64(v) => decoded_float(v, format)?,
            Atom::F32(v) => decoded_float(f64::from(v), format)?,
            Atom::Str(v) | Atom::Lexical(v) => Value::Str(v.as_ref().into()),
            Atom::Bytes(v) if format.supports_bytes() => Value::Bytes(v.into_owned()),
            Atom::Ext(v) => {
                if let Some(number) = v.downcast_value_ref::<deser::ext::Number>() {
                    if number.is_integer() { return Err(Error::new(ErrorKind::OutOfRange, "integer exceeds i64")); }
                    decoded_float(number.value(), format)?
                } else if let Some(&integer) = v.downcast_ref::<i128>() {
                    Value::Int(i64::try_from(integer).map_err(|_| Error::new(ErrorKind::OutOfRange, "integer exceeds i64"))?)
                } else if let Some(&integer) = v.downcast_ref::<u128>() {
                    Value::Int(i64::try_from(integer).map_err(|_| Error::new(ErrorKind::OutOfRange, "integer exceeds i64"))?)
                } else { return Err(Error::new(ErrorKind::UnsupportedType, "unsupported codec extension")); }
            }
            _ => return Err(Error::new(ErrorKind::UnsupportedType, "unsupported codec atom")),
        });
        Ok(())
    }
    fn seq(&mut self, _: &mut State) -> Result<(), Error> { self.value = Some(Value::List(Vec::new())); Ok(()) }
    fn map(&mut self, state: &mut State) -> Result<(), Error> {
        let format = *state.get::<Format>().expect("codec decode format");
        self.value = Some(if format.is_binary() { Value::Map(BTreeMap::new()) } else { Value::Record(RecordMap::new()) });
        Ok(())
    }
    fn next_key(&mut self, state: &mut State) -> Result<SinkHandle<'_, 'de>, Error> {
        self.flush()?;
        Ok(SinkHandle::arena(KeySink { out: &mut self.key, format: *state.get::<Format>().expect("codec decode format") }, state))
    }
    fn next_value(&mut self, state: &mut State) -> Result<SinkHandle<'_, 'de>, Error> {
        self.flush()?;
        Ok(Decoded::deserialize_into(&mut self.slot, state))
    }
    fn finish(&mut self, _: &mut State) -> Result<(), Error> {
        self.flush()?;
        let value = match self.value.take().expect("decoded value") {
            Value::Record(RecordMap::Dynamic(fields)) => Value::Record(RecordMap::from_name_values(
                fields.into_iter().map(|(key, value)| (Name::intern(&key), value)).collect(),
            )),
            Value::Map(fields) if !fields.is_empty() && fields.keys().all(|key| key.as_str().is_some()) => {
                let values = fields.into_iter().map(|(key, value)| {
                    let MapKey::Str(key) = key else { unreachable!("string-keyed binary map") };
                    (Name::intern(&key), value)
                }).collect();
                Value::Record(RecordMap::from_name_values(values))
            }
            value => value,
        };
        *self.out = Some(Decoded(value));
        Ok(())
    }
}

struct KeySink<'a> { out: &'a mut Option<MapKey>, format: Format }

impl<'de> Sink<'de> for KeySink<'_> {
    fn atom(&mut self, atom: Atom, _: &mut State) -> Result<(), Error> {
        let atom = match atom { Atom::Implicit(value) => value.value().to_atom(), other => other };
        *self.out = Some(match atom {
            Atom::Str(value) | Atom::Lexical(value) => MapKey::Str(Arc::from(value.as_ref())),
            Atom::I64(value) if self.format.is_binary() => MapKey::Int(value),
            Atom::U64(value) if self.format.is_binary() => MapKey::Int(i64::try_from(value).map_err(|_| Error::new(ErrorKind::OutOfRange, "map key exceeds i64"))?),
            Atom::Bool(value) if self.format.is_binary() => MapKey::Bool(value),
            Atom::Bytes(value) if self.format.is_binary() => MapKey::Bytes(Arc::from(value.data())),
            _ => return Err(Error::new(ErrorKind::UnsupportedType, "unsupported map key")),
        });
        Ok(())
    }
}


pub(crate) struct Output {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Output {
    pub(crate) fn new() -> Self { Self { bytes: Vec::new(), exceeded: false } }
    pub(crate) fn finish(self, result: Result<(), Error>) -> Result<String, Error> {
        self.finish_bytes(result).map(|bytes| String::from_utf8(bytes).expect("text codec emitted UTF-8"))
    }
    pub(crate) fn finish_bytes(self, result: Result<(), Error>) -> Result<Vec<u8>, Error> {
        if self.exceeded { return Err(Error::new(ErrorKind::LimitExceeded, "codec output exceeds 128 MiB")); }
        result?;
        Ok(self.bytes)
    }
}

impl std::io::Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > OUTPUT_LIMIT - self.bytes.len() {
            self.exceeded = true;
            return Err(std::io::Error::other("codec output limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

#[derive(Default)]
pub(crate) struct EncodeLimits {
    events: usize,
    payload_bytes: usize,
    containers: Vec<(bool, usize)>,
}

impl deser::ser::Layer for EncodeLimits {
    fn event(&mut self, event: deser::Event<'_>, next: &mut deser::ser::Next<'_>) -> Result<(), Error> {
        use deser::Event;
        self.events += 1;
        if self.events > 4_000_000 { return Err(Error::new(ErrorKind::LimitExceeded, "codec events exceed 4000000")); }
        let payload_bytes = match &event {
            Event::Atom(Atom::Str(value) | Atom::Lexical(value)) => value.len(),
            Event::Atom(Atom::Bytes(value)) => value.data().len(),
            _ => 0,
        };
        if payload_bytes > OUTPUT_LIMIT - self.payload_bytes { return Err(Error::new(ErrorKind::LimitExceeded, "codec payload exceeds 128 MiB")); }
        self.payload_bytes += payload_bytes;
        let end = matches!(event, Event::MapEnd | Event::SeqEnd);
        if !end && let Some((map, count)) = self.containers.last_mut() {
            if !*map || next.state().is_map_key() { *count += 1; }
            if *count > 1_000_000 { return Err(Error::new(ErrorKind::LimitExceeded, "codec container exceeds 1000000 entries")); }
        }
        match &event {
            Event::MapStart(_) | Event::SeqStart(_) => {
                if self.containers.len() >= 128 { return Err(Error::new(ErrorKind::LimitExceeded, "codec nesting exceeds 128")); }
                self.containers.push((matches!(event, Event::MapStart(_)), 0));
            }
            Event::MapEnd | Event::SeqEnd => { self.containers.pop().expect("codec container end"); }
            Event::Atom(Atom::Str(value) | Atom::Lexical(value)) if value.len() > 16 * 1024 * 1024 => {
                return Err(Error::new(ErrorKind::LimitExceeded, "codec string exceeds 16 MiB"));
            }
            Event::Atom(Atom::Bytes(value)) if value.data().len() > 16 * 1024 * 1024 => {
                return Err(Error::new(ErrorKind::LimitExceeded, "codec bytes exceed 16 MiB"));
            }
            _ => {}
        }
        next.emit(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    #[test]
    fn malformed_text_objects_do_not_intern_unfinished_record_keys() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for (format, text, variant) in [
            (Format::Json, "{\"unfinished-json-key\":1,\"unfinished-json-key\":2}", "DuplicateKey"),
            (Format::Yaml, "unfinished-yaml-key: 1\n42: 2\n", "UnsupportedValue"),
            (Format::Json, "{\"unfinished-syntax-key\":1,", "Syntax"),
        ] {
            assert_eq!(decode_text(format, text, span()).unwrap_err().variant, variant);
            assert_eq!(symbols.dynamic_stats(), (0, 0));
        }
    }

    #[test]
    fn decoded_objects_share_compact_shapes_across_codec_boundaries() {
        fn collect(value: &Value, shapes: &mut Vec<Arc<crate::runtime::value::RecordShapeData>>) {
            match value {
                Value::Record(fields) => {
                    let RecordMap::Shaped { shape, values } = fields else { panic!("decoded object retains a per-instance tree instead of compact shared shape storage") };
                    assert_eq!(values.len(), fields.len());
                    if fields.get("name").is_some() && fields.get("version").is_some() { shapes.push(shape.clone()); }
                    for (_, value) in fields.iter() { collect(value, shapes); }
                }
                Value::List(values) => for value in values { collect(value, shapes); },
                _ => {}
            }
        }
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut shapes = Vec::new();
            for (format, text) in [
                (Format::Json, r#"[{"version":1,"name":"pkg-one"},{"name":"pkg-two","version":2}]"#),
                (Format::Yaml, "- name: pkg-one\n  version: 1\n- name: pkg-two\n  version: 2\n"),
                (Format::Toml, "[[rows]]\nname = \"pkg-one\"\nversion = 1\n[[rows]]\nname = \"pkg-two\"\nversion = 2\n"),
                (Format::Ini, "[one]\nname=pkg-one\nversion=1\n[two]\nname=pkg-two\nversion=2\n"),
                (Format::Csv, "name,version\npkg-one,1\npkg-two,2\n"),
                (Format::Tsv, "name\tversion\npkg-one\t1\npkg-two\t2\n"),
            ] {
                let decoded = decode_text(format, text, span()).unwrap();
                let start = shapes.len(); collect(&decoded, &mut shapes);
                assert_eq!(shapes.len() - start, 2, "{format:?}");
                if format == Format::Json {
                    let Value::List(rows) = decoded else { panic!("JSON row list") };
                    let Value::Record(first) = &rows[0] else { panic!("JSON row record") };
                    assert_eq!(first.get("name"), Some(&Value::Str("pkg-one".into())));
                    assert_eq!(first.get("version"), Some(&Value::Int(1)));
                    assert_eq!(super::super::json::encode_json(&rows[1], false, span()).unwrap(), r#"{"name":"pkg-two","version":2}"#);
                }
            }
            let rows = [BTreeMap::from([("name", "pkg-one"), ("version", "1")]), BTreeMap::from([("name", "pkg-two"), ("version", "2")])];
            for (format, empty, scalar_map) in [(Format::Cbor, &[0xa0][..], &[0xa1, 1, 2][..]), (Format::Msgpack, &[0x80][..], &[0x81, 1, 2][..])] {
                let bytes = encode_bytes(format, &rows, false, span()).unwrap();
                let decoded = decode_bytes(format, &bytes, span()).unwrap();
                let start = shapes.len(); collect(&decoded, &mut shapes);
                assert_eq!(shapes.len() - start, 2, "{format:?}");
                assert!(matches!(decode_bytes(format, empty, span()).unwrap(), Value::Map(fields) if fields.is_empty()));
                assert!(matches!(decode_bytes(format, scalar_map, span()).unwrap(), Value::Map(fields) if fields.get(&MapKey::Int(1)) == Some(&Value::Int(2))));
            }
            assert!(shapes.iter().all(|shape| Arc::ptr_eq(shape, &shapes[0])));
            assert_eq!(decode_json(r#"{"name":"first","name":"second"}"#, span()).unwrap_err().variant, "DuplicateKey");
        });
    }

    #[test]
    fn json_decodes_directly_with_exact_integer_and_float_boundaries() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for (input, expected) in [("-9223372036854775808", i64::MIN), ("9223372036854775807", i64::MAX), ("-0.0", 0), ("1e2", 100)] {
            assert!(matches!(decode_json(input, span()).unwrap(), Value::Int(value) if value == expected), "{input}");
        }
        for input in ["1e999", "9223372036854775808", "-9223372036854775809", "18446744073709551616", "99999999999999999999999999999999999999999999999999999999999999"] {
            assert_eq!(decode_json(input, span()).unwrap_err().variant, "Overflow", "{input}");
        }
        let value = decode_json(r#"{"a":[null,true,"\uD834\uDD1E",0.10000000000000000001]}"#, span()).unwrap();
        let Value::Record(fields) = value else { panic!("record"); };
        let Value::List(items) = fields.get("a").unwrap() else { panic!("list"); };
        assert!(matches!(&items[2], Value::Str(text) if text.as_ref() == "𝄞"));
        assert!(matches!(&items[3], Value::Float(v) if v.0 == 0.1));
    }

    #[test]
    fn json_rejects_duplicate_keys_partial_values_and_excessive_depth() {
        for input in [r#"{"a":1,"a":2}"#, r#"{"outer":{"a":1,"a":2}}"#] {
            let error = decode_json(input, span()).unwrap_err();
            assert_eq!(error.family, "CodecError");
            assert_eq!(error.variant, "DuplicateKey");
            assert!(error.facets.iter().any(|facet| facet == "InvalidData"));
        }
        for input in ["[1,", "{} false", r#""\uD800""#, "{", "1e"] {
            assert_eq!(decode_json(input, span()).unwrap_err().variant, "Syntax", "{input}");
        }
        let nested = format!("{}0{}", "[".repeat(129), "]".repeat(129));
        assert_eq!(decode_json(&nested, span()).unwrap_err().variant, "Limit");
        assert!(decode_json(&format!("{}0{}", "[".repeat(128), "]".repeat(128)), span()).is_ok());
    }

    #[test]
    fn yaml_pilot_uses_the_direct_xsh_sink() {
        let value: Decoded = deser_yaml::DeserializerConfig::builder().context(decode_context(Format::Yaml)).build()
            .from_str("name: \"api\"\nflags: [true, 2]\n").unwrap();
        let Value::Record(fields) = value.0 else { panic!("record"); };
        assert!(matches!(fields.get("name"), Some(Value::Str(name)) if name.as_ref() == "api"));
        let Some(Value::List(flags)) = fields.get("flags") else { panic!("flags"); };
        assert!(matches!(flags.as_slice(), [Value::Bool(true), Value::Int(2)]));
    }
}

pub(crate) fn decode_text(format: Format, text: &str, span: Span) -> Result<Value, RuntimeError> {
    if text.len() > INPUT_LIMIT { return Err(failure("Limit", format.identifier(), "codec input exceeds 64 MiB", span)); }
    match format {
        Format::Json => decode_json(text, span),
        Format::Yaml | Format::Toml | Format::Ini => config::decode(format, text, span),
        Format::Csv | Format::Tsv => decode_table(format, text, span),
        _ => Err(failure("UnsupportedFormat", format.identifier(), "format does not support text decoding", span)),
    }
}

pub(crate) fn decode_bytes(format: Format, bytes: &[u8], span: Span) -> Result<Value, RuntimeError> {
    if bytes.len() > INPUT_LIMIT { return Err(failure("Limit", format.identifier(), "codec input exceeds 64 MiB", span)); }
    if format.is_binary() { return binary::decode(format, bytes, span); }
    let text = std::str::from_utf8(bytes).map_err(|error| {
        codec_error("Encoding", format.identifier(), "invalid UTF-8".into(), None, None, Some(error.valid_up_to()), span)
    })?;
    decode_text(format, text, span)
}

pub(crate) fn encode_text<T: deser::Serialize + ?Sized>(format: Format, value: &T, span: Span) -> Result<String, RuntimeError> {
    match format {
        Format::Json => {
            let mut output = Output::new();
            let result = deser_json::SerializerConfig::new().writer(&mut output).write_with(value, |driver| driver.push_layer(EncodeLimits::default()));
            output.finish(result).map_err(|error| runtime_error(error, "json", span))
        }
        Format::Yaml | Format::Toml | Format::Ini => config::encode(format, value, span),
        Format::Csv | Format::Tsv => encode_table(format, value, span),
        _ => Err(failure("UnsupportedFormat", format.identifier(), "format does not support text encoding", span)),
    }
}

pub(crate) fn encode_bytes<T: deser::Serialize + ?Sized>(format: Format, value: &T, canonical: bool, span: Span) -> Result<Vec<u8>, RuntimeError> {
    if canonical && format != Format::Cbor { return Err(failure("Options", format.identifier(), "canonical encoding requires cbor", span)); }
    if format.is_binary() { binary::encode(format, value, canonical, span) }
    else { encode_text(format, value, span).map(String::into_bytes) }
}

fn input_io_error(path: &std::path::Path, error: std::io::Error, span: Span) -> RuntimeError {
    let mut failure = RuntimeError::host("fs-read", &error).with_span(span);
    failure.message = format!("{}: {}", path.display(), failure.message);
    failure
}

pub(crate) fn open_input(path: &std::path::Path, span: Span) -> Result<std::fs::File, RuntimeError> {
    std::fs::File::open(path).map_err(|error| input_io_error(path, error, span))
}

pub(crate) fn read_input(path: &std::path::Path, span: Span) -> Result<Vec<u8>, RuntimeError> {
    use std::io::Read;
    let file = open_input(path, span)?;
    let mut bytes = Vec::new();
    file.take((INPUT_LIMIT + 1) as u64).read_to_end(&mut bytes).map_err(|error| input_io_error(path, error, span))?;
    Ok(bytes)
}

pub(crate) fn read(format: Format, path: &std::path::Path, input_path: &crate::runtime::value::PathValue, span: Span) -> Result<Value, RuntimeError> {
    let bytes = read_input(path, span)?;
    decode_bytes(format, &bytes, span).map_err(|mut error| {
        error.payload.insert("input_path".into(), Value::Path(input_path.clone()));
        error
    })
}

pub(crate) fn formats() -> Value {
    Value::List(FORMATS.iter().map(|info| Value::Record(RecordMap::from([
        ("identifier".into(), Value::Str(info.identifier.into())),
        ("text".into(), Value::Bool(matches!(info.kind, FormatKind::Text))),
        ("binary".into(), Value::Bool(matches!(info.kind, FormatKind::Binary))),
        ("single".into(), Value::Bool(true)),
        ("sequence".into(), Value::Bool(info.format.supports_sequence())),
        ("lexical_typing".into(), Value::Bool(info.format.supports_lexical())),
    ]))).collect())
}
