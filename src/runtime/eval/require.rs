use super::ValueView;
use super::{
    Evaluator, LoweredTagValue, LoweredTypeCheck, LoweredValue, Name, RuntimeError, Span, Type,
};
use crate::sema::wire_enums::{PreparedWireEnums, WireEnumMapping};
use std::sync::Arc;
use crate::syntax::arena::UnknownFields;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PreparedSchema {
    UnknownFields(UnknownFields, Arc<PreparedSchema>),
    Validate(Type),
    Record(Vec<(Name, Arc<PreparedSchema>)>),
    List(Arc<PreparedSchema>),
    Map(Type, Arc<PreparedSchema>),
    Optional(Arc<PreparedSchema>),
    WireEnum(Arc<WireEnumMapping>),
    Union(Type, Vec<Arc<PreparedSchema>>),
    Validated(Type, Arc<PreparedSchema>),
    Set(Type),
}

impl PreparedSchema {
    pub(super) fn compile(ty: &Type, enums: &PreparedWireEnums) -> Arc<Self> {
        Self::compile_policy(ty, enums, UnknownFields::Preserve)
    }

    pub(super) fn compile_boundary(ty: &Type, enums: &PreparedWireEnums, policy: UnknownFields) -> Arc<Self> {
        let schema = Self::compile_policy(ty, enums, policy);
        if policy == UnknownFields::Preserve { schema } else { Arc::new(Self::UnknownFields(policy, schema)) }
    }

    pub(super) fn unknown_fields(&self) -> UnknownFields {
        match self { Self::UnknownFields(policy, _) => *policy, _ => UnknownFields::Preserve }
    }

    fn compile_policy(ty: &Type, enums: &PreparedWireEnums, policy: UnknownFields) -> Arc<Self> {
        Arc::new(match ty {
            Type::Record(fields) if !fields.is_empty() || policy != UnknownFields::Preserve => Self::Record(
                fields
                    .iter()
                    .map(|(name, ty)| (*name, Self::compile_policy(ty, enums, policy)))
                    .collect(),
            ),
            Type::List(item) => Self::List(Self::compile_policy(item, enums, policy)),
            Type::Map(key, item) => Self::Map((**key).clone(), Self::compile_policy(item, enums, policy)),
            Type::Set(element) => Self::Set((**element).clone()),
            Type::Optional(item) => Self::Optional(Self::compile_policy(item, enums, policy)),
            Type::Union(members) => {
                let schemas = members
                    .iter()
                    .map(|member| Self::compile_policy(member, enums, policy))
                    .collect::<Vec<_>>();
                if policy != UnknownFields::Preserve || schemas.iter().any(|schema| schema.rebuilds_value()) {
                    Self::Union(ty.clone(), schemas)
                } else {
                    Self::Validate(ty.clone())
                }
            }
            Type::Validated(validated) => {
                Self::Validated(ty.clone(), Self::compile_policy(validated.base(), enums, policy))
            }
            Type::Tag(name) if enums.mappings.contains_key(name) => {
                Self::WireEnum(enums.mappings[name].clone())
            }
            ty => Self::Validate(ty.clone()),
        })
    }

    fn converts_wire(&self) -> bool {
        match self {
            Self::UnknownFields(_, _) | Self::WireEnum(_) | Self::Set(_) => true,
            Self::Record(fields) => fields.iter().any(|(_, schema)| schema.converts_wire()),
            Self::List(schema) | Self::Map(_, schema) | Self::Optional(schema) | Self::Validated(_, schema) => schema.converts_wire(),
            Self::Union(_, members) => members.iter().any(|schema| schema.converts_wire()),
            Self::Validate(_) => false,
        }
    }

    fn rebuilds_value(&self) -> bool {
        match self {
            Self::UnknownFields(_, _) | Self::WireEnum(_) | Self::Set(_) | Self::List(_) => true,
            Self::Record(fields) => fields.iter().any(|(_, schema)| schema.rebuilds_value()),
            Self::Map(_, schema)
            | Self::Optional(schema)
            | Self::Validated(_, schema) => schema.rebuilds_value(),
            Self::Union(_, members) => members.iter().any(|schema| schema.rebuilds_value()),
            Self::Validate(_) => false,
        }
    }

    pub(super) fn valid(&self) -> bool {
        match self {
            Self::UnknownFields(policy, schema) => *policy != UnknownFields::Preserve && schema.valid() && schema.policy_complete(),
            Self::WireEnum(mapping) => {
                !mapping.variants.is_empty()
                    && mapping
                        .variants
                        .values()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == mapping.variants.len()
            }
            Self::Record(fields) => fields.iter().all(|(_, schema)| schema.valid()),
            Self::Map(key, schema) => key.is_map_key() && schema.valid(),
            Self::Set(element) => element.is_map_key(),
            Self::List(schema) | Self::Optional(schema) | Self::Validated(_, schema) => {
                schema.valid()
            }
            Self::Union(_, members) => members.iter().all(|schema| schema.valid()),
            Self::Validate(_) => true,
        }
    }

    fn policy_complete(&self) -> bool {
        fn opaque(ty: &Type) -> bool {
            match ty {
                Type::Record(_) => false,
                Type::List(item) | Type::Optional(item) => opaque(item),
                Type::Map(_, item) => opaque(item),
                Type::Validated(validated) => opaque(validated.base()),
                Type::Union(members) => members.iter().all(opaque),
                _ => true,
            }
        }
        match self {
            Self::UnknownFields(_, _) => false,
            Self::Validate(ty) => opaque(ty),
            Self::Record(fields) => fields.iter().all(|(_, schema)| schema.policy_complete()),
            Self::List(schema) | Self::Optional(schema) | Self::Map(_, schema) | Self::Validated(_, schema) => schema.policy_complete(),
            Self::Union(_, members) => members.iter().all(|schema| schema.policy_complete()),
            Self::WireEnum(_) | Self::Set(_) => true,
        }
    }

    pub(super) fn visit_wire_mappings(
        &self,
        visit: &mut impl FnMut(&Arc<WireEnumMapping>) -> bool,
    ) -> bool {
        match self {
            Self::UnknownFields(_, schema) => schema.visit_wire_mappings(visit),
            Self::WireEnum(mapping) => visit(mapping),
            Self::Record(fields) => fields
                .iter()
                .all(|(_, schema)| schema.visit_wire_mappings(visit)),
            Self::List(schema)
            | Self::Map(_, schema)
            | Self::Optional(schema)
            | Self::Validated(_, schema) => schema.visit_wire_mappings(visit),
            Self::Union(_, members) => members
                .iter()
                .all(|schema| schema.visit_wire_mappings(visit)),
            Self::Validate(_) | Self::Set(_) => true,
        }
    }

    pub(super) fn matches_type(&self, ty: &Type) -> bool {
        match (self, ty) {
            (Self::UnknownFields(_, schema), actual) => schema.matches_type(actual),
            (Self::Validate(expected), actual) => expected == actual,
            (Self::Union(expected, schemas), actual) => {
                expected == actual
                    && matches!(expected, Type::Union(members)
                        if members.len() == schemas.len()
                            && schemas
                                .iter()
                                .zip(members)
                                .all(|(schema, member)| schema.matches_type(member)))
            }
            (Self::Validated(expected, schema), actual) => {
                expected == actual
                    && matches!(expected, Type::Validated(validated)
                        if schema.matches_type(validated.base()))
            }
            (Self::WireEnum(mapping), Type::Tag(name)) => mapping.type_name == *name,
            (Self::Record(schemas), Type::Record(fields)) => {
                schemas.len() == fields.len()
                    && schemas.iter().zip(fields.iter()).all(|((schema_name, schema), (name, ty))| {
                        schema_name == name && schema.matches_type(ty)
                    })
            }
            (Self::Map(key, schema), Type::Map(expected_key, ty)) => {
                key == expected_key.as_ref() && schema.matches_type(ty)
            }
            (Self::Set(element), Type::Set(expected)) => element == expected.as_ref(),
            (Self::List(schema), Type::List(ty)) | (Self::Optional(schema), Type::Optional(ty)) => {
                schema.matches_type(ty)
            }
            _ => false,
        }
    }

    fn decode(
        &self,
        evaluator: &Evaluator,
        value: LoweredValue,
        path: &str,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        self.decode_policy(evaluator, value, path, span, UnknownFields::Preserve)
    }

    fn decode_policy(&self, evaluator: &Evaluator, value: LoweredValue, path: &str, span: Span, policy: UnknownFields) -> Result<LoweredValue, RuntimeError> {
        let failure = |message: String| {
            RuntimeError::new(
                "schema",
                format!("schema check failed at {path}: {message}"),
            )
            .with_span(span)
        };
        match self {
            Self::UnknownFields(policy, schema) => schema.decode_policy(evaluator, value, path, span, *policy),
            Self::Validate(ty) => {
                if let Some(checked) = require_module_contract(evaluator, &value, ty, path, span) {
                    checked.map(|()| value)
                } else if super::type_view::value_matches_runtime_type(ValueView::Lowered(&value), ty) {
                    Ok(value)
                } else {
                    Err(failure(format!(
                        "expected {ty}, found {}",
                        value.type_name()
                    )))
                }
            }
            Self::WireEnum(mapping) => {
                if let LoweredValue::Tag(tag) = &value
                    && tag.type_name == mapping.type_name
                    && tag.fields.is_empty()
                    && mapping
                        .variants
                        .contains_key(&Name::intern(tag.name.as_ref()))
                {
                    return Ok(value);
                }
                if let Some(text) = super::lowered_ops::lowered_str_value(&value) {
                    if let Some((variant, _)) = mapping
                        .variants
                        .iter()
                        .find(|(_, wire)| wire.as_ref() == text)
                    {
                        return Ok(LoweredValue::Tag(Box::new(LoweredTagValue {
                            type_name: mapping.type_name,
                            name: Arc::from(variant.as_str().as_str()),
                            fields: Vec::new(),
                            wire: Some(mapping.clone()),
                        })));
                    }
                    return Err(failure(format!(
                        "unknown wire string {text:?} for {}",
                        mapping.type_name
                    )));
                }
                Err(failure(format!(
                    "expected {}, found {}",
                    mapping.type_name,
                    value.type_name()
                )))
            }
            Self::Optional(schema) => {
                if matches!(value, LoweredValue::Null) {
                    Ok(value)
                } else {
                    schema.decode_policy(evaluator, value, path, span, policy)
                }
            }
            Self::Validated(ty, schema) => {
                let decoded = schema.decode_policy(evaluator, value, path, span, policy)?;
                match ty {
                    Type::Validated(validated)
                        if !super::validated::lowered_value_passes(
                            validated.validation(),
                            &decoded,
                        ) =>
                    {
                        Err(failure(format!(
                            "expected {ty}, found {}",
                            super::validated::lowered_failure(validated.validation(), &decoded)
                        )))
                    }
                    _ => Ok(decoded),
                }
            }
            Self::Union(ty, members) => {
                let mut decoded = None;
                crate::sema::types::first_accepting_union_member(members, |schema| {
                    decoded = schema.decode_policy(evaluator, value.clone(), path, span, policy).ok();
                    decoded.is_some()
                });
                decoded
                    .ok_or_else(|| failure(format!("expected {ty}, found {}", value.type_name())))
            }
            Self::Record(fields) => {
                let value = match value {
                    LoweredValue::Stats { blanks, code, comments } if policy != UnknownFields::Preserve =>
                        LoweredValue::RecordVec(Arc::new(super::lowered_inline_stats_to_record_vec(blanks, code, comments))),
                    LoweredValue::StatsBlob(stats) if policy != UnknownFields::Preserve =>
                        LoweredValue::RecordVec(Arc::new(stats.to_record_vec())),
                    value => value,
                };
                if fields.is_empty() && !matches!(value, LoweredValue::Record(_) | LoweredValue::RecordVec(_)) {
                    return Err(failure(format!("expected Record, found {}", value.type_name())));
                }
                let mut value = value;
                if policy == UnknownFields::Reject {
                    let unexpected = match &value {
                        LoweredValue::Record(values) => values.keys().find(|name| !fields.iter().any(|(field, _)| field.as_str().as_str() == name.as_ref())).map(|name| name.to_string()),
                        LoweredValue::RecordVec(values) => values.iter().map(|(name, _)| name).find(|name| !fields.iter().any(|(field, _)| field == *name)).map(|name| name.to_string()),
                        _ => None,
                    };
                    if let Some(field) = unexpected {
                        let field_path = if path == "$" { field.to_string() } else { format!("{path}.{field}") };
                        return Err(RuntimeError::new("schema", format!("schema check failed at {field_path}: unexpected field {field}")).with_span(span));
                    }
                }
                let mut projected = if policy == UnknownFields::Strip { Vec::with_capacity(fields.len()) } else { Vec::new() };
                for (field, schema) in fields {
                    let field_path = if path == "$" {
                        field.to_string()
                    } else {
                        format!("{path}.{field}")
                    };
                    let selected =
                        super::lowered_run::lowered_record_field_value(&value, &field.as_str())
                            .ok_or_else(|| failure(format!("missing required field {field}")))?;
                    let converted = schema.decode_policy(evaluator, selected, &field_path, span, policy)?;
                    if policy == UnknownFields::Strip {
                        projected.push((*field, converted));
                    } else if policy != UnknownFields::Preserve || schema.rebuilds_value() {
                        *super::lowered_ops::lowered_record_field_mut(&mut value, *field, span)? =
                            converted;
                    }
                }
                if policy == UnknownFields::Strip { Ok(LoweredValue::RecordVec(Arc::new(projected))) } else { Ok(value) }
            }
            Self::List(schema) => {
                let items = match value {
                    LoweredValue::EnvPathList(paths) => paths.entries().iter().cloned().map(LoweredValue::Path).collect(),
                    LoweredValue::List(items) => items,
                    LoweredValue::SharedList(items) => super::lower::take_shared(items),
                    value => {
                        return Err(failure(format!(
                            "expected List, found {}",
                            value.type_name()
                        )));
                    }
                };
                let mut converted = Vec::with_capacity(items.len());
                for (index, item) in items.into_iter().enumerate() {
                    converted.push(schema.decode_policy(
                        evaluator,
                        item,
                        &format!("{path}[{index}]"),
                        span,
                        policy,
                    )?);
                }
                Ok(LoweredValue::List(converted))
            }
            Self::Set(element_type) => {
                let items = match value {
                    LoweredValue::Set(elements) => {
                        return if elements
                            .iter()
                            .all(|element| super::map_key_matches_type(element, element_type))
                        {
                            Ok(LoweredValue::Set(elements))
                        } else {
                            Err(failure(format!("expected Set[{element_type}]")))
                        };
                    }
                    LoweredValue::EnvPathList(paths) => paths.entries().iter().cloned().map(LoweredValue::Path).collect(),
                    LoweredValue::List(items) => items,
                    LoweredValue::SharedList(items) => super::lower::take_shared(items),
                    value => {
                        return Err(failure(format!(
                            "expected Set, found {}",
                            value.type_name()
                        )));
                    }
                };
                let mut elements = std::collections::BTreeSet::new();
                for (index, item) in items.iter().enumerate() {
                    let element = super::lowered_ops::lowered_map_key_ref(item, span)
                        .ok()
                        .map(|element| element.to_owned())
                        .filter(|element| super::map_key_matches_type(element, element_type))
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "schema",
                                format!(
                                    "schema check failed at {path}[{index}]: expected {element_type}, found {}",
                                    item.type_name()
                                ),
                            )
                            .with_span(span)
                        })?;
                    if !elements.insert(element) {
                        return Err(RuntimeError::new(
                            "schema",
                            format!(
                                "schema check failed at {path}[{index}]: a set holds each element once, and this one repeats an earlier element"
                            ),
                        )
                        .with_span(span));
                    }
                }
                Ok(LoweredValue::Set(Arc::new(elements)))
            }
            Self::Map(key_type, schema) => {
                let items = match value {
                    LoweredValue::Map(items) => super::lower::take_shared(items),
                    LoweredValue::Record(items)
                        if *key_type == Type::Str && schema.converts_wire() =>
                    {
                        super::lower::take_shared(items)
                            .into_iter()
                            .map(|(name, value)| (name.to_string().into(), value))
                            .collect()
                    }
                    LoweredValue::RecordVec(items)
                        if *key_type == Type::Str && schema.converts_wire() =>
                    {
                        super::lower::take_shared(items)
                            .into_iter()
                            .map(|(name, value)| (name.to_string().into(), value))
                            .collect()
                    }
                    value => {
                        return Err(failure(format!(
                            "expected Map, found {}",
                            value.type_name()
                        )));
                    }
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
                    converted.insert(key, schema.decode_policy(evaluator, item, &item_path, span, policy)?);
                }
                Ok(LoweredValue::Map(Arc::new(converted)))
            }
        }
    }
}

/// The contract check for a module value required as a module contract, or
/// `None` when the pair is not a module and a contract and the ordinary type
/// test applies.
pub(super) fn require_module_contract(
    evaluator: &Evaluator,
    value: &LoweredValue,
    ty: &Type,
    path: &str,
    span: Span,
) -> Option<Result<(), RuntimeError>> {
    match (value, ty) {
        (LoweredValue::Module(module), Type::Module(contract)) => {
            Some(super::module_contract::require_module_contract(
                evaluator, module, contract, path, span,
            ))
        }
        _ => None,
    }
}

pub(super) fn require_value(
    evaluator: &Evaluator,
    value: LoweredValue,
    check: &LoweredTypeCheck,
    span: Span,
) -> LoweredValue {
    let result = if let Some(schema) = &check.schema {
        schema.decode(evaluator, value, "$", span)
    } else if let Some(checked) = require_module_contract(evaluator, &value, &check.ty, "$", span) {
        checked.map(|()| value)
    } else if super::type_view::value_matches_runtime_type(ValueView::Lowered(&value), &check.ty) {
        Ok(value.materialize_validated_list(&check.ty))
    } else {
        Err(RuntimeError::new(
            "schema",
            format!(
                "schema check failed: expected {}, found {}",
                check.name,
                value.type_name()
            ),
        )
        .with_span(span))
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
    fn record_policies_cover_specialized_stats_representations() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let evaluator = Evaluator::new(Vec::new());
            let enums = PreparedWireEnums::default();
            let span = Span::new(crate::source::SourceId::new(0), 0, 0);
            let code = Name::intern("code");
            let subset = Type::Record(BTreeMap::from([(code, Type::Int)]).into());
            let empty = Type::Record(BTreeMap::new().into());
            let complete = Type::Record(BTreeMap::from([
                (Name::intern("blanks"), Type::Int),
                (Name::intern("blobs"), Type::Map(Box::new(Type::Str), Box::new(Type::Int))),
                (code, Type::Int),
                (Name::intern("comments"), Type::Int),
            ]).into());
            let values = [
                LoweredValue::Stats { blanks: 1, code: 2, comments: 3 },
                LoweredValue::StatsBlob(Box::new(super::super::LoweredStatsValue {
                    blanks: 1,
                    blobs: BTreeMap::from([(crate::map_key::MapKey::Str(Arc::from("retained")), LoweredValue::Int(4))]),
                    code: 2,
                    comments: 3,
                })),
            ];
            for raw in values {
                let original = raw.clone();
                let preserved = PreparedSchema::compile_boundary(&subset, &enums, UnknownFields::Preserve)
                    .decode(&evaluator, raw.clone(), "$", span).unwrap();
                assert_eq!(preserved, original);
                let rejected = PreparedSchema::compile_boundary(&subset, &enums, UnknownFields::Reject)
                    .decode(&evaluator, raw.clone(), "$", span).unwrap_err();
                assert_eq!(rejected.kind, "schema");
                assert!(rejected.message.contains("blanks: unexpected field blanks"), "{}", rejected.message);
                let stripped = PreparedSchema::compile_boundary(&subset, &enums, UnknownFields::Strip)
                    .decode(&evaluator, raw.clone(), "$", span).unwrap();
                assert_eq!(stripped, LoweredValue::RecordVec(Arc::new(vec![(code, LoweredValue::Int(2))])));
                let exact = PreparedSchema::compile_boundary(&complete, &enums, UnknownFields::Reject)
                    .decode(&evaluator, raw.clone(), "$", span).unwrap();
                assert_eq!(exact, original);
                let empty_projection = PreparedSchema::compile_boundary(&empty, &enums, UnknownFields::Strip)
                    .decode(&evaluator, raw.clone(), "$", span).unwrap();
                assert_eq!(empty_projection, LoweredValue::RecordVec(Arc::new(Vec::new())));
                let rejected_empty = PreparedSchema::compile_boundary(&empty, &enums, UnknownFields::Reject)
                    .decode(&evaluator, raw.clone(), "$", span).unwrap_err();
                assert!(rejected_empty.message.contains("blanks: unexpected field blanks"), "{}", rejected_empty.message);
                assert_eq!(raw, original);
            }
        });
    }
}
