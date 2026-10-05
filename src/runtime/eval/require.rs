use super::{
    Evaluator, LoweredTagValue, LoweredTypeCheck, LoweredValue, Name, RuntimeError, Span, Type,
};
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
    /// A union with a member that converts wire strings. Decoding tries the
    /// member schemas in the order the union lists them and publishes the
    /// first that accepts. A union no member of which converts is a plain
    /// `Validate`, which asks the same question without rebuilding the value.
    Union(Type, Vec<Arc<PreparedSchema>>),
    /// A validated type and the schema of its base. Decoding checks the base
    /// first, so a failure inside the value is reported at its own path, and
    /// then tests the validation once on the decoded value.
    Validated(Type, Arc<PreparedSchema>),
    /// A set of the element type. Decoding accepts a set, or a list (a JSON
    /// array) whose elements are distinct; a repeated element is a failure,
    /// since a validation boundary does not drop data.
    Set(Type),
}

impl PreparedSchema {
    pub(super) fn compile(ty: &Type, enums: &PreparedWireEnums) -> Arc<Self> {
        Arc::new(match ty {
            Type::Record(fields) if !fields.is_empty() => Self::Record(
                fields
                    .iter()
                    .map(|(name, ty)| (*name, Self::compile(ty, enums)))
                    .collect(),
            ),
            Type::List(item) => Self::List(Self::compile(item, enums)),
            Type::Map(key, item) => Self::Map((**key).clone(), Self::compile(item, enums)),
            Type::Set(element) => Self::Set((**element).clone()),
            Type::Optional(item) => Self::Optional(Self::compile(item, enums)),
            Type::Union(members) => {
                let schemas = members
                    .iter()
                    .map(|member| Self::compile(member, enums))
                    .collect::<Vec<_>>();
                if schemas.iter().any(|schema| schema.converts_wire()) {
                    Self::Union(ty.clone(), schemas)
                } else {
                    Self::Validate(ty.clone())
                }
            }
            Type::Validated(validated) => {
                Self::Validated(ty.clone(), Self::compile(validated.base(), enums))
            }
            Type::Tag(name) if enums.mappings.contains_key(name) => {
                Self::WireEnum(enums.mappings[name].clone())
            }
            ty => Self::Validate(ty.clone()),
        })
    }

    fn converts_wire(&self) -> bool {
        match self {
            // A list is rebuilt as a set.
            Self::WireEnum(_) | Self::Set(_) => true,
            Self::Record(fields) => fields.iter().any(|(_, schema)| schema.converts_wire()),
            Self::List(schema)
            | Self::Map(_, schema)
            | Self::Optional(schema)
            | Self::Validated(_, schema) => schema.converts_wire(),
            Self::Union(_, members) => members.iter().any(|schema| schema.converts_wire()),
            Self::Validate(_) => false,
        }
    }

    pub(super) fn valid(&self) -> bool {
        match self {
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

    pub(super) fn visit_wire_mappings(
        &self,
        visit: &mut impl FnMut(&Arc<WireEnumMapping>) -> bool,
    ) -> bool {
        match self {
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
            (Self::Validate(expected), actual) => expected == actual,
            // The member schemas are tried in the order the type lists its
            // members, so each has to be the schema of the member beside it.
            (Self::Union(expected, schemas), actual) => {
                expected == actual
                    && matches!(expected, Type::Union(members)
                        if members.len() == schemas.len()
                            && schemas
                                .iter()
                                .zip(members)
                                .all(|(schema, member)| schema.matches_type(member)))
            }
            // The base schema decodes the value the validation then tests,
            // so it has to be the schema of this type's own base.
            (Self::Validated(expected, schema), actual) => {
                expected == actual
                    && matches!(expected, Type::Validated(validated)
                        if schema.matches_type(validated.base()))
            }
            (Self::WireEnum(mapping), Type::Tag(name)) => mapping.type_name == *name,
            (Self::Record(schemas), Type::Record(fields)) => {
                schemas.len() == fields.len()
                    && schemas.iter().all(|(name, schema)| {
                        fields.get(name).is_some_and(|ty| schema.matches_type(ty))
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
        let failure = |message: String| {
            RuntimeError::new(
                "schema",
                format!("schema check failed at {path}: {message}"),
            )
            .with_span(span)
        };
        match self {
            Self::Validate(ty) => {
                if let Some(checked) = require_module_contract(evaluator, &value, ty, path, span) {
                    checked.map(|()| value)
                } else if super::lowered_value_matches_static_type(&value, ty) {
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
                    schema.decode(evaluator, value, path, span)
                }
            }
            Self::Validated(ty, schema) => {
                let decoded = schema.decode(evaluator, value, path, span)?;
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
                    decoded = schema.decode(evaluator, value.clone(), path, span).ok();
                    decoded.is_some()
                });
                decoded.ok_or_else(|| {
                    failure(format!("expected {ty}, found {}", value.type_name()))
                })
            }
            Self::Record(fields) => {
                let mut value = value;
                for (field, schema) in fields {
                    let field_path = if path == "$" {
                        field.to_string()
                    } else {
                        format!("{path}.{field}")
                    };
                    let selected =
                        super::lowered_run::lowered_record_field_value(&value, &field.as_str())
                            .ok_or_else(|| failure(format!("missing required field {field}")))?;
                    let converted = schema.decode(evaluator, selected, &field_path, span)?;
                    if schema.converts_wire() {
                        *super::lowered_ops::lowered_record_field_mut(&mut value, *field, span)? =
                            converted;
                    }
                }
                Ok(value)
            }
            Self::List(schema) => {
                let items = match value {
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
                    converted.push(schema.decode(
                        evaluator,
                        item,
                        &format!("{path}[{index}]"),
                        span,
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
                    converted.insert(key, schema.decode(evaluator, item, &item_path, span)?);
                }
                Ok(LoweredValue::Map(Arc::new(converted)))
            }
        }
    }
}

/// The contract check for a module value required as a module contract, or
/// `None` when the pair is not a module and a contract and the ordinary type
/// test applies.
fn require_module_contract(
    evaluator: &Evaluator,
    value: &LoweredValue,
    ty: &Type,
    path: &str,
    span: Span,
) -> Option<Result<(), RuntimeError>> {
    match (value, ty) {
        (LoweredValue::Module(module), Type::Module(contract)) => Some(
            super::module_contract::require_module_contract(evaluator, module, contract, path, span),
        ),
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
    } else if let Some(checked) = require_module_contract(evaluator, &value, &check.ty, "$", span)
    {
        checked.map(|()| value)
    } else if super::lowered_value_matches_static_type(&value, &check.ty) {
        Ok(value)
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
