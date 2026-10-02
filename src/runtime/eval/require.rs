use super::{Evaluator, LoweredTagValue, LoweredTypeCheck, LoweredValue, Name, RuntimeError, Span, Type};
use crate::sema::wire_enums::{PreparedWireEnums, WireEnumMapping};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) enum PreparedSchema {
    Validate(Type),
    Record(Vec<(Name, Arc<PreparedSchema>)>),
    List(Arc<PreparedSchema>),
    Map(Type, Arc<PreparedSchema>),
    Optional(Arc<PreparedSchema>),
    WireEnum(Arc<WireEnumMapping>),
}

impl PreparedSchema {
    pub(super) fn compile(ty: &Type, enums: &PreparedWireEnums) -> Arc<Self> {
        Arc::new(match ty {
            Type::Record(fields) if !fields.is_empty() => Self::Record(fields.iter()
                .map(|(name, ty)| (*name, Self::compile(ty, enums))).collect()),
            Type::List(item) => Self::List(Self::compile(item, enums)),
            Type::Map(key, item) => Self::Map((**key).clone(), Self::compile(item, enums)),
            Type::Optional(item) => Self::Optional(Self::compile(item, enums)),
            Type::Tag(name) if enums.mappings.contains_key(name) => Self::WireEnum(enums.mappings[name].clone()),
            ty => Self::Validate(ty.clone()),
        })
    }

    fn converts_wire(&self) -> bool {
        match self {
            Self::WireEnum(_) => true,
            Self::Record(fields) => fields.iter().any(|(_, schema)| schema.converts_wire()),
            Self::List(schema) | Self::Map(_, schema) | Self::Optional(schema) => schema.converts_wire(),
            Self::Validate(_) => false,
        }
    }

    pub(super) fn valid(&self) -> bool {
        match self {
            Self::WireEnum(mapping) => !mapping.variants.is_empty() && mapping.variants.values()
                .collect::<std::collections::BTreeSet<_>>().len() == mapping.variants.len(),
            Self::Record(fields) => fields.iter().all(|(_, schema)| schema.valid()),
            Self::Map(key, schema) => key.is_map_key() && schema.valid(),
            Self::List(schema) | Self::Optional(schema) => schema.valid(),
            Self::Validate(_) => true,
        }
    }

    pub(super) fn visit_wire_mappings(&self, visit: &mut impl FnMut(&Arc<WireEnumMapping>) -> bool) -> bool {
        match self {
            Self::WireEnum(mapping) => visit(mapping),
            Self::Record(fields) => fields.iter().all(|(_, schema)| schema.visit_wire_mappings(visit)),
            Self::List(schema) | Self::Map(_, schema) | Self::Optional(schema) => schema.visit_wire_mappings(visit),
            Self::Validate(_) => true,
        }
    }

    pub(super) fn matches_type(&self, ty: &Type) -> bool {
        match (self, ty) {
            (Self::Validate(expected), actual) => expected == actual,
            (Self::WireEnum(mapping), Type::Tag(name)) => mapping.type_name == *name,
            (Self::Record(schemas), Type::Record(fields)) => schemas.len() == fields.len()
                && schemas.iter().zip(fields).all(|((name, schema), (expected_name, ty))| name == expected_name && schema.matches_type(ty)),
            (Self::Map(key, schema), Type::Map(expected_key, ty)) => key == expected_key.as_ref() && schema.matches_type(ty),
            (Self::List(schema), Type::List(ty))
            | (Self::Optional(schema), Type::Optional(ty)) => schema.matches_type(ty),
            _ => false,
        }
    }

    fn decode(&self, evaluator: &Evaluator, value: LoweredValue, path: &str, span: Span) -> Result<LoweredValue, RuntimeError> {
        let failure = |message: String| RuntimeError::new("schema", format!("schema check failed at {path}: {message}")).with_span(span);
        match self {
            Self::Validate(ty) => {
                if super::lowered_run::lowered_value_satisfies_require(evaluator, &value, ty) {
                    Ok(value)
                } else {
                    Err(failure(format!("expected {ty}, found {}", value.type_name())))
                }
            }
            Self::WireEnum(mapping) => {
                if let LoweredValue::Tag(tag) = &value {
                    if tag.type_name == mapping.type_name && tag.fields.is_empty()
                        && mapping.variants.contains_key(&Name::intern(tag.name.as_ref())) {
                        return Ok(value);
                    }
                }
                if let Some(text) = super::lowered_ops::lowered_str_value(&value) {
                    if let Some((variant, _)) = mapping.variants.iter().find(|(_, wire)| wire.as_ref() == text) {
                        return Ok(LoweredValue::Tag(Box::new(LoweredTagValue {
                            type_name: mapping.type_name,
                            name: Arc::from(variant.as_str().as_str()),
                            fields: Vec::new(),
                            wire: Some(mapping.clone()),
                        })));
                    }
                    return Err(failure(format!("unknown wire string {text:?} for {}", mapping.type_name)));
                }
                Err(failure(format!("expected {}, found {}", mapping.type_name, value.type_name())))
            }
            Self::Optional(schema) => {
                if matches!(value, LoweredValue::Null) { Ok(value) } else { schema.decode(evaluator, value, path, span) }
            }
            Self::Record(fields) => {
                let mut converted = Vec::with_capacity(fields.len());
                for (field, schema) in fields {
                    let field_path = if path == "$" { field.to_string() } else { format!("{path}.{field}") };
                    let selected = super::lowered_run::lowered_record_field_value(&value, &field.as_str())
                        .ok_or_else(|| failure(format!("missing required field {field}")))?;
                    converted.push((*field, schema.decode(evaluator, selected, &field_path, span)?));
                }
                let original = match value {
                    LoweredValue::Record(fields) | LoweredValue::Module(fields) => super::lower::take_shared(fields).into_iter()
                        .map(|(name, value)| (Name::intern(name.as_ref()), value)).collect(),
                    LoweredValue::RecordVec(fields) => super::lower::take_shared(fields),
                    LoweredValue::Stats { blanks, code, comments } => super::lowered_inline_stats_to_record_vec(blanks, code, comments),
                    LoweredValue::StatsBlob(stats) => stats.to_record_vec(),
                    value => return Err(failure(format!("expected Record, found {}", value.type_name()))),
                };
                // Required fields occupy the prepared schema's numeric slots.
                // Extra fields remain visible after that prefix, and decoded
                // nested values carry their own prepared layouts and mappings.
                let required = fields.iter().map(|(name, _)| *name).collect::<std::collections::BTreeSet<_>>();
                converted.extend(original.into_iter().filter(|(name, _)| !required.contains(name)));
                Ok(LoweredValue::RecordVec(Arc::new(converted)))
            }
            Self::List(schema) => {
                let items = match value {
                    LoweredValue::List(items) => items,
                    LoweredValue::SharedList(items) => super::lower::take_shared(items),
                    value => return Err(failure(format!("expected List, found {}", value.type_name()))),
                };
                let mut converted = Vec::with_capacity(items.len());
                for (index, item) in items.into_iter().enumerate() {
                    converted.push(schema.decode(evaluator, item, &format!("{path}[{index}]"), span)?);
                }
                Ok(LoweredValue::List(converted))
            }
            Self::Map(key_type, schema) => {
                let items = match value {
                    LoweredValue::Map(items) => super::lower::take_shared(items),
                    LoweredValue::Record(items) if *key_type == Type::Str && schema.converts_wire() => super::lower::take_shared(items).into_iter()
                        .map(|(name, value)| (name.to_string().into(), value)).collect(),
                    LoweredValue::RecordVec(items) if *key_type == Type::Str && schema.converts_wire() => super::lower::take_shared(items).into_iter()
                        .map(|(name, value)| (name.to_string().into(), value)).collect(),
                    value => return Err(failure(format!("expected Map, found {}", value.type_name()))),
                };
                let mut converted = std::collections::BTreeMap::new();
                for (key, item) in items {
                    let item_path = match &key {
                        crate::map_key::MapKey::Str(text) => format!("{path}[{text:?}]"),
                        crate::map_key::MapKey::Int(value) => format!("{path}[{value}]"),
                        crate::map_key::MapKey::Bool(value) => format!("{path}[{value}]"),
                        crate::map_key::MapKey::Duration(millis) => format!("{path}[{millis}ms]"),
                        _ => format!("{path}[{key:?}]"),
                    };
                    if !super::map_key_matches_type(&key, key_type) {
                        return Err(failure(format!("expected {key_type} key at {item_path}")));
                    }
                    converted.insert(key, schema.decode(evaluator, item, &item_path, span)?);
                }
                Ok(LoweredValue::Map(Arc::new(converted)))
            }
        }
    }
}

pub(super) fn require_value(evaluator: &Evaluator, value: LoweredValue, check: &LoweredTypeCheck, span: Span) -> LoweredValue {
    let result = if let Some(schema) = &check.schema {
        schema.decode(evaluator, value, "$", span)
    } else if super::lowered_run::lowered_value_satisfies_require(evaluator, &value, &check.ty) {
        Ok(value)
    } else {
        Err(RuntimeError::new("schema", format!("schema check failed: expected {}, found {}", check.name, value.type_name())).with_span(span))
    };
    match result {
        Ok(value) => LoweredValue::ResultOk(Box::new(value)),
        Err(error) => LoweredValue::ResultErr(Box::new(super::Value::Error(Box::new(error)))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn prepared_record_schema_rejects_reordered_projection_layout() {
        let symbols = crate::symbol::SymbolOwner::new();
        symbols.with_current(|| {
            let ty = Type::Record(BTreeMap::from([(Name::intern("first"), Type::Int), (Name::intern("second"), Type::Str)]));
            let schema = PreparedSchema::compile(&ty, &PreparedWireEnums::default());
            assert!(schema.matches_type(&ty));
            let PreparedSchema::Record(fields) = schema.as_ref() else { unreachable!() };
            let mut fields = fields.clone();
            fields.reverse();
            let forged = PreparedSchema::Record(fields);
            assert!(!forged.matches_type(&ty), "the same field set cannot authorize another physical projection order");
        });
    }

    #[test]
    fn prepared_record_schema_preserves_nested_values_and_extra_fields_in_its_layout() {
        let symbols = crate::symbol::SymbolOwner::new();
        symbols.with_current(|| {
            let required = Name::intern("required");
            let payload = Name::intern("payload");
            let schema = PreparedSchema::compile(&Type::Record(BTreeMap::from([
                (required, Type::Int),
                (payload, Type::Record(BTreeMap::from([(required, Type::Int)]))),
            ])), &PreparedWireEnums::default());
            let raw = LoweredValue::Record(Arc::new(BTreeMap::from([
                (Arc::from("aardvark"), LoweredValue::Bool(true)),
                (Arc::from("required"), LoweredValue::Int(7)),
                (Arc::from("payload"), LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("aardvark"), LoweredValue::Str(Arc::from("extra"))),
                    (Arc::from("required"), LoweredValue::Int(9)),
                ])))),
            ])));
            let evaluator = Evaluator::new(Vec::new());
            let span = Span::new(crate::source::SourceId::new(0), 0, 0);
            let expected = raw.clone().into_value();
            let decoded = schema.decode(&evaluator, raw, "$", span).unwrap();
            assert_eq!(decoded.clone().into_value(), expected);
            let LoweredValue::RecordVec(fields) = &decoded else { panic!("a checked record must retain the prepared field layout"); };
            let PreparedSchema::Record(prepared) = schema.as_ref() else { unreachable!() };
            assert_eq!(fields.iter().take(prepared.len()).map(|(name, _)| *name).collect::<Vec<_>>(), prepared.iter().map(|(name, _)| *name).collect::<Vec<_>>());
            let nested = fields.iter().find(|(name, _)| *name == payload).unwrap();
            let LoweredValue::RecordVec(nested) = &nested.1 else { panic!("nested validation must retain its converted layout"); };
            assert_eq!(nested[0], (required, LoweredValue::Int(9)));
            assert_eq!(fields.len(), 3);
            assert_eq!(nested.len(), 2);
            assert_eq!(schema.decode(&evaluator, decoded.clone(), "$", span).unwrap(), decoded);
        });
    }

    #[test]
    fn prepared_record_schema_keeps_compact_statistics_fields() {
        let symbols = crate::symbol::SymbolOwner::new();
        symbols.with_current(|| {
            let code = Name::intern("code");
            let schema = PreparedSchema::compile(&Type::Record(BTreeMap::from([(code, Type::Int)])), &PreparedWireEnums::default());
            let evaluator = Evaluator::new(Vec::new());
            let span = Span::new(crate::source::SourceId::new(0), 0, 0);
            for value in [
                LoweredValue::Stats { blanks: 2, code: 7, comments: 3 },
                LoweredValue::StatsBlob(Box::new(super::super::LoweredStatsValue {
                    blanks: 2, code: 7, comments: 3,
                    blobs: BTreeMap::from([(crate::map_key::MapKey::Str(Arc::from("rust")), LoweredValue::Int(5))]),
                })),
            ] {
                let expected = value.clone().into_value();
                let decoded = schema.decode(&evaluator, value, "$", span).unwrap();
                assert_eq!(decoded.clone().into_value(), expected);
                let LoweredValue::RecordVec(fields) = decoded else { panic!("synthesized record fields need the checked layout"); };
                assert_eq!(fields[0], (code, LoweredValue::Int(7)));
                assert_eq!(fields.len(), 4);
            }
        });
    }
}
