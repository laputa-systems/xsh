use super::{PreparedSchema, Evaluator, LoweredValue, RuntimeError, Span, Type};
use crate::modules::codec::{self, Format};
use crate::syntax::arena::UnknownFields;
use deser::{Atom, Error, ErrorKind, State};
use deser::de::{DeserializeDriver, Sink, SinkHandle};
use std::sync::Arc;
use bbtree::unsync::BBTreeMap as BTreeMap;

fn decimal(text: &str, floating: bool) -> bool {
    let bytes = text.as_bytes();
    let mut index = usize::from(bytes.first().is_some_and(|byte| matches!(byte, b'+' | b'-')));
    let begin = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) { index += 1; }
    let mut digits = index - begin;
    if floating && bytes.get(index) == Some(&b'.') {
        index += 1;
        let begin = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) { index += 1; }
        digits += index - begin;
    }
    if digits == 0 { return false; }
    if floating && bytes.get(index).is_some_and(|byte| matches!(byte, b'e' | b'E')) {
        index += 1;
        if bytes.get(index).is_some_and(|byte| matches!(byte, b'+' | b'-')) { index += 1; }
        let begin = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) { index += 1; }
        if index == begin { return false; }
    }
    index == bytes.len()
}

impl PreparedSchema {
    pub(super) fn admits_text(&self) -> bool {
        match self {
            Self::Validate(Type::Str | Type::Any) => true,
            Self::Optional(schema) | Self::Validated(_, schema) => schema.admits_text(),
            Self::Union(_, members) => members.iter().any(|schema| schema.admits_text()),
            _ => false,
        }
    }

    pub(super) fn lexical_atom(&self, text: &str) -> Option<LoweredValue> {
        let parsed = match self {
            Self::Optional(_) if text.is_empty() => Some(LoweredValue::Null),
            Self::Optional(schema) | Self::Validated(_, schema) => return schema.lexical_atom(text),
            Self::Validate(Type::Int | Type::UInt) if decimal(text, false) => text.parse::<i64>().ok().map(LoweredValue::Int),
            Self::Validate(Type::Float) if decimal(text, true) => text.parse::<f64>().ok().filter(|value| value.is_finite()).map(|value| LoweredValue::Float(crate::runtime::value::FloatValue::new(value))),
            Self::Validate(Type::Bool) => match text { "true" => Some(LoweredValue::Bool(true)), "false" => Some(LoweredValue::Bool(false)), _ => None },
            _ => None,
        };
        parsed
    }

    fn child(&self, key: Option<&str>) -> Option<Arc<Self>> {
        match self {
            Self::Optional(schema) | Self::Validated(_, schema) => schema.child(key),
            Self::Record(fields) => key.and_then(|key| fields.iter().find(|(name, _)| name.as_str().as_str() == key).map(|(_, schema)| schema.clone())),
            Self::Map(_, schema) | Self::List(schema) => Some(schema.clone()),
            _ => None,
        }
    }
}

struct LexicalSink<'a> {
    out: &'a mut Option<LoweredValue>,
    schema: Option<Arc<PreparedSchema>>,
    value: Option<LoweredValue>,
    key: Option<String>,
    slot: Option<LoweredValue>,
}

impl<'a> LexicalSink<'a> {
    fn new(out: &'a mut Option<LoweredValue>, schema: Option<Arc<PreparedSchema>>) -> Self {
        Self { out, schema, value: None, key: None, slot: None }
    }

    fn flush(&mut self) -> Result<(), Error> {
        if let Some(value) = self.slot.take() {
            match self.value.as_mut() {
                Some(LoweredValue::List(items)) => items.push(value),
                Some(LoweredValue::Record(fields)) => {
                    let key: Arc<str> = self.key.take().expect("lexical record key").into();
                    if Arc::make_mut(fields).insert(key, value).is_some() { return Err(Error::new(ErrorKind::DuplicateKey, "duplicate lexical key")); }
                }
                _ => return Err(Error::new(ErrorKind::InvalidState, "lexical child has no container")),
            }
        }
        Ok(())
    }
}

impl<'de> Sink<'de> for LexicalSink<'_> {
    fn atom(&mut self, atom: Atom, _: &mut State) -> Result<(), Error> {
        self.value = Some(match atom {
            Atom::Null => LoweredValue::Null,
            Atom::Lexical(text) => self.schema.as_ref().and_then(|schema| schema.lexical_atom(text.as_ref())).unwrap_or_else(|| LoweredValue::Str(text.as_ref().into())),
            _ => return Err(Error::new(ErrorKind::UnsupportedType, "typed lexical input requires lexical fields")),
        });
        Ok(())
    }
    fn seq(&mut self, _: &mut State) -> Result<(), Error> { self.value = Some(LoweredValue::List(Vec::new())); Ok(()) }
    fn map(&mut self, _: &mut State) -> Result<(), Error> { self.value = Some(LoweredValue::Record(Arc::new(BTreeMap::new()))); Ok(()) }
    fn next_key(&mut self, state: &mut State) -> Result<SinkHandle<'_, 'de>, Error> {
        self.flush()?;
        Ok(SinkHandle::arena(LexicalKey(&mut self.key), state))
    }
    fn next_value(&mut self, state: &mut State) -> Result<SinkHandle<'_, 'de>, Error> {
        self.flush()?;
        let schema = self.schema.as_ref().and_then(|schema| schema.child(self.key.as_deref()));
        Ok(SinkHandle::arena(LexicalSink::new(&mut self.slot, schema), state))
    }
    fn finish(&mut self, _: &mut State) -> Result<(), Error> { self.flush()?; *self.out = self.value.take(); Ok(()) }
}

struct LexicalKey<'a>(&'a mut Option<String>);
impl<'de> Sink<'de> for LexicalKey<'_> {
    fn atom(&mut self, atom: Atom, _: &mut State) -> Result<(), Error> {
        match atom {
            Atom::Str(value) | Atom::Lexical(value) => { *self.0 = Some(value.to_string()); Ok(()) }
            _ => Err(Error::new(ErrorKind::UnsupportedType, "lexical keys must be strings")),
        }
    }
}

pub(super) fn decode(evaluator: &Evaluator, format: Format, text: &str, schema: Arc<PreparedSchema>, span: Span) -> Result<LoweredValue, RuntimeError> {
    let mut out = None;
    {
        let mut driver = DeserializeDriver::from_fn(|state| SinkHandle::arena(LexicalSink::new(&mut out, Some(schema.clone())), state));
        codec::drive_text(format, text, &mut driver, span)?;
    }
    let value = out.ok_or_else(|| codec::failure("Syntax", format.identifier(), "lexical document has no value", span))?;
    schema.decode_policy(evaluator, value, "$", span, UnknownFields::Preserve, true).map_err(|error| {
        let path = error.payload.get("value_path").cloned().unwrap_or_else(|| super::super::Value::Str("$".into()));
        let mut failure = codec::failure("Schema", format.identifier(), error.message, span);
        failure.payload.insert("value_path".into(), path);
        failure
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::validated::IntRange;
    use crate::sema::wire_enums::PreparedWireEnums;
    use crate::source::SourceId;
    use crate::symbol::Name;

    #[test]
    fn lexical_union_counts_primitive_interpretations_before_wire_admission() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::new(SourceId::new(0), 0, 1);
            let evaluator = Evaluator::new(Vec::new());
            let names = [Name::intern("FirstWire"), Name::intern("SecondWire")];
            let enums = PreparedWireEnums { mappings: names.into_iter().map(|name| (name, Arc::new(crate::sema::wire_enums::WireEnumMapping { type_name: name, variants: BTreeMap::from([(Name::intern("One"), Arc::from("1"))]) }))).collect() };
            let target = Type::Union(names.into_iter().map(Type::Tag).collect());
            let schema = PreparedSchema::compile(&target, &enums);
            let text = LoweredValue::Str("1".into());
            let expected = schema.decode(&evaluator, text.clone(), "$", span).unwrap();
            let actual = schema.decode_policy(&evaluator, text, "$", span, UnknownFields::Preserve, true).unwrap();
            assert_eq!(actual, expected);
        });
    }

    #[test]
    fn lexical_admission_converts_structural_unions_and_checks_refinements_once() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::new(SourceId::new(0), 0, 1);
            let evaluator = Evaluator::new(Vec::new());
            let bounded = Type::bounded(IntRange::new(1, 10).unwrap(), Type::Int).unwrap();
            let record = |ty| Type::Record(BTreeMap::from([(Name::intern("port"), ty)]).into());
            let target = Type::Union(vec![record(bounded.clone()), record(Type::Float)]);
            let schema = PreparedSchema::compile(&target, &PreparedWireEnums::default());
            let (value, count) = super::super::super::validated::count_checks(|| decode(&evaluator, Format::Ini, "port=7", schema, span));
            let value = value.unwrap();
            let LoweredValue::Record(fields) = value else { panic!("lexical record") };
            assert_eq!(fields.get("port"), Some(&LoweredValue::Int(7)));
            assert_eq!(count, 1);
            let target = Type::Union(vec![bounded, Type::Float]);
            let schema = PreparedSchema::compile(&target, &PreparedWireEnums::default());
            let (result, count) = super::super::super::validated::count_checks(|| schema.decode_policy(&evaluator, LoweredValue::Str("7".into()), "$", span, UnknownFields::Preserve, true));
            assert!(result.unwrap_err().message.contains("ambiguous"));
            assert_eq!(count, 1);
            let lower = Type::bounded(IntRange::new(0, 10).unwrap(), Type::Int).unwrap();
            let upper = Type::bounded(IntRange::new(5, 15).unwrap(), Type::Int).unwrap();
            let target = Type::Union(vec![lower, upper]);
            let schema = PreparedSchema::compile(&target, &PreparedWireEnums::default());
            for (text, integer, checks) in [("7", 7, 1), ("12", 12, 2)] {
                let (result, count) = super::super::super::validated::count_checks(|| schema.decode_policy(&evaluator, LoweredValue::Str(text.into()), "$", span, UnknownFields::Preserve, true));
                assert_eq!(result.unwrap(), LoweredValue::Int(integer));
                assert_eq!(count, checks);
            }
            for target in [Type::Union(vec![Type::Str, Type::Int]), Type::Union(vec![Type::Int, Type::Str])] {
                let schema = PreparedSchema::compile(&target, &PreparedWireEnums::default());
                assert_eq!(schema.decode_policy(&evaluator, LoweredValue::Str("7".into()), "$", span, UnknownFields::Preserve, true).unwrap(), LoweredValue::Str("7".into()));
            }
        });
    }
}
