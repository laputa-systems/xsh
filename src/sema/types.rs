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
    Module(Arc<ModuleType>),
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
    /// A closed set of member types, in the order written. A value fits when
    /// it fits a member. The members are never simplified, so a resolved
    /// union has at least two members, none of which fits another, and none
    /// of which is `Any`, `Null`, optional, a stream, or itself a union
    /// (`union_member_error`).
    Union(Vec<Type>),
    /// A callable value whose signature and effect bound the checker knows:
    /// `proc(root: Path) [fs] -> Result[Unit]` or `pure(n: Int) -> Int`. At
    /// run time it is the same handle as `Proc` or `Pure`; the signature is a
    /// checked fact about every value that reaches the type, never a runtime
    /// test, so no dynamic value converts to it.
    Callable(Arc<TypedCallable>),
    /// A base type narrowed by a validation, such as `NonEmpty[T]` over
    /// `List[T]`. At run time it is a value of the base; the validation is a
    /// checked fact about every value that reaches the type. It fits its base
    /// and never the reverse.
    Validated(Box<super::validated::ValidatedType>),
}

/// The contract of a typed callable. `sig.effects` is the upper bound a call
/// charges to its caller; `None` is the unrestricted bound of a `proc(...)`
/// type written without a clause. Parameters carry no defaults and no rest
/// parameter, so a call supplies every argument.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedCallable {
    pub pure: bool,
    pub sig: CallableType,
}

impl TypedCallable {
    /// Builds the contract a callable type expression writes, resolving each
    /// parameter and the return type through `resolve`. Every resolver of
    /// type expressions builds it here, so they agree on its shape.
    pub fn from_type_expr<E>(
        arena: &AstArena,
        id: TypeExprId,
        mut resolve: impl FnMut(TypeExprId) -> Result<Type, E>,
    ) -> Result<Self, E> {
        let callable = arena.callable_type_expr(id);
        Ok(Self {
            pure: callable.pure,
            sig: CallableType {
                params: arena
                    .params(callable.params)
                    .iter()
                    .map(|param| {
                        Ok(CallableParamType {
                            name: param.name,
                            ty: resolve(param.ty)?,
                            defaulted: false,
                            rest: false,
                        })
                    })
                    .collect::<Result<_, E>>()?,
                return_ty: Box::new(resolve(callable.return_ty)?),
                // A bound is a set: two clauses that list the same effects
                // are the same type however they are written.
                effects: callable.effects.map(|effects| {
                    let written = arena.effects(effects).collect::<Vec<_>>();
                    Effect::ALL
                        .into_iter()
                        .filter(|effect| written.contains(effect))
                        .collect()
                }),
            },
        })
    }

    /// Why a callable type expression is not a callable type, or `None` when
    /// it is one. A call through the type supplies every argument by position
    /// or label, so a parameter is a label and a type and nothing else.
    pub fn type_expr_error(arena: &AstArena, id: TypeExprId) -> Option<String> {
        let params = arena.params(arena.callable_type_expr(id).params);
        for (index, param) in params.iter().enumerate() {
            let name = param.name;
            if param.rest {
                return Some(format!(
                    "a callable type cannot have a rest parameter (`...{name}`)"
                ));
            }
            if param.ty_defaulted {
                return Some(format!(
                    "parameter `{name}` of a callable type needs a type"
                ));
            }
            if param.default.is_some() {
                return Some(format!(
                    "parameter `{name}` of a callable type cannot have a default"
                ));
            }
            if params[..index].iter().any(|earlier| earlier.name == name) {
                return Some(format!(
                    "a callable type names parameter `{name}` twice"
                ));
            }
        }
        None
    }

    /// Why a callable with signature `actual` (`actual_pure` for a pure
    /// function) cannot be used where `self` is expected, or `None` when it
    /// can. The rule is deliberately narrow: the same kind, the same
    /// parameter labels and types in order, and the same return type. Only
    /// effects vary: the callable may need fewer than the bound allows.
    /// `check_effects` is false while effect summaries are still being
    /// solved, when an inferred callable's effects and types may not be
    /// known yet. Otherwise a type the checker has not resolved does not fit:
    /// an unknown type is not evidence of a matching one. A type that is
    /// invalid was already reported and is not reported again.
    pub fn mismatch(
        &self,
        actual_pure: bool,
        actual: &CallableType,
        check_effects: bool,
    ) -> Option<String> {
        let kind = |pure: bool| if pure { "pure function" } else { "proc" };
        if self.pure != actual_pure {
            return Some(format!(
                "expected a {}, found a {}",
                kind(self.pure),
                kind(actual_pure)
            ));
        }
        if actual.params.len() != self.sig.params.len() {
            return Some(format!(
                "expected {} parameters, found {}",
                self.sig.params.len(),
                actual.params.len()
            ));
        }
        for (index, (found, expected)) in actual.params.iter().zip(&self.sig.params).enumerate() {
            let position = index + 1;
            if found.rest {
                return Some(format!(
                    "parameter {position} `{}` is a rest parameter",
                    found.name
                ));
            }
            if found.name != expected.name {
                return Some(format!(
                    "parameter {position} is named `{}`, expected `{}`",
                    found.name, expected.name
                ));
            }
            if found.ty == Type::Invalid || expected.ty == Type::Invalid {
                continue;
            }
            if found.ty.is_recovery() || expected.ty.is_recovery() {
                if !check_effects {
                    continue;
                }
                return Some(format!(
                    "the type of parameter `{}` is not known; annotate it",
                    found.name
                ));
            }
            if found.ty != expected.ty {
                return Some(format!(
                    "parameter `{}` has type {}, expected {}",
                    found.name, found.ty, expected.ty
                ));
            }
        }
        let unresolved = actual.return_ty.is_recovery() || self.sig.return_ty.is_recovery();
        let invalid = *actual.return_ty == Type::Invalid || *self.sig.return_ty == Type::Invalid;
        if unresolved && !invalid && check_effects {
            return Some("its return type is not known; annotate it".to_string());
        }
        if !unresolved && actual.return_ty != self.sig.return_ty {
            return Some(format!(
                "it returns {}, expected {}",
                actual.return_ty, self.sig.return_ty
            ));
        }
        if !check_effects || self.pure {
            return None;
        }
        let Some(bound) = &self.sig.effects else {
            return None;
        };
        let Some(required) = &actual.effects else {
            return Some(format!(
                "its effects are unknown or unrestricted, so it cannot be held to `[{}]`",
                effect_list(bound)
            ));
        };
        required
            .iter()
            .find(|effect| !crate::sema::check::Checker::effects_covers(bound, effect))
            .map(|effect| {
                format!(
                    "it requires the `{}` effect, which `[{}]` does not allow",
                    effect.as_str(),
                    effect_list(bound)
                )
            })
    }
}

fn effect_list(effects: &[Effect]) -> String {
    effects
        .iter()
        .map(Effect::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The first member, in the order written, that `accepts` the value at hand.
/// Every place that asks which member of a union a value is — the checker for
/// a static type, the runtime for a dynamic value, schema decoding for a
/// value it may convert — asks through this one function, so they agree when
/// more than one member could accept.
pub fn first_accepting_union_member<'a, M>(
    members: &'a [M],
    mut accepts: impl FnMut(&'a M) -> bool,
) -> Option<&'a M> {
    members.iter().find(|member| accepts(member))
}

/// Why `members` do not form a union type, or `None` when they do. A union is
/// kept exactly as written: nothing is flattened, deduplicated, or absorbed,
/// so each shape a simplifier would rewrite is rejected here instead.
pub fn union_member_error(members: &[Type]) -> Option<String> {
    if members.len() < 2 {
        return Some("a union lists at least two member types".to_string());
    }
    for member in members {
        let reason = match member {
            Type::Any => "`Any` already accepts every value; use `Any` alone",
            Type::Null | Type::Optional(_) => {
                "a member cannot be `Null` or optional; write `Union[...]?` around the non-null members"
            }
            Type::Union(_) => "a member cannot be another union; list its members here",
            Type::Stream(_) => {
                "a member cannot be a stream: a type test cannot inspect the items of a stream"
            }
            Type::Callable(_) => {
                "a member cannot be a callable type: a type test cannot inspect a callable's signature"
            }
            _ => continue,
        };
        return Some(reason.to_string());
    }
    for (index, left) in members.iter().enumerate() {
        if left.is_recovery() {
            continue;
        }
        for right in &members[index + 1..] {
            if right.is_recovery() {
                continue;
            }
            if left == right {
                return Some(format!("`{left}` is listed twice"));
            }
            if left.matches_expected(right) {
                return Some(format!(
                    "every `{left}` already fits the member `{right}`; list one of them"
                ));
            }
            if right.matches_expected(left) {
                return Some(format!(
                    "every `{right}` already fits the member `{left}`; list one of them"
                ));
            }
        }
    }
    None
}

/// The exports a module type promises. An open type is a lower bound: the
/// module has at least these exports. An exact type is the whole surface: the
/// module exports nothing else. A statically imported module's own type is
/// exact, because the checker has seen every export; an `exact module`
/// contract is exact by declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleType {
    pub exports: BTreeMap<Name, ModuleExportType>,
    pub exact: bool,
}

impl ModuleType {
    pub fn open(exports: BTreeMap<Name, ModuleExportType>) -> Self {
        Self {
            exports,
            exact: false,
        }
    }

    pub fn exact(exports: BTreeMap<Name, ModuleExportType>) -> Self {
        Self {
            exports,
            exact: true,
        }
    }
}

impl ModuleType {
    /// Why a module of type `actual` does not satisfy `self`, one line per
    /// export, for a diagnostic. Empty when it does.
    pub fn unmet_by(&self, actual: &ModuleType) -> Vec<String> {
        let mut reasons = Vec::new();
        for (name, expected) in self.iter() {
            match actual.get(name) {
                None if expected.optional() => {}
                None => reasons.push(format!("missing export `{name}`")),
                Some(found) if module_export_matches_expected(found, expected) => {}
                Some(_) => reasons.push(format!(
                    "mismatched export `{name}`: its kind or signature differs from the contract"
                )),
            }
        }
        if self.exact {
            if actual.exact {
                reasons.extend(
                    actual
                        .keys()
                        .filter(|name| !self.contains_key(name))
                        .map(|name| {
                            format!("unexpected export `{name}`: the exact contract does not list it")
                        }),
                );
            } else {
                reasons.push(
                    "the value's type does not say what else the module exports; an exact contract needs `.require(Contract)`"
                        .to_string(),
                );
            }
        }
        reasons
    }
}

// Reading a module type is reading its exports; exactness matters only where
// two module types are compared or a module value is checked.
impl std::ops::Deref for ModuleType {
    type Target = BTreeMap<Name, ModuleExportType>;

    fn deref(&self) -> &Self::Target {
        &self.exports
    }
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
            Self::Union(members) => members.iter().any(Self::has_unsigned_constraint),
            Self::Validated(validated) => validated.base().has_unsigned_constraint(),
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
            Self::Union(members) => members.iter().all(Self::can_escape_context_scope),
            Self::Validated(validated) => validated.base().can_escape_context_scope(),
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
            Self::Validated(validated) => validated.base().iteration_item_type(),
            Self::Result(ok, _)
                if matches!(
                    ok.unvalidated(),
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
            Self::Union(members) => {
                for member in members {
                    total = total.saturating_add(member.retained_bytes());
                }
            }
            Self::Callable(callable) => {
                total = total.saturating_add(callable.sig.retained_bytes());
            }
            Self::Validated(validated) => {
                total = total.saturating_add(validated.base().retained_bytes());
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
                Self::Module(Arc::new(ModuleType::open(btree_map(vec![(
                    Name::intern("<schema>"),
                    ModuleExportType::Value {
                        ty: inner,
                        optional: false,
                    },
                )]))))
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
            ArenaTypeExprTag::Union => Self::Union(
                arena
                    .union_type_members(id)
                    .map(|member| Self::from_arena(arena, member))
                    .collect(),
            ),
            ArenaTypeExprTag::Callable => {
                let Ok(callable) = TypedCallable::from_type_expr(arena, id, |ty| {
                    Ok::<_, std::convert::Infallible>(Self::from_arena(arena, ty))
                });
                Self::Callable(Arc::new(callable))
            }
            ArenaTypeExprTag::NonEmpty => Self::non_empty(Self::from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
            )),
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
            BuiltinTypeName::RelPath => Self::rel_path(),
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
            | Self::Optional(_)
            | Self::Union(_)
            | Self::Callable(_)
            | Self::Validated(_) => None,
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
                Self::Union(members) => pending.extend(members),
                Self::Validated(validated) => pending.push(validated.base()),
                Self::Callable(callable) => {
                    pending.push(&callable.sig.return_ty);
                    pending.extend(callable.sig.params.iter().map(|param| &param.ty));
                }
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

    /// Whether a callable type appears anywhere a value of this type holds one.
    pub fn contains_typed_callable(&self) -> bool {
        match self {
            Self::Callable(_) => true,
            Self::List(inner) | Self::Stream(inner) | Self::Optional(inner) => {
                inner.contains_typed_callable()
            }
            Self::Map(key, value) | Self::Result(key, value) => {
                key.contains_typed_callable() || value.contains_typed_callable()
            }
            Self::Record(fields) => fields.values().any(Self::contains_typed_callable),
            Self::Union(members) => members.iter().any(Self::contains_typed_callable),
            Self::Validated(validated) => validated.base().contains_typed_callable(),
            _ => false,
        }
    }

    pub fn contains_any(&self) -> bool {
        match self {
            Self::Any => true,
            Self::List(inner) | Self::Stream(inner) | Self::Optional(inner) => inner.contains_any(),
            Self::Map(key, value) => key.contains_any() || value.contains_any(),
            Self::Result(ok, err) => ok.contains_any() || err.contains_any(),
            Self::Record(fields) => fields.values().any(Self::contains_any),
            Self::Union(members) => members.iter().any(Self::contains_any),
            Self::Validated(validated) => validated.base().contains_any(),
            Self::Callable(callable) => {
                callable.sig.params.iter().any(|param| param.ty.contains_any())
                    || callable.sig.return_ty.contains_any()
            }
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
            (Self::Validated(actual), Self::Validated(expected)) => {
                actual.base().any_flows_to_concrete(expected.base())
            }
            (Self::Validated(actual), expected) => actual.base().any_flows_to_concrete(expected),
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
            // A validated type fits the same validation over a fitting base.
            // Nothing else fits a validated type: a value of the base has
            // not been validated.
            (Self::Validated(actual), Self::Validated(expected)) => {
                actual.validation().implies(expected.validation())
                    && actual.base().matches_expected(expected.base())
            }
            // A union fits another when each of its members does; it never
            // fits a single member, which needs a narrowing type test.
            (Self::Union(actual), Self::Union(_)) => {
                actual.iter().all(|member| member.matches_expected(expected))
            }
            (actual, Self::Union(members)) => {
                first_accepting_union_member(members, |member| actual.matches_expected(member))
                    .is_some()
            }
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
            (Self::Module(_), Self::Module(expected))
                if expected.is_empty() && !expected.exact =>
            {
                true
            }
            (Self::Module(actual), Self::Module(expected)) => {
                // An exact contract needs proof that nothing else is
                // exported, which only an exact actual type gives.
                let no_unexpected_export = !expected.exact
                    || (actual.exact && actual.keys().all(|name| expected.contains_key(name)));
                no_unexpected_export
                    && expected
                        .iter()
                        .all(|(name, expected)| match actual.get(name) {
                            Some(actual) => module_export_matches_expected(actual, expected),
                            None => expected.optional(),
                        })
            }
            (Self::DynamicModule, Self::Module(_)) => false,
            // A typed callable fits another typed callable under the narrow
            // rule, and either dynamic handle of its own kind. A dynamic
            // handle never fits a typed callable: its signature is unknown.
            (Self::Callable(actual), Self::Callable(expected)) => expected
                .mismatch(actual.pure, &actual.sig, true)
                .is_none(),
            (Self::Callable(actual), Self::Proc) => !actual.pure,
            (Self::Callable(actual), Self::Pure) => actual.pure,
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
            // After the union and optional rules, which ask about the
            // validated type itself, it fits whatever its base fits.
            (Self::Validated(actual), expected) => actual.base().matches_expected(expected),
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
            (Self::Validated(actual), Self::Validated(expected)) => {
                actual.validation() == expected.validation()
                    && actual.base().matches_invariant(expected.base())
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
            self.unvalidated(),
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
        match self {
            // Argv conversion reads the runtime value, so a union converts
            // when each member does.
            Self::Union(members) => members.iter().all(Self::can_be_argv_item),
            _ => matches!(
                self.unvalidated(),
                Self::Str | Self::Int | Self::UInt | Self::Bool | Self::Path | Self::Duration
            ),
        }
    }

    /// The members of a union, in the order written.
    pub fn union_members(&self) -> Option<&[Type]> {
        match self {
            Self::Union(members) => Some(members),
            _ => None,
        }
    }

    /// What is left of a union once a type test for `tested` has failed: the
    /// members `tested` does not cover, as the one member or a smaller union.
    /// `None` when `self` is not a union or the test covers every member.
    pub fn union_without(&self, tested: &Type) -> Option<Type> {
        let members = self.union_members()?;
        let mut rest = members
            .iter()
            .filter(|member| !member.matches_expected(tested))
            .cloned()
            .collect::<Vec<_>>();
        match rest.len() {
            0 => None,
            1 => rest.pop(),
            _ => Some(Self::Union(rest)),
        }
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
            Self::Union(members) => members
                .iter()
                .all(|member| member.is_json_compatible_with(wire_enum)),
            Self::Validated(validated) => validated.base().is_json_compatible_with(wire_enum),
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
            Self::Union(members) => Some(format!(
                "Union[{}]",
                members
                    .iter()
                    .map(Self::annotation_source)
                    .collect::<Option<Vec<_>>>()?
                    .join(", ")
            )),
            Self::Callable(callable) => {
                let params = callable
                    .sig
                    .params
                    .iter()
                    .map(|param| Some(format!("{}: {}", param.name, param.ty.annotation_source()?)))
                    .collect::<Option<Vec<_>>>()?
                    .join(", ");
                let effects = callable
                    .sig
                    .effects
                    .as_ref()
                    .map_or(String::new(), |effects| format!(" [{}]", effect_list(effects)));
                Some(format!(
                    "{}({params}){effects} -> {}",
                    if callable.pure { "pure" } else { "proc" },
                    callable.sig.return_ty.annotation_source()?
                ))
            }
            Self::Validated(validated) => validated.annotation_source(),
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
            Self::Union(members) => {
                write!(f, "Union[")?;
                for (index, member) in members.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{member}")?;
                }
                write!(f, "]")
            }
            Self::Callable(callable) => {
                write!(f, "{}(", if callable.pure { "pure" } else { "proc" })?;
                for (index, param) in callable.sig.params.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", param.name, param.ty)?;
                }
                write!(f, ")")?;
                if let Some(effects) = &callable.sig.effects {
                    write!(f, " [{}]", effect_list(effects))?;
                }
                write!(f, " -> {}", callable.sig.return_ty)
            }
            Self::Validated(validated) => write!(f, "{validated}"),
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

    fn module_type(
        exports: BTreeMap<Name, ModuleExportType>,
    ) -> std::sync::Arc<super::ModuleType> {
        std::sync::Arc::new(super::ModuleType::open(exports))
    }

    // An exact contract accepts only a type that proves the module exports
    // nothing else: an exact type whose exports the contract lists.
    #[test]
    fn exact_module_contract_needs_an_exact_type_without_other_exports() {
        let value = |ty| ModuleExportType::Value {
            ty,
            optional: false,
        };
        let name = Name::intern("name");
        let extra = Name::intern("status");
        let listed = BTreeMap::from([(name, value(Type::Str))]);
        let with_extra = BTreeMap::from([(name, value(Type::Str)), (extra, value(Type::Int))]);
        let exact = |exports: &BTreeMap<_, _>| {
            Type::Module(std::sync::Arc::new(super::ModuleType::exact(
                exports.clone(),
            )))
        };
        let open = |exports: &BTreeMap<_, _>| Type::Module(module_type(exports.clone()));

        assert!(exact(&listed).matches_expected(&exact(&listed)));
        assert!(!exact(&with_extra).matches_expected(&exact(&listed)));
        assert!(!open(&listed).matches_expected(&exact(&listed)));
        // The open contract keeps accepting extras from either kind of type.
        assert!(exact(&with_extra).matches_expected(&open(&listed)));
        assert!(open(&with_extra).matches_expected(&open(&listed)));

        let Type::Module(contract) = exact(&listed) else {
            unreachable!()
        };
        let Type::Module(actual) = exact(&with_extra) else {
            unreachable!()
        };
        assert_eq!(
            contract.unmet_by(&actual),
            ["unexpected export `status`: the exact contract does not list it"]
        );
    }

    // A member fits its union and a union fits a wider one, in any order;
    // nothing takes a union back to a member, and containers stay invariant.
    #[test]
    fn union_accepts_members_and_wider_unions_but_never_a_member() {
        let words = Type::Union(vec![Type::Str, Type::Path]);
        let reordered = Type::Union(vec![Type::Path, Type::Str]);
        let wide = Type::Union(vec![Type::Str, Type::Path, Type::Int]);
        let list = |item: &Type| Type::List(Box::new(item.clone()));

        assert!(Type::Str.matches_expected(&words));
        assert!(Type::Path.matches_expected(&words));
        assert!(!Type::Int.matches_expected(&words));
        assert!(!Type::Null.matches_expected(&words));
        assert!(!Type::Any.matches_expected(&words));
        assert!(Type::Any.any_flows_to_concrete(&words));

        assert!(words.matches_expected(&wide));
        assert!(words.matches_expected(&reordered));
        assert!(!wide.matches_expected(&words));
        assert!(!words.matches_expected(&Type::Str));
        assert!(words.matches_expected(&Type::Any));
        assert!(words.matches_expected(&Type::Optional(Box::new(words.clone()))));
        assert!(Type::Null.matches_expected(&Type::Optional(Box::new(words.clone()))));

        assert!(!list(&Type::Str).matches_expected(&list(&words)));
        assert!(!list(&words).matches_expected(&list(&wide)));
        assert!(list(&words).matches_expected(&list(&reordered)));

        assert_eq!(words.union_without(&Type::Str), Some(Type::Path));
        assert_eq!(
            wide.union_without(&Type::Int),
            Some(Type::Union(vec![Type::Str, Type::Path]))
        );
        assert_eq!(words.union_without(&reordered), None);
        assert_eq!(Type::Str.union_without(&Type::Str), None);
        assert_eq!(words.to_string(), "Union[Str, Path]");
        assert_eq!(
            list(&words).annotation_source().as_deref(),
            Some("List[Union[Str, Path]]")
        );
        assert!(words.can_be_argv_item());
        assert!(!Type::Union(vec![Type::Int, Type::Float]).can_be_argv_item());
    }

    // Every shape a simplifier would rewrite is an error, and the order the
    // members are written in is the order they are asked in.
    #[test]
    fn union_members_are_validated_and_asked_in_written_order() {
        let optional = Type::Optional(Box::new(Type::Int));
        let nested = Type::Union(vec![Type::Str, Type::Path]);
        let stream = Type::Stream(Box::new(Type::Int));
        for members in [
            vec![Type::Str],
            vec![Type::Str, Type::Str],
            vec![Type::Int, Type::UInt],
            vec![Type::Str, Type::Any],
            vec![Type::Str, Type::Null],
            vec![Type::Str, optional],
            vec![nested, Type::Int],
            vec![Type::Str, stream],
            vec![Type::Error, Type::ProcessError],
        ] {
            assert!(super::union_member_error(&members).is_some(), "{members:?}");
        }
        assert_eq!(super::union_member_error(&[Type::Str, Type::Path]), None);
        assert_eq!(
            super::union_member_error(&[Type::Int, Type::Float, Type::Str]),
            None
        );
        // A recovery member already has its own diagnostic.
        assert_eq!(super::union_member_error(&[Type::Invalid, Type::Invalid]), None);

        let members = [Type::Int, Type::Str, Type::Path];
        let mut asked = Vec::new();
        let chosen = super::first_accepting_union_member(&members, |member| {
            asked.push(member.clone());
            matches!(member, Type::Str | Type::Path)
        });
        assert_eq!(chosen, Some(&Type::Str));
        assert_eq!(asked, [Type::Int, Type::Str]);
    }

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
        let expected = Type::Module(module_type(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error])),
        )])));
        assert!(!Type::Module(module_type(BTreeMap::new())).matches_expected(&expected));
    }

    #[test]
    fn callable_effects_must_match_exactly() {
        let expected = Type::Module(module_type(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error])),
        )])));
        let actual = Type::Module(module_type(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error, Effect::Fs])),
        )])));
        assert!(!actual.matches_expected(&expected));
    }

    #[test]
    fn unrestricted_contract_entry_accepts_any_export_effects() {
        let expected = Type::Module(std::sync::Arc::new(super::ModuleType::open(BTreeMap::from([(
            Name::intern("run"),
            proc(None),
        )]))));
        for effects in [None, Some(Vec::new()), Some(vec![Effect::Fs, Effect::Error])] {
            let actual = Type::Module(std::sync::Arc::new(super::ModuleType::open(BTreeMap::from([(
                Name::intern("run"),
                proc(effects.clone()),
            )]))));
            assert!(actual.matches_expected(&expected), "{effects:?}");
        }
        let restricted = Type::Module(std::sync::Arc::new(super::ModuleType::open(BTreeMap::from([(
            Name::intern("run"),
            proc(Some(vec![Effect::Error])),
        )]))));
        let unrestricted = Type::Module(std::sync::Arc::new(super::ModuleType::open(BTreeMap::from([(
            Name::intern("run"),
            proc(None),
        )]))));
        assert!(!unrestricted.matches_expected(&restricted));
    }

    #[test]
    fn module_contract_checks_member_kind_and_value_type() {
        let name = Name::intern("run");
        let expected = Type::Module(module_type(BTreeMap::from([(
            name,
            value(Type::Str, false),
        )])));
        assert!(!Type::Module(module_type(BTreeMap::new())).matches_expected(&expected));
        assert!(
            !Type::Module(module_type(BTreeMap::from([(
                name,
                value(Type::Int, false)
            )])))
            .matches_expected(&expected)
        );
        assert!(
            !Type::Module(module_type(BTreeMap::from([(name, proc(None))])))
                .matches_expected(&expected)
        );

        let actual = Type::Module(module_type(BTreeMap::from([
            (name, value(Type::Str, false)),
            (Name::intern("value"), value(Type::Bool, false)),
        ])));
        assert!(actual.matches_expected(&expected));
    }

    #[test]
    fn module_contract_checks_callable_kind_and_signature_invariantly() {
        let name = Name::intern("run");
        let expected = Type::Module(module_type(BTreeMap::from([(
            name,
            proc(Some(vec![Effect::Error])),
        )])));
        assert!(
            !Type::Module(module_type(BTreeMap::from([(name, pure())])))
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
            !Type::Module(module_type(BTreeMap::from([(name, wrong_count)])))
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
            !Type::Module(module_type(BTreeMap::from([(
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
            !Type::Module(module_type(BTreeMap::from([(name, wrong_return)])))
                .matches_expected(&expected)
        );
    }

    #[test]
    fn optional_module_export_must_match_when_present() {
        let name = Name::intern("description");
        let expected = Type::Module(module_type(BTreeMap::from([(
            name,
            value(Type::Str, true),
        )])));
        assert!(Type::Module(module_type(BTreeMap::new())).matches_expected(&expected));
        assert!(
            Type::Module(module_type(BTreeMap::from([(
                name,
                value(Type::Str, false)
            )])))
            .matches_expected(&expected)
        );
        assert!(
            !Type::Module(module_type(BTreeMap::from([(
                name,
                value(Type::Int, false)
            )])))
            .matches_expected(&expected)
        );
    }
}
