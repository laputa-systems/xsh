use crate::runtime::value::{FloatValue, RecordMap, RuntimeError, Value};
use crate::source::Span;
use deser::{Atom, Deserialize, Error, ErrorKind, State};
use deser::de::{Sink, SinkHandle};
use std::sync::Arc;

pub(crate) const INPUT_LIMIT: usize = 64 * 1024 * 1024;
pub(crate) const OUTPUT_LIMIT: usize = 128 * 1024 * 1024;

pub(crate) fn context() -> deser::Context {
    deser::Context::with(deser::de::Limits::builder().max_depth(128)
        .max_len(16 * 1024 * 1024).max_items(1_000_000).max_events(4_000_000).build())
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
    let coordinate = |value: Option<usize>| value.map(|v| Value::Int(v as i64)).unwrap_or(Value::Null);
    let message = error.message().to_string();
    let payload = RecordMap::from([
        ("message".into(), Value::Str(message.as_str().into())),
        ("format".into(), Value::Str(format.into())),
        ("input_path".into(), Value::Null),
        ("line".into(), coordinate(error.line())),
        ("column".into(), coordinate(error.column())),
        ("offset".into(), coordinate(error.offset())),
        ("value_path".into(), Value::Null),
    ]);
    let facets = xsh_registry::errors::CODEC_ERROR_VARIANTS.iter()
        .find(|v| v.name == variant).expect("codec error variant").facets
        .iter().map(|f| f.name().to_string()).collect();
    RuntimeError::structured("CodecError", variant, payload, facets, message).with_span(span)
}

pub(crate) fn decode_json(text: &str, span: Span) -> Result<Value, RuntimeError> {
    if text.len() > INPUT_LIMIT {
        return Err(runtime_error(Error::new(ErrorKind::LimitExceeded, "codec input exceeds 64 MiB"), "json", span));
    }
    deser_json::DeserializerConfig::builder().context(context()).build()
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

pub(crate) struct Decoded(pub(crate) Value);
struct DecodeSink<'a> {
    out: &'a mut Option<Decoded>,
    value: Option<Value>,
    key: Option<String>,
    slot: Option<Decoded>,
}

fn decoded_float(value: f64) -> Result<Value, Error> {
    if !value.is_finite() { return Err(Error::new(ErrorKind::OutOfRange, "non-finite number")); }
    if value.fract() == 0.0 && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) && (value as i64) as f64 == value {
        Ok(Value::Int(value as i64))
    } else { Ok(Value::Float(FloatValue::new(value))) }
}

impl DecodeSink<'_> {
    fn flush(&mut self) -> Result<(), Error> {
        if let Some(Decoded(child)) = self.slot.take() {
            match self.value.as_mut() {
                Some(Value::List(items)) => items.push(child),
                Some(Value::Record(fields)) => {
                    let key: Arc<str> = self.key.take().expect("decoded object key").into();
                    if fields.get(&key).is_some() {
                        return Err(Error::new(ErrorKind::DuplicateKey, format!("duplicate key `{key}`")));
                    }
                    fields.insert(key, child);
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
    fn atom(&mut self, atom: Atom, _: &mut State) -> Result<(), Error> {
        let atom = match atom { Atom::Implicit(value) => value.value().to_atom(), other => other };
        self.value = Some(match atom {
            Atom::Null => Value::Null,
            Atom::Bool(v) => Value::Bool(v),
            Atom::I64(v) => Value::Int(v),
            Atom::U64(v) => Value::Int(i64::try_from(v).map_err(|_| Error::new(ErrorKind::OutOfRange, "integer exceeds i64"))?),
            Atom::F64(v) => decoded_float(v)?,
            Atom::Str(v) | Atom::Lexical(v) => Value::Str(v.as_ref().into()),
            Atom::Ext(v) => {
                if let Some(number) = v.downcast_value_ref::<deser::ext::Number>() {
                    if number.is_integer() { return Err(Error::new(ErrorKind::OutOfRange, "integer exceeds i64")); }
                    decoded_float(number.value())?
                } else if v.downcast_ref::<i128>().is_some() || v.downcast_ref::<u128>().is_some() {
                    return Err(Error::new(ErrorKind::OutOfRange, "integer exceeds i64"));
                } else { return Err(Error::new(ErrorKind::UnsupportedType, "unsupported codec extension")); }
            }
            _ => return Err(Error::new(ErrorKind::UnsupportedType, "unsupported codec atom")),
        });
        Ok(())
    }
    fn seq(&mut self, _: &mut State) -> Result<(), Error> { self.value = Some(Value::List(Vec::new())); Ok(()) }
    fn map(&mut self, _: &mut State) -> Result<(), Error> { self.value = Some(Value::Record(RecordMap::new())); Ok(()) }
    fn next_key(&mut self, state: &mut State) -> Result<SinkHandle<'_, 'de>, Error> {
        self.flush()?;
        Ok(String::deserialize_into(&mut self.key, state))
    }
    fn next_value(&mut self, state: &mut State) -> Result<SinkHandle<'_, 'de>, Error> {
        self.flush()?;
        Ok(Decoded::deserialize_into(&mut self.slot, state))
    }
    fn finish(&mut self, _: &mut State) -> Result<(), Error> {
        self.flush()?;
        *self.out = Some(Decoded(self.value.take().expect("decoded value")));
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
        if self.exceeded { return Err(Error::new(ErrorKind::LimitExceeded, "codec output exceeds 128 MiB")); }
        result?;
        Ok(String::from_utf8(self.bytes).expect("text codec emitted UTF-8"))
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
    containers: Vec<(bool, usize)>,
}

impl deser::ser::Layer for EncodeLimits {
    fn event(&mut self, event: deser::Event<'_>, next: &mut deser::ser::Next<'_>) -> Result<(), Error> {
        use deser::Event;
        self.events += 1;
        if self.events > 4_000_000 { return Err(Error::new(ErrorKind::LimitExceeded, "codec events exceed 4000000")); }
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
    fn json_decodes_directly_with_exact_integer_and_float_boundaries() {
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
        let value: Decoded = deser_yaml::DeserializerConfig::builder().context(context()).build()
            .from_str("name: \"api\"\nflags: [true, 2]\n").unwrap();
        let Value::Record(fields) = value.0 else { panic!("record"); };
        assert!(matches!(fields.get("name"), Some(Value::Str(name)) if name.as_ref() == "api"));
        let Some(Value::List(flags)) = fields.get("flags") else { panic!("flags"); };
        assert!(matches!(flags.as_slice(), [Value::Bool(true), Value::Int(2)]));
    }
}
