use crate::symbol::{Name, Symbol};
use crate::syntax::arena::{ArenaTypeExprTag, AstArena, TypeExprId};
use crate::syntax::node::Effect;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use xsh_registry::types::BuiltinTypeName;
pub use xsh_registry::types::BuiltinTypeParameter;

fn btree_map<K: Ord, V>(entries: Vec<(K, V)>) -> BTreeMap<K, V> {
    let mut map = BTreeMap::new();
    map.extend(entries);
    map
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    BuiltinParameter(BuiltinTypeParameter),
    Inference(super::constraints::TypeVariableId),
    Any,
    Unknown,
    Invalid,
    Null,
    Bool,
    Int,
    /// Nonnegative Int constraint; runtime values retain the Int representation.
    UInt,
    Float,
    Duration,
    Str,
    Bytes,
    Digest,
    Regex,
    Path,
    List(Box<Type>),
    /// Ordered scalar key type followed by homogeneous value type.
    Map(Box<Type>, Box<Type>),
    Stream(Box<Type>),
    /// A record value whose fields were deliberately erased.
    ErasedRecord,
    Record(BTreeMap<Name, Type>),
    /// Shared because namespace bindings carry whole module contracts through
    /// every checker scope snapshot.
    Module(Arc<BTreeMap<Name, ModuleExportType>>),
    DynamicModule,
    Result(Box<Type>, Box<Type>),
    Status,
    EnvPathList,
    Error,
    ErrorFamily(Name),
    ErrorVariant {
        family: Name,
        variant: Name,
    },
    ErrorFacet(Name),
    ProcessError,
    Pure,
    Proc,
    Command,
    ProcessHandle,
    NetJob,
    FsRoot,
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
    /// Whether runtime storage must preserve a nonnegative integer constraint.
    pub(crate) fn has_unsigned_constraint(&self) -> bool {
        match self {
            Self::UInt => true,
            Self::List(item) | Self::Stream(item) | Self::Optional(item) => {
                item.has_unsigned_constraint()
            }
            Self::Map(key, value) | Self::Result(key, value) => {
                key.has_unsigned_constraint() || value.has_unsigned_constraint()
            }
            Self::Record(fields) => fields.values().any(Self::has_unsigned_constraint),
            _ => false,
        }
    }

    pub fn is_map_key(&self) -> bool {
        matches!(
            self,
            Self::Str
                | Self::Int
                | Self::UInt
                | Self::Bool
                | Self::Bytes
                | Self::Path
                | Self::Duration
        )
    }

    /// Context restoration cannot outlive a producer or live host handle.
    pub(crate) fn can_escape_context_scope(&self) -> bool {
        match self {
            Self::Stream(_) | Self::ProcessHandle | Self::NetJob => false,
            Self::List(item) | Self::Optional(item) => item.can_escape_context_scope(),
            Self::Map(_, item) => item.can_escape_context_scope(),
            Self::Result(ok, error) => {
                ok.can_escape_context_scope() && error.can_escape_context_scope()
            }
            Self::Record(fields) => fields.values().all(Self::can_escape_context_scope),
            _ => true,
        }
    }

    /// Checked item facts for direct loops and comprehension clauses.
    pub(crate) fn iteration_item_type(&self) -> Option<Type> {
        match self {
            Self::List(item) | Self::Stream(item) => Some((**item).clone()),
            Self::Str => Some(Self::Str),
            Self::Bytes => Some(Self::Int),
            Self::Map(key, item) => Some(Self::Record(BTreeMap::from([
                (Name::intern("key"), (**key).clone()),
                (Name::intern("value"), (**item).clone()),
            ]))),
            Self::Result(ok, _)
                if matches!(
                    ok.as_ref(),
                    Self::List(_) | Self::Stream(_) | Self::Map(_, _) | Self::Str | Self::Bytes
                ) =>
            {
                ok.iteration_item_type()
            }
            _ => None,
        }
    }

    /// Conservative owned-heap estimate for one semantic type tree.
    pub fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut total = size_of::<Self>();
        match self {
            Self::List(inner) | Self::Stream(inner) | Self::Optional(inner) => {
                total = total.saturating_add(size_of::<Type>() + inner.retained_bytes());
            }
            Self::Map(key, value) => {
                total = total.saturating_add(
                    2 * size_of::<Type>() + key.retained_bytes() + value.retained_bytes(),
                );
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
            ArenaTypeExprTag::Applied => Type::Unknown,
            ArenaTypeExprTag::Named => {
                Self::from_name(&Name::from_symbol(Symbol::from_raw(data.lhs)).as_str())
            }
            ArenaTypeExprTag::Qualified => Self::Unknown,
            ArenaTypeExprTag::List => Self::List(Box::new(Self::from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
            ))),
            ArenaTypeExprTag::Map => Self::Map(
                Box::new(
                    TypeExprId::from_optional_raw(data.rhs)
                        .map_or(Self::Str, |key| Self::from_arena(arena, key)),
                ),
                Box::new(Self::from_arena(
                    arena,
                    TypeExprId::from_index(data.lhs as usize),
                )),
            ),
            ArenaTypeExprTag::Stream => Self::Stream(Box::new(Self::from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
            ))),
            ArenaTypeExprTag::Module => {
                let inner = Self::from_arena(arena, TypeExprId::from_index(data.lhs as usize));
                Self::Module(Arc::new(btree_map(vec![(
                    Name::intern("<schema>"),
                    ModuleExportType::Value {
                        ty: inner,
                        optional: false,
                    },
                )])))
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
            BuiltinTypeName::Int => Self::Int,
            BuiltinTypeName::UInt => Self::UInt,
            BuiltinTypeName::Float => Self::Float,
            BuiltinTypeName::Duration => Self::Duration,
            BuiltinTypeName::Str => Self::Str,
            BuiltinTypeName::Bytes => Self::Bytes,
            BuiltinTypeName::Digest => Self::Digest,
            BuiltinTypeName::Regex => Self::Regex,
            BuiltinTypeName::Path => Self::Path,
            BuiltinTypeName::Map => Self::Map(Box::new(Self::Str), Box::new(Self::Unknown)),
            BuiltinTypeName::Module => Self::DynamicModule,
            BuiltinTypeName::Record => Self::ErasedRecord,
            BuiltinTypeName::Status => Self::Status,
            BuiltinTypeName::EnvPathList => Self::EnvPathList,
            BuiltinTypeName::Error => Self::Error,
            BuiltinTypeName::ProcessError => Self::ProcessError,
            BuiltinTypeName::Pure => Self::Pure,
            BuiltinTypeName::Proc => Self::Proc,
            BuiltinTypeName::Command => Self::Command,
            BuiltinTypeName::ProcessHandle => Self::ProcessHandle,
            BuiltinTypeName::NetJob => Self::NetJob,
            BuiltinTypeName::FsRoot => Self::FsRoot,
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
            Self::UInt => Some(BuiltinTypeName::UInt),
            Self::Float => Some(BuiltinTypeName::Float),
            Self::Duration => Some(BuiltinTypeName::Duration),
            Self::Str => Some(BuiltinTypeName::Str),
            Self::Bytes => Some(BuiltinTypeName::Bytes),
            Self::Digest => Some(BuiltinTypeName::Digest),
            Self::Regex => Some(BuiltinTypeName::Regex),
            Self::Path => Some(BuiltinTypeName::Path),
            Self::Map(_, _) => Some(BuiltinTypeName::Map),
            Self::Module(_) | Self::DynamicModule => Some(BuiltinTypeName::Module),
            Self::ErasedRecord | Self::Record(_) => Some(BuiltinTypeName::Record),
            Self::Status => Some(BuiltinTypeName::Status),
            Self::EnvPathList => Some(BuiltinTypeName::EnvPathList),
            Self::Error => Some(BuiltinTypeName::Error),
            Self::ProcessError => Some(BuiltinTypeName::ProcessError),
            Self::Pure => Some(BuiltinTypeName::Pure),
            Self::Proc => Some(BuiltinTypeName::Proc),
            Self::Command => Some(BuiltinTypeName::Command),
            Self::ProcessHandle => Some(BuiltinTypeName::ProcessHandle),
            Self::NetJob => Some(BuiltinTypeName::NetJob),
            Self::FsRoot => Some(BuiltinTypeName::FsRoot),
            Self::Result(_, _) => Some(BuiltinTypeName::Result),
            Self::Unit => Some(BuiltinTypeName::Unit),
            Self::BuiltinParameter(_)
            | Self::Inference(_)
            | Self::Invalid
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

    /// Transient variables must be substituted before publishing signatures,
    /// schema instances, or runtime checks.
    pub fn contains_inference(&self) -> bool {
        let mut pending = vec![self];
        while let Some(ty) = pending.pop() {
            match ty {
                Self::Inference(_) => return true,
                Self::List(inner) | Self::Stream(inner) | Self::Optional(inner) => {
                    pending.push(inner)
                }
                Self::Map(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                Self::Result(ok, error) => {
                    pending.push(ok);
                    pending.push(error);
                }
                Self::Record(fields) => pending.extend(fields.values()),
                Self::Module(exports) => {
                    for export in exports.values() {
                        match export {
                            ModuleExportType::Value { ty, .. } => pending.push(ty),
                            ModuleExportType::Proc { sig, .. }
                            | ModuleExportType::Pure { sig, .. } => {
                                pending.push(&sig.return_ty);
                                pending.extend(sig.params.iter().map(|param| &param.ty));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    pub fn contains_any(&self) -> bool {
        match self {
            Self::Any => true,
            Self::List(inner) | Self::Stream(inner) | Self::Optional(inner) => inner.contains_any(),
            Self::Map(key, value) => key.contains_any() || value.contains_any(),
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
            (Self::ErasedRecord, Self::Record(_)) => true,
            (Self::DynamicModule, Self::Module(_)) => true,
            (Self::List(actual), Self::List(expected))
            | (Self::Stream(actual), Self::Stream(expected))
            | (Self::Optional(actual), Self::Optional(expected)) => {
                actual.any_flows_to_concrete(expected)
            }
            (Self::Map(ak, av), Self::Map(ek, ev)) => {
                ak.any_flows_to_concrete(ek) || av.any_flows_to_concrete(ev)
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
            || matches!(
                (self, expected),
                (Self::Int, Self::UInt) | (Self::UInt, Self::Int)
            )
            || matches!(self, Self::Unknown | Self::Invalid)
            || matches!(expected, Self::Any | Self::Unknown | Self::Invalid)
        {
            return true;
        }
        match (self, expected) {
            (Self::Any, _) => false,
            (Self::List(actual), Self::List(expected))
            | (Self::Stream(actual), Self::Stream(expected)) => actual.matches_invariant(expected),
            (Self::Map(ak, actual), Self::Map(ek, expected)) => {
                ak.matches_invariant(ek) && actual.matches_invariant(expected)
            }
            (Self::Result(actual_ok, actual_err), Self::Result(expected_ok, expected_err)) => {
                actual_ok.matches_expected(expected_ok) && actual_err.matches_expected(expected_err)
            }
            (Self::Record(_), Self::ErasedRecord) => true,
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
            (Self::Module(actual_exports), Self::Module(expected_exports)) => expected_exports
                .iter()
                .all(|(name, expected)| match actual_exports.get(name) {
                    Some(actual) => module_export_matches_expected(actual, expected),
                    None => expected.optional(),
                }),
            (Self::DynamicModule, Self::Module(_)) => false,
            (Self::Tag(a), Self::Tag(b)) => a == b,
            (Self::ErrorVariant { family, .. }, Self::ErrorFamily(expected)) => family == expected,
            (Self::ErrorVariant { .. }, Self::Error) => true,
            (Self::ErrorFamily(_), Self::Error) => true,
            (Self::ErrorFacet(_), Self::Error) => true,
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

    // Scalar domain conversions require a checked value boundary. A container
    // cannot apply those checks to its stored or lazily produced elements.
    pub(super) fn matches_invariant(&self, expected: &Type) -> bool {
        match (self, expected) {
            (Self::Int, Self::UInt) | (Self::UInt, Self::Int) => false,
            (Self::List(actual), Self::List(expected))
            | (Self::Stream(actual), Self::Stream(expected))
            | (Self::Optional(actual), Self::Optional(expected)) => {
                actual.matches_invariant(expected)
            }
            (Self::Map(ak, av), Self::Map(ek, ev)) => {
                ak.matches_invariant(ek) && av.matches_invariant(ev)
            }
            (Self::Result(ao, ae), Self::Result(eo, ee)) => {
                ao.matches_invariant(eo) && ae.matches_invariant(ee)
            }
            (Self::Record(actual), Self::Record(expected)) => {
                actual.len() == expected.len()
                    && expected.iter().all(|(name, ty)| {
                        actual
                            .get(name)
                            .is_some_and(|actual| actual.matches_invariant(ty))
                    })
            }
            _ => self.matches_expected(expected) && expected.matches_expected(self),
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
            Self::Str
                | Self::Int
                | Self::UInt
                | Self::Bool
                | Self::Path
                | Self::Duration
                | Self::Float
        )
    }

    pub fn can_be_argv_item(&self) -> bool {
        matches!(
            self,
            Self::Str | Self::Int | Self::UInt | Self::Bool | Self::Path | Self::Duration
        )
    }

    pub fn can_word_convert_to(&self) -> bool {
        matches!(
            self,
            Self::Any
                | Self::Str
                | Self::Path
                | Self::Int
                | Self::UInt
                | Self::Bool
                | Self::Duration
        )
    }

    pub fn is_json_compatible(&self) -> bool {
        self.is_json_compatible_with(&|_| false)
    }

    pub fn is_json_compatible_with(&self, wire_enum: &impl Fn(Name) -> bool) -> bool {
        match self {
            Self::Any
            | Self::Unknown
            | Self::Invalid
            | Self::Null
            | Self::Bool
            | Self::Int
            | Self::UInt
            | Self::Float
            | Self::Str => true,
            Self::List(item) | Self::Stream(item) | Self::Optional(item) => {
                item.is_json_compatible_with(wire_enum)
            }
            Self::ErasedRecord => true,
            Self::Map(key, value) => {
                matches!(key.as_ref(), Self::Str) && value.is_json_compatible_with(wire_enum)
            }
            Self::Record(fields) => fields
                .values()
                .all(|ty| ty.is_json_compatible_with(wire_enum)),
            Self::Tag(name) => wire_enum(*name),
            _ => false,
        }
    }

    pub fn annotation_source(&self) -> Option<String> {
        match self {
            Self::BuiltinParameter(parameter) => Some(parameter.label().to_string()),
            Self::Inference(_)
            | Self::Any
            | Self::Unknown
            | Self::Invalid
            | Self::EnvPathList
            | Self::Record(_)
            | Self::ErasedRecord
            | Self::Module(_)
            | Self::DynamicModule => None,
            Self::Unit => Some("Unit".to_string()),
            Self::Null => Some("Null".to_string()),
            Self::Bool => Some("Bool".to_string()),
            Self::Int => Some("Int".to_string()),
            Self::UInt => Some("UInt".to_string()),
            Self::Float => Some("Float".to_string()),
            Self::Duration => Some("Duration".to_string()),
            Self::Str => Some("Str".to_string()),
            Self::Bytes => Some("Bytes".to_string()),
            Self::Digest => Some("Digest".to_string()),
            Self::Regex => Some("Regex".to_string()),
            Self::Path => Some("Path".to_string()),
            Self::List(inner) => Some(format!("List[{}]", inner.annotation_source()?)),
            Self::Map(key, inner) => Some(if matches!(key.as_ref(), Self::Str) {
                format!("Map[{}]", inner.annotation_source()?)
            } else {
                format!(
                    "Map[{}, {}]",
                    key.annotation_source()?,
                    inner.annotation_source()?
                )
            }),
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
            Self::FsRoot => Some("FsRoot".to_string()),
            Self::Tag(name) => Some(name.to_string()),
            Self::Optional(inner) => Some(format!("{}?", inner.annotation_source()?)),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltinParameter(parameter) => write!(f, "{}", parameter.label()),
            Self::Inference(_) => write!(f, "<type needs an annotation>"),
            Self::Any => write!(f, "Any"),
            Self::Unknown => write!(f, "<unknown>"),
            Self::Invalid => write!(f, "<invalid>"),
            Self::Null => write!(f, "Null"),
            Self::Bool => write!(f, "Bool"),
            Self::Int => write!(f, "Int"),
            Self::UInt => write!(f, "UInt"),
            Self::Float => write!(f, "Float"),
            Self::Duration => write!(f, "Duration"),
            Self::Str => write!(f, "Str"),
            Self::Bytes => write!(f, "Bytes"),
            Self::Digest => write!(f, "Digest"),
            Self::Regex => write!(f, "Regex"),
            Self::Path => write!(f, "Path"),
            Self::List(inner) => write!(f, "List[{inner}]"),
            Self::Map(key, inner) => {
                if matches!(key.as_ref(), Self::Str) {
                    write!(f, "Map[{inner}]")
                } else {
                    write!(f, "Map[{key}, {inner}]")
                }
            }
            Self::Stream(inner) => write!(f, "Stream[{inner}]"),
            Self::ErasedRecord | Self::Record(_) => write!(f, "Record"),
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
            Self::FsRoot => write!(f, "FsRoot"),
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

/// A contract entry without a clause is unrestricted, so its callers already
/// assume any effect and it accepts every export. A clause names the exact set.
pub(crate) fn callable_effects_match(
    actual: &Option<Vec<Effect>>,
    expected: &Option<Vec<Effect>>,
) -> bool {
    match (actual, expected) {
        (_, None) => true,
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
        let expected = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error])),
        )])));
        assert!(!Type::Module(std::sync::Arc::new(BTreeMap::new())).matches_expected(&expected));
    }

    #[test]
    fn callable_effects_must_match_exactly() {
        let expected = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error])),
        )])));
        let actual = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error, Effect::Fs])),
        )])));
        assert!(!actual.matches_expected(&expected));
    }

    #[test]
    fn unrestricted_contract_entry_accepts_any_export_effects() {
        let expected = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            Name::intern("run"),
            proc(None),
        )])));
        for effects in [None, Some(Vec::new()), Some(vec![Effect::Fs, Effect::Error])] {
            let actual = Type::Module(std::sync::Arc::new(BTreeMap::from([(
                Name::intern("run"),
                proc(effects.clone()),
            )])));
            assert!(actual.matches_expected(&expected), "{effects:?}");
        }
        let restricted = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error])),
        )])));
        let unrestricted = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            Name::intern("run"),
            proc(None),
        )])));
        assert!(!unrestricted.matches_expected(&restricted));
    }

    #[test]
    fn module_contract_checks_member_kind_and_value_type() {
        let name = Name::intern("run");
        let expected = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            name,
            value(Type::Str, false),
        )])));
        assert!(!Type::Module(std::sync::Arc::new(BTreeMap::new())).matches_expected(&expected));
        assert!(
            !Type::Module(std::sync::Arc::new(BTreeMap::from([(
                name,
                value(Type::Int, false)
            )])))
            .matches_expected(&expected)
        );
        assert!(
            !Type::Module(std::sync::Arc::new(BTreeMap::from([(name, proc(None))])))
                .matches_expected(&expected)
        );

        let actual = Type::Module(std::sync::Arc::new(BTreeMap::from([
            (name, value(Type::Str, false)),
            (Name::intern("value"), value(Type::Bool, false)),
        ])));
        assert!(actual.matches_expected(&expected));
    }

    #[test]
    fn module_contract_checks_callable_kind_and_signature_invariantly() {
        let name = Name::intern("run");
        let expected = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            name,
            proc(Some(vec![Effect::Error])),
        )])));
        assert!(
            !Type::Module(std::sync::Arc::new(BTreeMap::from([(name, pure())])))
                .matches_expected(&expected)
        );

        let wrong_count = ModuleExportType::Proc {
            sig: CallableType {
                params: Vec::new(),
                return_ty: Box::new(Type::Unit),
                effects: Some(vec![Effect::Error]),
            },
            optional: false,
        };
        assert!(
            !Type::Module(std::sync::Arc::new(BTreeMap::from([(name, wrong_count)])))
                .matches_expected(&expected)
        );

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
            !Type::Module(std::sync::Arc::new(BTreeMap::from([(
                name,
                wrong_parameter
            )])))
            .matches_expected(&expected)
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
        assert!(
            !Type::Module(std::sync::Arc::new(BTreeMap::from([(name, wrong_return)])))
                .matches_expected(&expected)
        );
    }

    #[test]
    fn optional_module_export_must_match_when_present() {
        let name = Name::intern("description");
        let expected = Type::Module(std::sync::Arc::new(BTreeMap::from([(
            name,
            value(Type::Str, true),
        )])));
        assert!(Type::Module(std::sync::Arc::new(BTreeMap::new())).matches_expected(&expected));
        assert!(
            Type::Module(std::sync::Arc::new(BTreeMap::from([(
                name,
                value(Type::Str, false)
            )])))
            .matches_expected(&expected)
        );
        assert!(
            !Type::Module(std::sync::Arc::new(BTreeMap::from([(
                name,
                value(Type::Int, false)
            )])))
            .matches_expected(&expected)
        );
    }
}
