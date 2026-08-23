use crate::symbol::{Name, Symbol};
use crate::syntax::arena::{ArenaTypeExprTag, AstArena, TypeExprId};
use crate::syntax::node::Effect;
use std::collections::BTreeMap;
use std::fmt;
use xsh_registry::types::BuiltinTypeName;

fn btree_map<K: Ord, V>(entries: Vec<(K, V)>) -> BTreeMap<K, V> {
    let mut map = BTreeMap::new();
    map.extend(entries);
    map
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    Any,
    Unknown,
    Invalid,
    Null,
    Bool,
    Int,
    Float,
    Duration,
    Str,
    Bytes,
    Digest,
    Regex,
    Path,
    List(Box<Type>),
    Map(Box<Type>),
    Stream(Box<Type>),
    Record(BTreeMap<Name, Type>),
    Module(BTreeMap<Name, ModuleExportType>),
    DynamicModule,
    Result(Box<Type>, Box<Type>),
    Status,
    EnvPathList,
    Error,
    ErrorFamily(Name),
    ErrorVariant { family: Name, variant: Name },
    ErrorFacet(Name),
    ProcessError,
    Pure,
    Proc,
    Command,
    ProcessHandle,
    NetJob,
    Unit,
    Tag(Name),
    Optional(Box<Type>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModuleExportType {
    Value { ty: Type, optional: bool },
    Proc { sig: CallableType, optional: bool },
    Pure { sig: CallableType, optional: bool },
}

impl ModuleExportType {
    pub fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            + match self {
                Self::Value { ty, .. } => ty.retained_bytes(),
                Self::Proc { sig, .. } | Self::Pure { sig, .. } => sig.retained_bytes(),
            }
    }

    pub fn optional(&self) -> bool {
        match self {
            Self::Value { optional, .. }
            | Self::Proc { optional, .. }
            | Self::Pure { optional, .. } => *optional,
        }
    }

    pub fn field_type(&self) -> Type {
        match self {
            Self::Value { ty, .. } => ty.clone(),
            Self::Proc { .. } => Type::Proc,
            Self::Pure { .. } => Type::Pure,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableType {
    pub params: Vec<CallableParamType>,
    pub return_ty: Box<Type>,
    pub effects: Option<Vec<Effect>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableParamType {
    pub name: Name,
    pub ty: Type,
    pub defaulted: bool,
    pub rest: bool,
}

impl CallableType {
    pub fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut total = size_of::<Self>()
            + self.params.capacity() * size_of::<CallableParamType>()
            + size_of::<Type>()
            + self.return_ty.retained_bytes();
        if let Some(effects) = &self.effects {
            total = total.saturating_add(effects.capacity() * size_of::<Effect>());
        }
        for param in &self.params {
            total = total.saturating_add(param.ty.retained_bytes());
        }
        total
    }
}

impl Type {
    /// Conservative owned-heap estimate for one semantic type tree.
    pub fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut total = size_of::<Self>();
        match self {
            Self::List(inner) | Self::Map(inner) | Self::Stream(inner) | Self::Optional(inner) => {
                total = total.saturating_add(size_of::<Type>() + inner.retained_bytes());
            }
            Self::Result(ok, err) => {
                total = total
                    .saturating_add(size_of::<Type>() + ok.retained_bytes())
                    .saturating_add(size_of::<Type>() + err.retained_bytes());
            }
            Self::Record(fields) => {
                total = total.saturating_add(fields.len() * size_of::<(Name, Type)>());
                for ty in fields.values() {
                    total = total.saturating_add(ty.retained_bytes());
                }
            }
            Self::Module(exports) => {
                total = total.saturating_add(exports.len() * size_of::<(Name, ModuleExportType)>());
                for export in exports.values() {
                    total = total.saturating_add(export.retained_bytes());
                }
            }
            _ => {}
        }
        total
    }

    pub fn from_arena(arena: &AstArena, id: TypeExprId) -> Self {
        let index = id.index();
        let tag = arena.type_expr_tags[index];
        let data = arena.type_expr_data[index];
        match tag {
            ArenaTypeExprTag::Named => {
                Self::from_name(&Name::from_symbol(Symbol::from_raw(data.lhs)).as_str())
            }
            ArenaTypeExprTag::Qualified => Self::Unknown,
            ArenaTypeExprTag::List => Self::List(Box::new(Self::from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
            ))),
            ArenaTypeExprTag::Map => Self::Map(Box::new(Self::from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
            ))),
            ArenaTypeExprTag::Stream => Self::Stream(Box::new(Self::from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
            ))),
            ArenaTypeExprTag::Module => {
                let inner = Self::from_arena(arena, TypeExprId::from_index(data.lhs as usize));
                Self::Module(btree_map(vec![(
                    Name::intern("<schema>"),
                    ModuleExportType::Value {
                        ty: inner,
                        optional: false,
                    },
                )]))
            }
            ArenaTypeExprTag::Result => Self::Result(
                Box::new(Self::from_arena(
                    arena,
                    TypeExprId::from_index(data.lhs as usize),
                )),
                Box::new(
                    TypeExprId::from_optional_raw(data.rhs)
                        .map_or(Self::Error, |err| Self::from_arena(arena, err)),
                ),
            ),
            ArenaTypeExprTag::Optional => Self::Optional(Box::new(Self::from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
            ))),
        }
    }

    pub fn from_name(name: &str) -> Self {
        BuiltinTypeName::parse(name).map_or(Self::Unknown, Self::from_builtin_name)
    }

    pub fn builtin_from_name(name: &str) -> Option<Self> {
        BuiltinTypeName::parse(name).map(Self::from_builtin_name)
    }

    pub fn from_builtin_name(name: BuiltinTypeName) -> Self {
        match name {
            BuiltinTypeName::Unknown => Self::Unknown,
            BuiltinTypeName::Any => Self::Any,
            BuiltinTypeName::Null => Self::Null,
            BuiltinTypeName::Bool => Self::Bool,
            BuiltinTypeName::Int | BuiltinTypeName::UInt => Self::Int,
            BuiltinTypeName::Float => Self::Float,
            BuiltinTypeName::Duration => Self::Duration,
            BuiltinTypeName::Str => Self::Str,
            BuiltinTypeName::Bytes => Self::Bytes,
            BuiltinTypeName::Digest => Self::Digest,
            BuiltinTypeName::Regex => Self::Regex,
            BuiltinTypeName::Path => Self::Path,
            BuiltinTypeName::Map => Self::Map(Box::new(Self::Unknown)),
            BuiltinTypeName::Module => Self::DynamicModule,
            BuiltinTypeName::Record => Self::Record(BTreeMap::new()),
            BuiltinTypeName::Status => Self::Status,
            BuiltinTypeName::EnvPathList => Self::EnvPathList,
            BuiltinTypeName::Error => Self::Error,
            BuiltinTypeName::ProcessError => Self::ProcessError,
            BuiltinTypeName::Pure => Self::Pure,
            BuiltinTypeName::Proc => Self::Proc,
            BuiltinTypeName::Command => Self::Command,
            BuiltinTypeName::ProcessHandle => Self::ProcessHandle,
            BuiltinTypeName::NetJob => Self::NetJob,
            BuiltinTypeName::Result => Self::Result(Box::new(Self::Unknown), Box::new(Self::Error)),
            BuiltinTypeName::Unit => Self::Unit,
        }
    }

    pub fn builtin_type_name(&self) -> Option<BuiltinTypeName> {
        match self {
            Self::Any => Some(BuiltinTypeName::Any),
            Self::Unknown => Some(BuiltinTypeName::Unknown),
            Self::Null => Some(BuiltinTypeName::Null),
            Self::Bool => Some(BuiltinTypeName::Bool),
            Self::Int => Some(BuiltinTypeName::Int),
            Self::Float => Some(BuiltinTypeName::Float),
            Self::Duration => Some(BuiltinTypeName::Duration),
            Self::Str => Some(BuiltinTypeName::Str),
            Self::Bytes => Some(BuiltinTypeName::Bytes),
            Self::Digest => Some(BuiltinTypeName::Digest),
            Self::Regex => Some(BuiltinTypeName::Regex),
            Self::Path => Some(BuiltinTypeName::Path),
            Self::Map(_) => Some(BuiltinTypeName::Map),
            Self::Module(_) | Self::DynamicModule => Some(BuiltinTypeName::Module),
            Self::Record(_) => Some(BuiltinTypeName::Record),
            Self::Status => Some(BuiltinTypeName::Status),
            Self::EnvPathList => Some(BuiltinTypeName::EnvPathList),
            Self::Error => Some(BuiltinTypeName::Error),
            Self::ProcessError => Some(BuiltinTypeName::ProcessError),
            Self::Pure => Some(BuiltinTypeName::Pure),
            Self::Proc => Some(BuiltinTypeName::Proc),
            Self::Command => Some(BuiltinTypeName::Command),
            Self::ProcessHandle => Some(BuiltinTypeName::ProcessHandle),
            Self::NetJob => Some(BuiltinTypeName::NetJob),
            Self::Result(_, _) => Some(BuiltinTypeName::Result),
            Self::Unit => Some(BuiltinTypeName::Unit),
            Self::Invalid
            | Self::List(_)
            | Self::Stream(_)
            | Self::ErrorFamily(_)
            | Self::ErrorVariant { .. }
            | Self::ErrorFacet(_)
            | Self::Tag(_)
            | Self::Optional(_) => None,
        }
    }

    pub fn is_recovery(&self) -> bool {
        matches!(self, Self::Unknown | Self::Invalid)
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::Any)
    }

    pub fn contains_any(&self) -> bool {
        match self {
            Self::Any => true,
            Self::List(inner) | Self::Map(inner) | Self::Stream(inner) | Self::Optional(inner) => {
                inner.contains_any()
            }
            Self::Result(ok, err) => ok.contains_any() || err.contains_any(),
            Self::Record(fields) => fields.values().any(Self::contains_any),
            Self::Module(exports) => exports.values().any(|export| match export {
                ModuleExportType::Value { ty, .. } => ty.contains_any(),
                ModuleExportType::Proc { sig, .. } | ModuleExportType::Pure { sig, .. } => {
                    sig.params.iter().any(|param| param.ty.contains_any())
                        || sig.return_ty.contains_any()
                }
            }),
            _ => false,
        }
    }

    pub fn any_flows_to_concrete(&self, expected: &Type) -> bool {
        if self.is_recovery() || expected.is_recovery() || expected.is_dynamic() {
            return false;
        }
        match (self, expected) {
            (Self::Any, _) => true,
            (Self::List(actual), Self::List(expected))
            | (Self::Map(actual), Self::Map(expected))
            | (Self::Stream(actual), Self::Stream(expected))
            | (Self::Optional(actual), Self::Optional(expected)) => {
                actual.any_flows_to_concrete(expected)
            }
            (Self::Result(actual_ok, actual_err), Self::Result(expected_ok, expected_err)) => {
                actual_ok.any_flows_to_concrete(expected_ok)
                    || actual_err.any_flows_to_concrete(expected_err)
            }
            (Self::Record(actual_fields), Self::Record(expected_fields))
                if !actual_fields.is_empty() && !expected_fields.is_empty() =>
            {
                expected_fields.iter().any(|(name, expected)| {
                    actual_fields
                        .get(name)
                        .is_some_and(|actual| actual.any_flows_to_concrete(expected))
                })
            }
            (Self::Module(actual_exports), Self::Module(expected_exports))
                if !actual_exports.is_empty() && !expected_exports.is_empty() =>
            {
                expected_exports.iter().any(|(name, expected)| {
                    actual_exports
                        .get(name)
                        .is_some_and(|actual| module_export_any_flows_to_concrete(actual, expected))
                })
            }
            (actual, Self::Optional(expected)) => actual.any_flows_to_concrete(expected),
            _ => false,
        }
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result(_, _))
    }

    pub fn result_ok(&self) -> Option<&Type> {
        match self {
            Self::Result(ok, _) => Some(ok),
            _ => None,
        }
    }

    pub fn is_result_unit(&self) -> bool {
        matches!(self, Self::Result(ok, _) if matches!(ok.as_ref(), Self::Unit))
    }

    pub fn matches_expected(&self, expected: &Type) -> bool {
        if self == expected
            || matches!(self, Self::Any | Self::Unknown | Self::Invalid)
            || matches!(expected, Self::Any | Self::Unknown | Self::Invalid)
        {
            return true;
        }
        match (self, expected) {
            (Self::List(actual), Self::List(expected)) => actual.matches_expected(expected),
            (Self::Map(actual), Self::Map(expected)) => actual.matches_expected(expected),
            (Self::Stream(actual), Self::Stream(expected)) => actual.matches_expected(expected),
            (Self::Result(actual_ok, actual_err), Self::Result(expected_ok, expected_err)) => {
                actual_ok.matches_expected(expected_ok) && actual_err.matches_expected(expected_err)
            }
            (Self::Record(actual_fields), Self::Record(_)) if actual_fields.is_empty() => true,
            (Self::Record(_), Self::Record(expected_fields)) if expected_fields.is_empty() => true,
            (Self::Record(actual_fields), Self::Record(expected_fields)) => {
                expected_fields.iter().all(|(name, expected)| {
                    actual_fields
                        .get(name)
                        .is_some_and(|actual| actual.matches_expected(expected))
                })
            }
            (Self::Module(_), Self::Module(expected_exports)) if expected_exports.is_empty() => {
                true
            }
            (Self::Module(actual_exports), Self::Module(expected_exports)) => {
                expected_exports.iter().all(|(name, expected)| match actual_exports.get(name) {
                    Some(actual) => module_export_matches_expected(actual, expected),
                    None => expected.optional(),
                })
            }
            (Self::DynamicModule, Self::Module(_)) => false,
            (Self::Tag(a), Self::Tag(b)) => a == b,
            (Self::ErrorVariant { family, .. }, Self::ErrorFamily(expected)) => family == expected,
            (Self::ErrorVariant { .. }, Self::Error) => true,
            (Self::ErrorFamily(_), Self::Error) => true,
            (Self::ProcessError, Self::Error) => true,
            (
                Self::ErrorVariant { family, variant },
                Self::ErrorVariant {
                    family: ef,
                    variant: ev,
                },
            ) => family == ef && variant == ev,
            (Self::ErrorFamily(a), Self::ErrorFamily(b)) => a == b,
            (Self::ErrorFacet(a), Self::ErrorFacet(b)) => a == b,
            // null matches any Optional
            (Self::Null, Self::Optional(_)) => true,
            // T matches Optional[T]
            (actual, Self::Optional(expected)) => actual.matches_expected(expected),
            _ => false,
        }
    }

    pub fn optional_inner(&self) -> Option<&Type> {
        match self {
            Self::Optional(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn can_display(&self) -> bool {
        matches!(
            self,
            Self::Any
                | Self::Str
                | Self::Int
                | Self::Bool
                | Self::Path
                | Self::Duration
                | Self::Float
        )
    }

    pub fn can_be_argv_item(&self) -> bool {
        matches!(
            self,
            Self::Any | Self::Str | Self::Int | Self::Bool | Self::Path | Self::Duration
        )
    }

    pub fn can_word_convert_to(&self) -> bool {
        matches!(
            self,
            Self::Any | Self::Str | Self::Path | Self::Int | Self::Bool | Self::Duration
        )
    }

    pub fn is_json_compatible(&self) -> bool {
        match self {
            Self::Any
            | Self::Unknown
            | Self::Invalid
            | Self::Null
            | Self::Bool
            | Self::Int
            | Self::Float
            | Self::Str => true,
            Self::List(item) | Self::Map(item) | Self::Stream(item) => item.is_json_compatible(),
            Self::Record(fields) => fields.values().all(Self::is_json_compatible),
            _ => false,
        }
    }

    pub fn annotation_source(&self) -> Option<String> {
        match self {
            Self::Any
            | Self::Unknown
            | Self::Invalid
            | Self::EnvPathList
            | Self::Record(_)
            | Self::Module(_) | Self::DynamicModule => None,
            Self::Unit => Some("Unit".to_string()),
            Self::Null => Some("Null".to_string()),
            Self::Bool => Some("Bool".to_string()),
            Self::Int => Some("Int".to_string()),
            Self::Float => Some("Float".to_string()),
            Self::Duration => Some("Duration".to_string()),
            Self::Str => Some("Str".to_string()),
            Self::Bytes => Some("Bytes".to_string()),
            Self::Digest => Some("Digest".to_string()),
            Self::Regex => Some("Regex".to_string()),
            Self::Path => Some("Path".to_string()),
            Self::List(inner) => Some(format!("List[{}]", inner.annotation_source()?)),
            Self::Map(inner) => Some(format!("Map[{}]", inner.annotation_source()?)),
            Self::Stream(inner) => Some(format!("Stream[{}]", inner.annotation_source()?)),
            Self::Result(ok, err) => {
                let ok = ok.annotation_source()?;
                if matches!(err.as_ref(), Self::Error) {
                    Some(format!("Result[{ok}]"))
                } else {
                    Some(format!("Result[{ok}, {}]", err.annotation_source()?))
                }
            }
            Self::Status => Some("Status".to_string()),
            Self::Error => Some("Error".to_string()),
            Self::ErrorFamily(name) => Some(name.to_string()),
            Self::ErrorVariant { family, variant } => Some(format!("{family}.{variant}")),
            Self::ErrorFacet(name) => Some(name.to_string()),
            Self::ProcessError => Some("ProcessError".to_string()),
            Self::Pure => Some("Pure".to_string()),
            Self::Proc => Some("Proc".to_string()),
            Self::Command => Some("Command".to_string()),
            Self::ProcessHandle => Some("ProcessHandle".to_string()),
            Self::NetJob => Some("NetJob".to_string()),
            Self::Tag(name) => Some(name.to_string()),
            Self::Optional(inner) => Some(format!("{}?", inner.annotation_source()?)),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, "Any"),
            Self::Unknown => write!(f, "<unknown>"),
            Self::Invalid => write!(f, "<invalid>"),
            Self::Null => write!(f, "Null"),
            Self::Bool => write!(f, "Bool"),
            Self::Int => write!(f, "Int"),
            Self::Float => write!(f, "Float"),
            Self::Duration => write!(f, "Duration"),
            Self::Str => write!(f, "Str"),
            Self::Bytes => write!(f, "Bytes"),
            Self::Digest => write!(f, "Digest"),
            Self::Regex => write!(f, "Regex"),
            Self::Path => write!(f, "Path"),
            Self::List(inner) => write!(f, "List[{inner}]"),
            Self::Map(inner) => write!(f, "Map[{inner}]"),
            Self::Stream(inner) => write!(f, "Stream[{inner}]"),
            Self::Record(_) => write!(f, "Record"),
            Self::Module(_) => write!(f, "Module"),
            Self::DynamicModule => write!(f, "Module"),
            Self::Result(ok, err) => write!(f, "Result[{ok}, {err}]"),
            Self::Status => write!(f, "Status"),
            Self::EnvPathList => write!(f, "EnvPathList"),
            Self::Error => write!(f, "Error"),
            Self::ErrorFamily(name) => write!(f, "{name}"),
            Self::ErrorVariant { family, variant } => write!(f, "{family}.{variant}"),
            Self::ErrorFacet(name) => write!(f, "{name}"),
            Self::ProcessError => write!(f, "ProcessError"),
            Self::Pure => write!(f, "Pure"),
            Self::Proc => write!(f, "Proc"),
            Self::Command => write!(f, "Command"),
            Self::ProcessHandle => write!(f, "ProcessHandle"),
            Self::NetJob => write!(f, "NetJob"),
            Self::Unit => write!(f, "Unit"),
            Self::Tag(name) => write!(f, "{name}"),
            Self::Optional(inner) => write!(f, "{inner}?"),
        }
    }
}

fn module_export_any_flows_to_concrete(
    actual: &ModuleExportType,
    expected: &ModuleExportType,
) -> bool {
    match (actual, expected) {
        (
            ModuleExportType::Value { ty: actual, .. },
            ModuleExportType::Value { ty: expected, .. },
        ) => actual.any_flows_to_concrete(expected),
        (
            ModuleExportType::Proc { sig: actual, .. },
            ModuleExportType::Proc { sig: expected, .. },
        )
        | (
            ModuleExportType::Pure { sig: actual, .. },
            ModuleExportType::Pure { sig: expected, .. },
        ) => callable_any_flows_to_concrete(actual, expected),
        _ => false,
    }
}

fn module_export_matches_expected(actual: &ModuleExportType, expected: &ModuleExportType) -> bool {
    match (actual, expected) {
        (
            ModuleExportType::Value { ty: actual, .. },
            ModuleExportType::Value { ty: expected, .. },
        ) => actual.matches_expected(expected),
        (
            ModuleExportType::Proc { sig: actual, .. },
            ModuleExportType::Proc { sig: expected, .. },
        )
        | (
            ModuleExportType::Pure { sig: actual, .. },
            ModuleExportType::Pure { sig: expected, .. },
        ) => callable_matches_expected(actual, expected),
        _ => false,
    }
}

fn callable_any_flows_to_concrete(actual: &CallableType, expected: &CallableType) -> bool {
    actual
        .params
        .iter()
        .zip(expected.params.iter())
        .any(|(actual, expected)| actual.ty.any_flows_to_concrete(&expected.ty))
        || actual.return_ty.any_flows_to_concrete(&expected.return_ty)
}

fn callable_matches_expected(actual: &CallableType, expected: &CallableType) -> bool {
    actual.params.len() == expected.params.len()
        && actual
            .params
            .iter()
            .zip(expected.params.iter())
            .all(|(actual_param, expected_param)| {
                actual_param.rest == expected_param.rest
                    && actual_param.ty.matches_expected(&expected_param.ty)
            })
        && actual.return_ty.matches_expected(&expected.return_ty)
        && callable_effects_match(&actual.effects, &expected.effects)
}

fn callable_effects_match(actual: &Option<Vec<Effect>>, expected: &Option<Vec<Effect>>) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some(actual), Some(expected)) => {
            actual.iter().all(|effect| expected.contains(effect))
                && expected.iter().all(|effect| actual.contains(effect))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{CallableParamType, CallableType, ModuleExportType, Type};
    use crate::symbol::Name;
    use crate::syntax::node::Effect;
    use std::collections::BTreeMap;

    fn proc(effects: Option<Vec<Effect>>) -> ModuleExportType {
        ModuleExportType::Proc {
            sig: CallableType {
                params: vec![CallableParamType {
                    name: Name::intern("value"),
                    ty: Type::Str,
                    defaulted: false,
                    rest: false,
                }],
                return_ty: Box::new(Type::Unit),
                effects,
            },
            optional: false,
        }
    }

    fn pure() -> ModuleExportType {
        ModuleExportType::Pure {
            sig: CallableType {
                params: vec![CallableParamType {
                    name: Name::intern("value"),
                    ty: Type::Str,
                    defaulted: false,
                    rest: false,
                }],
                return_ty: Box::new(Type::Unit),
                effects: None,
            },
            optional: false,
        }
    }

    fn value(ty: Type, optional: bool) -> ModuleExportType {
        ModuleExportType::Value { ty, optional }
    }

    #[test]
    fn empty_module_does_not_satisfy_concrete_contract() {
        let expected = Type::Module(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error])),
        )]));
        assert!(!Type::Module(BTreeMap::new()).matches_expected(&expected));
    }

    #[test]
    fn callable_effects_must_match_exactly() {
        let expected = Type::Module(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error])),
        )]));
        let actual = Type::Module(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error, Effect::Fs])),
        )]));
        assert!(!actual.matches_expected(&expected));
    }

    #[test]
    fn module_contract_checks_member_kind_and_value_type() {
        let name = Name::intern("run");
        let expected = Type::Module(BTreeMap::from([(name, value(Type::Str, false))]));
        assert!(!Type::Module(BTreeMap::new()).matches_expected(&expected));
        assert!(!Type::Module(BTreeMap::from([(name, value(Type::Int, false))]))
            .matches_expected(&expected));
        assert!(!Type::Module(BTreeMap::from([(name, proc(None))])).matches_expected(&expected));

        let actual = Type::Module(BTreeMap::from([
            (name, value(Type::Str, false)),
            (Name::intern("value"), value(Type::Bool, false)),
        ]));
        assert!(actual.matches_expected(&expected));
    }

    #[test]
    fn module_contract_checks_callable_kind_and_signature_invariantly() {
        let name = Name::intern("run");
        let expected = Type::Module(BTreeMap::from([(name, proc(Some(vec![Effect::Error])))]));
        assert!(!Type::Module(BTreeMap::from([(name, pure())])).matches_expected(&expected));

        let wrong_count = ModuleExportType::Proc {
            sig: CallableType {
                params: Vec::new(),
                return_ty: Box::new(Type::Unit),
                effects: Some(vec![Effect::Error]),
            },
            optional: false,
        };
        assert!(!Type::Module(BTreeMap::from([(name, wrong_count)])).matches_expected(&expected));

        let wrong_parameter = ModuleExportType::Proc {
            sig: CallableType {
                params: vec![CallableParamType {
                    name: Name::intern("value"),
                    ty: Type::Int,
                    defaulted: false,
                    rest: false,
                }],
                return_ty: Box::new(Type::Unit),
                effects: Some(vec![Effect::Error]),
            },
            optional: false,
        };
        assert!(
            !Type::Module(BTreeMap::from([(name, wrong_parameter)])).matches_expected(&expected)
        );

        let wrong_return = ModuleExportType::Proc {
            sig: CallableType {
                params: vec![CallableParamType {
                    name: Name::intern("value"),
                    ty: Type::Str,
                    defaulted: false,
                    rest: false,
                }],
                return_ty: Box::new(Type::Str),
                effects: Some(vec![Effect::Error]),
            },
            optional: false,
        };
        assert!(!Type::Module(BTreeMap::from([(name, wrong_return)])).matches_expected(&expected));
    }

    #[test]
    fn optional_module_export_must_match_when_present() {
        let name = Name::intern("description");
        let expected = Type::Module(BTreeMap::from([(name, value(Type::Str, true))]));
        assert!(Type::Module(BTreeMap::new()).matches_expected(&expected));
        assert!(Type::Module(BTreeMap::from([(name, value(Type::Str, false))]))
            .matches_expected(&expected));
        assert!(!Type::Module(BTreeMap::from([(name, value(Type::Int, false))]))
            .matches_expected(&expected));
    }
}
