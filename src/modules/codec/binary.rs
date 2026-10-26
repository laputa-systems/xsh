use super::{Decoded, EncodeLimits, Format, Output};
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use deser::de::{Layer, LayerEvent, Next};
use deser::{Atom, Error, ErrorKind, Event, Serialize};

pub(super) fn setup_decode(format: Format, driver: &mut deser::de::DeserializeDriver<'_, '_>) {
    driver.push_layer(RejectBinaryMetadata { format });
}

pub(super) fn cbor_config() -> deser_cbor::DeserializerConfig {
    deser_cbor::DeserializerConfig::builder().context(super::decode_context(Format::Cbor)).build()
}

pub(super) fn msgpack_config() -> deser_msgpack::DeserializerConfig {
    deser_msgpack::DeserializerConfig::builder().context(super::decode_context(Format::Msgpack)).build()
}

pub(crate) fn decode(format: Format, bytes: &[u8], span: Span) -> Result<Value, RuntimeError> {
    if bytes.len() > super::INPUT_LIMIT {
        return Err(super::runtime_error(
            Error::new(ErrorKind::LimitExceeded, "codec input exceeds 64 MiB"),
            format.identifier(),
            span,
        ));
    }
    if !matches!(format, Format::Cbor | Format::Msgpack) {
        return Err(super::failure(
            "UnsupportedFormat",
            format.identifier(),
            "binary backend requires CBOR or MessagePack",
            span,
        ));
    }
    let value = match format {
        Format::Cbor => decode_cbor(bytes, format, span),
        Format::Msgpack => decode_msgpack(bytes, format, span),
        _ => unreachable!("binary format was checked"),
    }?;
    Ok(value)
}

fn decode_cbor(bytes: &[u8], format: Format, span: Span) -> Result<Value, RuntimeError> {
    let config = cbor_config();
    let mut deserializer = deser_cbor::Deserializer::from_slice_with_config(bytes, config);
    let decoded = deserializer
        .deserialize_with::<Decoded, _>(|driver| {
            setup_decode(format, driver);
        })
        .map_err(|error| super::runtime_error(error, format.identifier(), span))?;
    deserializer
        .end()
        .map_err(|error| super::runtime_error(error, format.identifier(), span))?;
    Ok(decoded.0)
}

fn decode_msgpack(bytes: &[u8], format: Format, span: Span) -> Result<Value, RuntimeError> {
    let config = msgpack_config();
    let mut deserializer = deser_msgpack::Deserializer::from_slice_with_config(bytes, config);
    let decoded = deserializer
        .deserialize_with::<Decoded, _>(|driver| {
            setup_decode(format, driver);
        })
        .map_err(|error| super::runtime_error(error, format.identifier(), span))?;
    deserializer
        .end()
        .map_err(|error| super::runtime_error(error, format.identifier(), span))?;
    Ok(decoded.0)
}

pub(crate) fn encode<T: Serialize + ?Sized>(
    format: Format,
    value: &T,
    canonical: bool,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    if !matches!(format, Format::Cbor | Format::Msgpack) {
        return Err(super::failure(
            "UnsupportedFormat",
            format.identifier(),
            "binary backend requires CBOR or MessagePack",
            span,
        ));
    }
    if canonical && format != Format::Cbor {
        return Err(super::failure(
            "Options",
            format.identifier(),
            "canonical output is available only for CBOR",
            span,
        ));
    }

    let mut output = Output::new();
    let result = match format {
        Format::Cbor => deser_cbor::SerializerConfig::builder()
            .canonical(canonical)
            .build()
            .writer(&mut output)
            .write_with(value, |driver| driver.push_layer(EncodeLimits::default())),
        Format::Msgpack => deser_msgpack::SerializerConfig::new()
            .writer(&mut output)
            .write_with(value, |driver| driver.push_layer(EncodeLimits::default())),
        _ => unreachable!("binary format was checked"),
    };
    output
        .finish_bytes(result)
        .map_err(|error| super::runtime_error(error, format.identifier(), span))
}

struct RejectBinaryMetadata {
    format: Format,
}

impl Layer for RejectBinaryMetadata {
    fn event<'de>(
        &mut self,
        event: LayerEvent<'_, 'de>,
        next: &mut Next<'_, 'de>,
    ) -> Result<(), Error> {
        if self.format == Format::Cbor && deser_cbor::take_tag(next.state_mut()).is_some() {
            return Err(Error::new(ErrorKind::UnsupportedType, "unsupported CBOR tag"));
        }
        if let Event::Atom(Atom::Ext(value)) = event.event() {
            if self.format == Format::Cbor && (value.downcast_ref::<i128>().is_some()
                || value.downcast_ref::<u128>().is_some()
                || value.downcast_ref::<deser::ext::BigInt>().is_some())
            {
                return Err(Error::new(ErrorKind::OutOfRange, "integer exceeds i64"));
            }
            return Err(Error::new(ErrorKind::UnsupportedType,
                format!("unsupported {} extension", self.format.identifier())));
        }
        next.emit(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_key::MapKey;
    use crate::runtime::eval::borrowed_codec_value;
    use crate::runtime::value::FloatValue;
    use crate::source::SourceId;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 1)
    }

    fn encode_value(format: Format, value: &Value, canonical: bool) -> Vec<u8> {
        let borrowed = borrowed_codec_value(format, value, span());
        encode(format, &borrowed, canonical, span()).unwrap()
    }

    fn map(entries: impl IntoIterator<Item = (MapKey, Value)>) -> Value {
        Value::Map(BTreeMap::from_iter(entries))
    }

    #[test]
    fn binary_bytes_preserve_zero_and_invalid_utf8() {
        for format in [Format::Cbor, Format::Msgpack] {
            let value = Value::Bytes(vec![0, 0x80, 0xff]);
            let encoded = encode_value(format, &value, false);
            assert_eq!(decode(format, &encoded, span()).unwrap(), value);
        }
    }

    #[test]
    fn binary_decodes_scalar_keys_and_keeps_empty_maps() {
        for format in [Format::Cbor, Format::Msgpack] {
            let encoded = encode_value(
                format,
                &map([
                    (MapKey::Str(Arc::from("text")), Value::Int(1)),
                    (MapKey::Int(-2), Value::Bool(true)),
                    (MapKey::Bool(false), Value::Bytes(vec![0, 0xff])),
                    (MapKey::Bytes(Arc::from([1, 2])), Value::Null),
                ]),
                false,
            );
            let Value::Map(decoded) = decode(format, &encoded, span()).unwrap() else {
                panic!("mixed scalar map");
            };
            assert_eq!(decoded.len(), 4);
            assert!(matches!(decoded.get(&MapKey::Int(-2)), Some(Value::Bool(true))));
            assert!(matches!(decoded.get(&MapKey::Bool(false)), Some(Value::Bytes(bytes)) if bytes == &[0, 0xff]));
            assert!(matches!(decode(format, &encode_value(format, &Value::Map(BTreeMap::new()), false), span()).unwrap(), Value::Map(entries) if entries.is_empty()));
        }
    }

    #[test]
    fn binary_string_maps_become_records_and_cbor_canonical_is_exact() {
        let value = map([
            (MapKey::Str(Arc::from("b")), Value::Int(2)),
            (MapKey::Str(Arc::from("a")), Value::Int(1)),
        ]);
        let expected = [0xa2, 0x61, b'a', 0x01, 0x61, b'b', 0x02];
        assert_eq!(encode_value(Format::Cbor, &value, true), expected);
        let Value::Record(fields) = decode(Format::Cbor, &expected, span()).unwrap() else {
            panic!("string map did not become record");
        };
        assert!(matches!(fields.get("a"), Some(Value::Int(1))));
    }

    #[test]
    fn binary_rejects_semantic_metadata_overflow_and_trailing_data() {
        assert_eq!(decode(Format::Cbor, &[0xc1, 0x01], span()).unwrap_err().variant, "UnsupportedValue");
        assert_eq!(decode(Format::Cbor, &[0xc0, 0x01], span()).unwrap_err().variant, "UnsupportedValue");
        assert_eq!(decode(Format::Cbor, &[0xc0, 0xa0], span()).unwrap_err().variant, "UnsupportedValue");
        assert_eq!(decode(Format::Cbor, &[0xc0, 0x80], span()).unwrap_err().variant, "UnsupportedValue");
        assert_eq!(decode(Format::Msgpack, &[0xd4, 0xff, 0x00], span()).unwrap_err().variant, "UnsupportedValue");
        assert_eq!(decode(Format::Msgpack, &[0xd4, 0x00, 0x00], span()).unwrap_err().variant, "UnsupportedValue");
        assert_eq!(decode(Format::Msgpack, &[0xd4, 0x2a, 0x00], span()).unwrap_err().variant, "UnsupportedValue");
        assert_eq!(decode(Format::Msgpack, &[0xd6, 0xff, 0x51, 0x4b, 0x67, 0xb0], span()).unwrap_err().variant, "UnsupportedValue");
        assert_eq!(decode(Format::Cbor, &[0x1b, 0x80, 0, 0, 0, 0, 0, 0, 0], span()).unwrap_err().variant, "Overflow");
        for bytes in [
            &[0x3b, 0x80, 0, 0, 0, 0, 0, 0, 0][..],
            &[0xc3, 0x49, 1, 0, 0, 0, 0, 0, 0, 0, 0][..],
            &[0xc2, 0x49, 1, 0, 0, 0, 0, 0, 0, 0, 0][..],
            &[0xc3, 0x51, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0][..],
        ] {
            assert_eq!(decode(Format::Cbor, bytes, span()).unwrap_err().variant, "Overflow");
        }
        assert_eq!(decode(Format::Msgpack, &[0xcf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], span()).unwrap_err().variant, "Overflow");
        assert_eq!(decode(Format::Cbor, &[0x01, 0x02], span()).unwrap_err().variant, "Syntax");
        assert_eq!(decode(Format::Msgpack, &[0xcc, 0x01, 0x02], span()).unwrap_err().variant, "Syntax");
    }

    #[test]
    fn binary_preserves_floats_and_rejects_nonfinite_values() {
        for format in [Format::Cbor, Format::Msgpack] {
            let value = Value::Float(FloatValue::new(1.25));
            assert_eq!(decode(format, &encode_value(format, &value, false), span()).unwrap(), value);
            let non_finite = match format {
                Format::Cbor => &[0xfa, 0x7f, 0xc0, 0, 0][..],
                Format::Msgpack => &[0xca, 0x7f, 0xc0, 0, 0][..],
                _ => unreachable!(),
            };
            assert_eq!(decode(format, non_finite, span()).unwrap_err().variant, "Overflow");
        }
    }
}
