use super::constants::{SchemaComponent, SchemaExpectation, SchemaInstance};
use super::types::{ModuleExportType, Type};
use super::callable_application::CallableApplication;
use super::effects::{CheckedEffects, EffectContract, EffectPredicate};
use crate::source::Span;
use crate::symbol::Name;
use crate::sema::validated::Validation;
use crate::syntax::arena::{ExprId, StmtId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum InstantiationRole { Call, Value }

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct InstantiationSite<S> { pub(crate) source: S, pub(crate) role: InstantiationRole }

impl<S> InstantiationSite<S> {
    pub(crate) fn call(source: S) -> Self { Self { source, role: InstantiationRole::Call } }
    pub(crate) fn value(source: S) -> Self { Self { source, role: InstantiationRole::Value } }
}

impl From<Span> for InstantiationSite<Span> {
    fn from(source: Span) -> Self { Self::call(source) }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SchemeTypeParameterId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SchemeRowParameterId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SchemeEffectParameterId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub(crate) enum ValueConversion { Display, Argv, ArgvOrList, ArgvList, ProcessWord, Env }

impl ValueConversion {
    pub(crate) const ALL: [Self; 6] = [Self::Display, Self::Argv, Self::ArgvOrList, Self::ArgvList, Self::ProcessWord, Self::Env];

    pub(crate) fn accepts(self, ty: &Type) -> bool {
        if ty.contains_recovery() || ty.contains_any() { return false; }
        match self {
            Self::Display => ty.can_display(),
            Self::Env => ty.can_be_env_value(),
            Self::ProcessWord => ty.can_be_process_command_argv_item(),
            Self::Argv => ty.can_be_argv_item(),
            Self::ArgvOrList => ty.can_be_argv_item() || matches!(ty.unvalidated(), Type::List(item) if item.can_be_argv_item()),
            Self::ArgvList => matches!(ty.unvalidated(), Type::List(item) if item.can_be_argv_item()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ValueConversionSet(u8);

impl ValueConversionSet {
    pub(crate) fn from_bits(bits: u8) -> Option<Self> {
        (bits & !((1 << ValueConversion::ALL.len()) - 1) == 0).then_some(Self(bits))
    }

    pub(crate) fn bits(self) -> u8 { self.0 }
    pub(crate) fn is_empty(self) -> bool { self.0 == 0 }
    pub(crate) fn contains(self, conversion: ValueConversion) -> bool { self.0 & (1 << conversion as u8) != 0 }
    pub(crate) fn insert(&mut self, conversion: ValueConversion) { self.0 |= 1 << conversion as u8; }
    pub(crate) fn union(self, other: Self) -> Self { Self(self.0 | other.0) }
    pub(crate) fn accepts(self, ty: &Type) -> bool { ValueConversion::ALL.into_iter().all(|conversion| !self.contains(conversion) || conversion.accepts(ty)) }
    pub(crate) fn is_satisfiable(self, map_key: bool) -> bool {
        !self.contains(ValueConversion::ArgvList)
            || !(map_key || self.contains(ValueConversion::Display) || self.contains(ValueConversion::Argv) || self.contains(ValueConversion::ProcessWord))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IterationPolicy<T> {
    IterableInfallible,
    IterablePropagates(T),
    PipelineInfallible,
    PipelinePropagates(T),
    PipelineTransport,
}

impl<T> IterationPolicy<T> {
    pub(crate) fn error(&self) -> Option<&T> {
        match self {
            Self::IterablePropagates(error) | Self::PipelinePropagates(error) => Some(error),
            Self::IterableInfallible | Self::PipelineInfallible | Self::PipelineTransport => None,
        }
    }

    pub(crate) fn try_map<U, E>(&self, map: impl FnOnce(&T) -> Result<U, E>) -> Result<IterationPolicy<U>, E> {
        Ok(match self {
            Self::IterableInfallible => IterationPolicy::IterableInfallible,
            Self::IterablePropagates(error) => IterationPolicy::IterablePropagates(map(error)?),
            Self::PipelineInfallible => IterationPolicy::PipelineInfallible,
            Self::PipelinePropagates(error) => IterationPolicy::PipelinePropagates(map(error)?),
            Self::PipelineTransport => IterationPolicy::PipelineTransport,
        })
    }

    pub(crate) fn into_pipeline(self) -> Self {
        match self {
            Self::IterableInfallible => Self::PipelineInfallible,
            Self::IterablePropagates(error) => Self::PipelinePropagates(error),
            policy => policy,
        }
    }

    pub(crate) fn is_pipeline(&self) -> bool {
        matches!(self, Self::PipelineInfallible | Self::PipelinePropagates(_) | Self::PipelineTransport)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum IterationSourceKind {
    Direct,
    DirectOrResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IterationRequirement<T> {
    pub(crate) item: T,
    pub(crate) policy: IterationPolicy<T>,
    pub(crate) source_kind: IterationSourceKind,
}

impl<T> IterationRequirement<T> {
    pub(crate) fn types(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.item).chain(self.policy.error())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CallableRequirements {
    pub(crate) map_key_parameters: Vec<SchemeTypeParameterId>,
    pub(crate) conversion_parameters: BTreeMap<SchemeTypeParameterId, ValueConversionSet>,
    pub(crate) iteration_parameters: BTreeMap<SchemeTypeParameterId, IterationRequirement<SchemeType>>,
    pub(crate) effect_parameters: Vec<SchemeEffectParameterId>,
    pub(crate) effect_predicates: Vec<EffectPredicate<SchemeEffectParameterId>>,
    pub(crate) applications: Vec<CallableApplication<SchemeType, SchemeEffectParameterId>>,
    pub(crate) operations: Vec<super::sealed_operation::SealedOperation<SchemeType>>,
    pub(crate) methods: Vec<MethodFamilyRequirement<SchemeType>>,
    pub(crate) wrappers: Vec<super::wrapper_operation::WrapperRequirement<SchemeType, SchemeEffectParameterId>>,
    pub(crate) guards: Vec<Arc<super::guarded_requirements::GuardRequirements>>,
    pub(crate) guard_calls: Vec<Arc<super::guarded_requirements::GuardCallRequirement>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MethodFamilyRequirement<T> {
    pub(crate) family: xsh_registry::signature::MethodFamilyId,
    pub(crate) receiver: T,
    pub(crate) argument_slots: Vec<u32>,
    pub(crate) arguments: Vec<T>,
    pub(crate) result: T,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SchemeType {
    Ground(Type),
    SchemeCallable(super::scheme_callable::SchemeCallableId),
    Parameter(SchemeTypeParameterId),
    Callable {
        kind: crate::sema::types::CallableKind,
        monotype: Arc<CallableMonotype>,
        effects: EffectContract<SchemeEffectParameterId>,
    },
    List(Box<Self>),
    Map(Box<Self>, Box<Self>),
    Stream(Box<Self>),
    Set(Box<Self>),
    Result(Box<Self>, Box<Self>),
    Optional(Box<Self>),
    Record {
        fields: BTreeMap<Name, Self>,
        tail: SchemeRowTail,
    },
    Union(Vec<Self>),
    Validated(Validation, Box<Self>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemeRowTail {
    Closed,
    Parameter(SchemeRowParameterId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemeRowParameter {
    pub(crate) id: SchemeRowParameterId,
    pub(crate) lacks: BTreeSet<Name>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemeParameter {
    pub(crate) name: Name,
    pub(crate) ty: SchemeType,
    pub(crate) schema_expectation: Option<SchemaExpectation<SchemeType>>,
    pub(crate) defaulted: bool,
    pub(crate) rest: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableScheme {
    type_parameters: Arc<[SchemeTypeParameterId]>,
    row_parameters: Arc<[SchemeRowParameter]>,
    monotype: CallableMonotype,
    explicit_return: bool,
    requirements: CallableRequirements,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableMonotype {
    params: Arc<[SchemeParameter]>,
    return_type: SchemeType,
    return_schema: Option<SchemaExpectation<SchemeType>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypeScheme {
    kind: crate::sema::types::CallableKind,
    callable: Arc<CallableScheme>,
    effects: CheckedEffects,
    inferred_effects: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CallableApplicationBinding {
    Selected(super::arguments::StaticArgumentBinding),
    Forwarded(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(crate) enum SealedOperationBinding {
    Selected(super::sealed_operation::SealedOperationSignature),
    Forwarded(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(crate) enum SealedMethodBinding {
    Selected(xsh_registry::signature::RegisteredMethodId),
    Forwarded(u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallInstantiation {
    type_arguments: BTreeMap<SchemeTypeParameterId, SchemeType>,
    row_arguments: BTreeMap<SchemeRowParameterId, RowEvidence>,
    effect_arguments: BTreeMap<SchemeEffectParameterId, EffectContract<SchemeEffectParameterId>>,
    application_bindings: Vec<CallableApplicationBinding>,
    operation_bindings: Vec<SealedOperationBinding>,
    method_bindings: Vec<SealedMethodBinding>,
    wrapper_bindings: Vec<super::wrapper_operation::WrapperBinding>,
    guard_bindings: super::guarded_requirements::GuardBindings,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RowEvidence {
    fields: BTreeMap<Name, SchemeType>,
    tail: SchemeRowTail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RegisteredArgumentCheck {
    Unsigned(Type),
    UIntMapKey,
}

#[derive(Clone, Debug)]
pub(crate) struct RegisteredCallPlan {
    pub(crate) receiver: Option<crate::modules::MethodReceiver>,
    pub(crate) name: Arc<str>,
    pub(crate) signature: &'static crate::modules::ModuleFnSig,
    pub(crate) argument_slots: Arc<[usize]>,
    pub(crate) argument_checks: Arc<[Option<RegisteredArgumentCheck>]>,
}

impl PartialEq for RegisteredCallPlan {
    fn eq(&self, other: &Self) -> bool {
        self.receiver == other.receiver
            && self.name == other.name
            && std::ptr::eq(self.signature, other.signature)
            && self.argument_slots == other.argument_slots
            && self.argument_checks == other.argument_checks
    }
}

impl Eq for RegisteredCallPlan {}

impl RegisteredCallPlan {
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.name.len()
            + std::mem::size_of_val(self.argument_slots.as_ref())
            + std::mem::size_of_val(self.argument_checks.as_ref())
            + self.argument_checks.iter().flatten().map(|check| match check {
                RegisteredArgumentCheck::Unsigned(ty) => ty.retained_bytes().saturating_sub(std::mem::size_of::<Type>()),
                RegisteredArgumentCheck::UIntMapKey => 0,
            }).sum::<usize>()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SchemeProcReturnSite {
    pub(crate) application: u32,
    pub(crate) envelope: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NonmaterialConstructor {
    EmptyList,
    EmptyMap,
    Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemeBodyFacts {
    scheme: Arc<TypeScheme>,
    expression_types: BTreeMap<ExprId, SchemeType>,
    constructor_producers: BTreeMap<ExprId, NonmaterialConstructor>,
    constructor_instances: BTreeMap<ExprId, super::constants::CheckedRecordConstructor<SchemeType>>,
    tail_types: BTreeMap<StmtId, SchemeType>,
    registered_calls: BTreeMap<ExprId, Arc<RegisteredCallPlan>>,
    read_bindings: BTreeSet<super::check::BindingId>,
    proc_return_sites: BTreeMap<Span, SchemeProcReturnSite>,
    operation_sites: BTreeMap<ExprId, u32>,
    method_sites: BTreeMap<ExprId, u32>,
    wrapper_sites: BTreeMap<ExprId, u32>,
    guard_scopes: super::guarded_requirements::GuardBodyScopes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemeBuildError {
    UnresolvedSealedOperation,
    UnresolvedMethodFamily,
    UnsupportedMethodFamily,
    AmbiguousMethodFamily,
    UnresolvedWrapper,
    UnresolvedGuardScope,
    RecoveryType,
    UnresolvedType,
    UnresolvedRow,
    #[cfg(test)]
    RigidVariable,
    InvalidValidatedBase,
    UnboundTypeParameter,
    UnboundRowParameter,
    UnboundEffectParameter,
    UnusedEffectParameter,
    DuplicateEffectParameter,
    UnusedTypeParameter,
    UnusedRowParameter,
    DuplicateTypeParameter,
    DuplicateRowParameter,
    RowLacksMismatch,
    UnresolvedEffects,
    UnresolvedCallableApplication,
    UnresolvedSchema,
    DynamicType,
    BodyTypeNotBound,
    MapKey,
    Conversion,
}

impl TypeScheme {
    #[cfg(test)]
    pub(crate) fn new(
        kind: crate::sema::types::CallableKind,
        type_parameters: Vec<SchemeTypeParameterId>,
        row_parameters: Vec<SchemeRowParameter>,
        params: Vec<SchemeParameter>,
        return_type: SchemeType,
        return_schema: Option<SchemaExpectation<SchemeType>>,
        explicit_return: bool,
        effects: Option<Vec<crate::syntax::node::Effect>>,
        inferred_effects: bool,
    ) -> Result<Self, SchemeBuildError> {
        let callable = Arc::new(CallableScheme::new(
            type_parameters,
            row_parameters,
            params,
            return_type,
            return_schema,
            explicit_return,
            CallableRequirements::default(),
        )?);
        Self::with_callable(kind, callable, CheckedEffects::from_clause(effects), inferred_effects)
    }

    pub(crate) fn with_callable(
        kind: crate::sema::types::CallableKind,
        callable: Arc<CallableScheme>,
        effects: CheckedEffects,
        inferred_effects: bool,
    ) -> Result<Self, SchemeBuildError> {
        let effect_ids = callable.requirements().effect_parameters.iter().copied().collect();
        validate_effect_contract(effects.contract(), &effect_ids, &mut BTreeSet::new())?;
        if callable.is_quantified() {
            match kind {
                crate::sema::types::CallableKind::Pure => {}
                crate::sema::types::CallableKind::Proc | crate::sema::types::CallableKind::Stream if effects.is_known() => {}
                crate::sema::types::CallableKind::Proc | crate::sema::types::CallableKind::Stream => return Err(SchemeBuildError::UnresolvedEffects),
            }
        }
        Ok(Self {
            kind,
            callable,
            effects,
            inferred_effects,
        })
    }

    pub(crate) fn callable(&self) -> &Arc<CallableScheme> {
        &self.callable
    }

    pub(crate) fn kind(&self) -> crate::sema::types::CallableKind {
        self.kind
    }

    pub(crate) fn type_parameters(&self) -> &[SchemeTypeParameterId] {
        self.callable.type_parameters()
    }

    pub(crate) fn row_parameters(&self) -> &[SchemeRowParameter] {
        self.callable.row_parameters()
    }

    pub(crate) fn params(&self) -> &[SchemeParameter] {
        self.callable.params()
    }

    pub(crate) fn return_type(&self) -> &SchemeType {
        self.callable.return_type()
    }

    pub(crate) fn method_path_return_matches(&self, actual: &SchemeType) -> bool {
        if !self.explicit_return() { return false; }
        let payload = match (self.kind(), self.return_type(), actual) {
            (crate::sema::types::CallableKind::Pure, SchemeType::Ground(Type::Path), actual) => actual,
            (crate::sema::types::CallableKind::Proc, SchemeType::Result(ok, error), SchemeType::Result(actual_ok, actual_error))
                if **ok == SchemeType::Ground(Type::Path) && actual_error.matches_body_return(error) => actual_ok,
            _ => return false,
        };
        self.method_path_return_payload(payload)
    }

    pub(crate) fn method_path_return_payload(&self, actual: &SchemeType) -> bool {
        if !self.explicit_return() || !matches!(actual, SchemeType::Parameter(_)) { return false; }
        let header = match (self.kind(), self.return_type()) {
            (crate::sema::types::CallableKind::Pure, SchemeType::Ground(Type::Path)) => true,
            (crate::sema::types::CallableKind::Proc, SchemeType::Result(ok, _)) => **ok == SchemeType::Ground(Type::Path),
            _ => false,
        };
        header && self.callable().requirements().methods.iter().any(|method| method.result == *actual && crate::modules::api_spec().method_family_returns_paths(method.family))
    }

    pub(crate) fn return_schema(&self) -> Option<&SchemaExpectation<SchemeType>> {
        self.callable.return_schema()
    }

    pub(crate) fn explicit_return(&self) -> bool {
        self.callable.explicit_return()
    }

    pub(crate) fn effects(&self) -> &CheckedEffects {
        &self.effects
    }

    pub(crate) fn inferred_effects(&self) -> bool {
        self.inferred_effects
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            .saturating_add(self.callable.retained_bytes())
            .saturating_add(self.effects.retained_bytes().saturating_sub(size_of::<CheckedEffects>()))
    }

    pub(crate) fn map_ground_types(&self, mut map: impl FnMut(&mut Type)) -> Self {
        let callable = self.callable.map_ground_types(&mut map);
        Self {
            kind: self.kind,
            callable: Arc::new(callable),
            effects: self.effects.clone(),
            inferred_effects: self.inferred_effects,
        }
    }
}

impl CallableScheme {
    pub(crate) fn new(
        type_parameters: Vec<SchemeTypeParameterId>,
        row_parameters: Vec<SchemeRowParameter>,
        params: Vec<SchemeParameter>,
        return_type: SchemeType,
        return_schema: Option<SchemaExpectation<SchemeType>>,
        explicit_return: bool,
        requirements: CallableRequirements,
    ) -> Result<Self, SchemeBuildError> {
        let map_keys = requirements.map_key_parameters.iter().copied().collect::<BTreeSet<_>>();
        if map_keys.len() != requirements.map_key_parameters.len()
            || !map_keys.is_subset(&type_parameters.iter().copied().collect())
        { return Err(SchemeBuildError::MapKey); }
        if requirements.conversion_parameters.iter().any(|(parameter, conversions)| {
            !type_parameters.contains(parameter) || conversions.is_empty() || !conversions.is_satisfiable(map_keys.contains(parameter))
        }) { return Err(SchemeBuildError::Conversion); }
        let effect_ids = requirements.effect_parameters.iter().copied().collect::<BTreeSet<_>>();
        if effect_ids.len() != requirements.effect_parameters.len() {
            return Err(SchemeBuildError::DuplicateEffectParameter);
        }
        let mut used_effects = BTreeSet::new();
        let type_ids = type_parameters.iter().copied().collect::<BTreeSet<_>>();
        if type_ids.len() != type_parameters.len() {
            return Err(SchemeBuildError::DuplicateTypeParameter);
        }
        let row_ids = row_parameters
            .iter()
            .map(|parameter| parameter.id)
            .collect::<BTreeSet<_>>();
        if row_ids.len() != row_parameters.len() {
            return Err(SchemeBuildError::DuplicateRowParameter);
        }
        let mut used_types = BTreeSet::new();
        let mut used_rows = BTreeSet::new();
        let mut row_requirements = BTreeMap::<SchemeRowParameterId, BTreeSet<Name>>::new();
        for parameter in &params {
            if let Some(schema) = &parameter.schema_expectation { validate_schema_types(schema, &mut |ty| ty.validate(&type_ids, &row_ids, &effect_ids, &mut used_effects, &mut used_types, &mut used_rows, &mut row_requirements))?; }
            parameter.ty.validate(
                &type_ids,
                &row_ids,
                &effect_ids,
                &mut used_effects,
                &mut used_types,
                &mut used_rows,
                &mut row_requirements,
            )?;
        }
        return_type.validate(
            &type_ids,
            &row_ids,
            &effect_ids,
            &mut used_effects,
            &mut used_types,
            &mut used_rows,
            &mut row_requirements,
        )?;
        if let Some(schema) = &return_schema { validate_schema_types(schema, &mut |ty| ty.validate(&type_ids, &row_ids, &effect_ids, &mut used_effects, &mut used_types, &mut used_rows, &mut row_requirements))?; }
        visit_requirement_terms(&requirements, |term| match term {
            RequirementTerm::Type(ty) => ty.validate(&type_ids, &row_ids, &effect_ids, &mut used_effects, &mut used_types, &mut used_rows, &mut row_requirements),
            RequirementTerm::Effects(contract) => validate_effect_contract(contract, &effect_ids, &mut used_effects),
            RequirementTerm::Parameter(parameter) => if type_ids.contains(&parameter) { Ok(()) } else { Err(SchemeBuildError::UnboundTypeParameter) },
            RequirementTerm::RowParameter(parameter) => if row_ids.contains(&parameter) { used_rows.insert(parameter); row_requirements.entry(parameter).or_default(); Ok(()) } else { Err(SchemeBuildError::UnboundRowParameter) },
        })?;
        validate_guard_requirements(&requirements.guards, &type_ids, &row_ids, &effect_ids, &mut used_effects, &mut used_types, &mut used_rows, &mut row_requirements)?;
        if used_types != type_ids {
            return Err(SchemeBuildError::UnusedTypeParameter);
        }
        if used_rows != row_ids {
            return Err(SchemeBuildError::UnusedRowParameter);
        }
        if used_effects != effect_ids {
            return Err(SchemeBuildError::UnusedEffectParameter);
        }
        if row_parameters.iter().any(|parameter| {
            !row_requirements
                .get(&parameter.id)
                .is_some_and(|required| required.is_subset(&parameter.lacks))
        }) {
            return Err(SchemeBuildError::RowLacksMismatch);
        }
        Ok(Self {
            type_parameters: type_parameters.into(),
            row_parameters: row_parameters.into(),
            monotype: CallableMonotype::new(params, return_type, return_schema),
            explicit_return,
            requirements,
        })
    }

    pub(crate) fn requirements(&self) -> &CallableRequirements {
        &self.requirements
    }

    pub(crate) fn is_quantified(&self) -> bool {
        !self.type_parameters.is_empty() || !self.row_parameters.is_empty()
            || !self.requirements.effect_parameters.is_empty()
            || !self.requirements.guards.is_empty()
    }

    pub(crate) fn type_parameters(&self) -> &[SchemeTypeParameterId] {
        &self.type_parameters
    }

    pub(crate) fn row_parameters(&self) -> &[SchemeRowParameter] {
        &self.row_parameters
    }

    pub(crate) fn params(&self) -> &[SchemeParameter] {
        self.monotype.params()
    }

    pub(crate) fn return_type(&self) -> &SchemeType {
        self.monotype.return_type()
    }

    pub(crate) fn return_schema(&self) -> Option<&SchemaExpectation<SchemeType>> {
        self.monotype.return_schema()
    }

    pub(crate) fn explicit_return(&self) -> bool {
        self.explicit_return
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>();
        bytes = bytes.saturating_add(self.type_parameters.len() * size_of::<SchemeTypeParameterId>());
        bytes = bytes.saturating_add(self.row_parameters.len() * size_of::<SchemeRowParameter>());
        bytes = bytes.saturating_add(self.monotype.retained_bytes().saturating_sub(size_of::<CallableMonotype>()));
        bytes = bytes.saturating_add(self.requirements.retained_bytes().saturating_sub(size_of::<CallableRequirements>()));
        bytes = bytes.saturating_add(
            self.row_parameters
                .iter()
                .map(|parameter| {
                    parameter.lacks.len() * size_of::<Name>()
                        + parameter
                            .lacks
                            .iter()
                            .map(|name| name.as_str().len())
                            .sum::<usize>()
                })
                .sum::<usize>(),
        );
        bytes
    }

    fn map_ground_types(&self, map: &mut impl FnMut(&mut Type)) -> Self {
        Self {
            type_parameters: self.type_parameters.clone(),
            row_parameters: self.row_parameters.clone(),
            monotype: self.monotype.map_ground_types(map),
            explicit_return: self.explicit_return,
            requirements: self.requirements.map_ground_types(map),
        }
    }
}

pub(super) enum RequirementTerm<'a> {
    Type(&'a SchemeType),
    Effects(&'a EffectContract<SchemeEffectParameterId>),
    Parameter(SchemeTypeParameterId),
    RowParameter(SchemeRowParameterId),
}

pub(super) fn visit_requirement_terms(requirements: &CallableRequirements, mut visit: impl FnMut(RequirementTerm<'_>) -> Result<(), SchemeBuildError>) -> Result<(), SchemeBuildError> {
    let CallableRequirements { map_key_parameters, conversion_parameters, iteration_parameters, effect_parameters: _, effect_predicates, applications, operations, methods, wrappers, guards: _, guard_calls } = requirements;
    for call in guard_calls {
        call.validate_target()?;
        for ty in call.type_arguments.values() { visit(RequirementTerm::Type(ty))?; }
        for row in call.row_arguments.values() {
            for ty in row.fields.values() { visit(RequirementTerm::Type(ty))?; }
            if let SchemeRowTail::Parameter(parameter) = row.tail { visit(RequirementTerm::RowParameter(parameter))?; }
        }
        for contract in call.effect_arguments.values() { visit(RequirementTerm::Effects(contract))?; }
    }
    for parameter in map_key_parameters.iter().chain(conversion_parameters.keys()) { visit(RequirementTerm::Parameter(*parameter))?; }
    for (parameter, iteration) in iteration_parameters {
        visit(RequirementTerm::Parameter(*parameter))?;
        for ty in iteration.types() { visit(RequirementTerm::Type(ty))?; }
    }
    for application in applications {
        for ty in application.types() { visit(RequirementTerm::Type(ty))?; }
        if let Some(application) = application.invocation() { visit(RequirementTerm::Effects(&application.effects))?; }
    }
    for wrapper in wrappers {
        for ty in wrapper.types() { visit(RequirementTerm::Type(ty))?; }
        if let super::wrapper_operation::WrapperRequirementKind::Field { effects, .. } = &wrapper.kind { visit(RequirementTerm::Effects(effects))?; }
    }
    validate_wrapper_group_contracts(wrappers)?;
    for operation in operations {
        for ty in operation.types() { visit(RequirementTerm::Type(ty))?; }
    }
    for method in methods {
        if method.argument_slots.len() != method.arguments.len() { return Err(SchemeBuildError::UnresolvedMethodFamily); }
        for ty in std::iter::once(&method.receiver).chain(method.arguments.iter()).chain(std::iter::once(&method.result)) {
            if let Some(error) = ty.body_build_error() { return Err(error); }
            visit(RequirementTerm::Type(ty))?;
        }
    }
    for predicate in effect_predicates {
        for contract in predicate.contracts() { visit(RequirementTerm::Effects(contract))?; }
    }
    Ok(())
}

#[derive(Default)]
struct GuardUsage {
    types: BTreeSet<SchemeTypeParameterId>,
    rows: BTreeSet<SchemeRowParameterId>,
    effects: BTreeSet<SchemeEffectParameterId>,
}

fn validate_guard_type(
    ty: &SchemeType, scope: Option<super::guarded_requirements::GuardScopeId>, ownership: &super::guarded_requirements::GuardOwnership<'_>, usage: &mut GuardUsage,
    row_requirements: &mut BTreeMap<SchemeRowParameterId, BTreeSet<Name>>,
) -> Result<(), SchemeBuildError> {
    let mut types = BTreeSet::new();
    let mut rows = BTreeSet::new();
    let mut effects = BTreeSet::new();
    ty.validate(ownership.types(), ownership.rows(), ownership.effects(), &mut effects, &mut types, &mut rows, row_requirements)?;
    ownership.check_types(scope, &types)?;
    ownership.check_rows(scope, &rows)?;
    ownership.check_effects(scope, &effects)?;
    usage.types.extend(types);
    usage.rows.extend(rows);
    usage.effects.extend(effects);
    Ok(())
}

fn validate_guard_requirements(
    guards: &[Arc<super::guarded_requirements::GuardRequirements>],
    root_types: &BTreeSet<SchemeTypeParameterId>, root_rows: &BTreeSet<SchemeRowParameterId>, root_effects: &BTreeSet<SchemeEffectParameterId>,
    used_effects: &mut BTreeSet<SchemeEffectParameterId>, used_types: &mut BTreeSet<SchemeTypeParameterId>, used_rows: &mut BTreeSet<SchemeRowParameterId>,
    row_requirements: &mut BTreeMap<SchemeRowParameterId, BTreeSet<Name>>,
) -> Result<(), SchemeBuildError> {
    let mut ownership = super::guarded_requirements::GuardOwnership::new(root_types, root_rows, root_effects);
    ownership.collect(guards)?;
    let mut usage = GuardUsage::default();
    let mut pending = guards.iter().rev().map(|guard| (guard, None)).collect::<Vec<_>>();
    while let Some((guard, parent)) = pending.pop() {
        validate_guard_type(&guard.source, parent, &ownership, &mut usage, row_requirements)?;
        validate_guard_type(&guard.view, Some(guard.scope), &ownership, &mut usage, row_requirements)?;
        for read in &guard.reads {
            validate_guard_type(&read.source, parent, &ownership, &mut usage, row_requirements)?;
            validate_guard_type(&read.view, Some(guard.scope), &ownership, &mut usage, row_requirements)?;
        }
        visit_requirement_terms(&guard.requirements, |term| match term {
            RequirementTerm::Type(ty) => validate_guard_type(ty, Some(guard.scope), &ownership, &mut usage, row_requirements),
            RequirementTerm::Parameter(parameter) => ownership.check_type(Some(guard.scope), parameter),
            RequirementTerm::RowParameter(parameter) => {
                ownership.check_rows(Some(guard.scope), &BTreeSet::from([parameter]))?;
                usage.rows.insert(parameter);
                row_requirements.entry(parameter).or_default();
                Ok(())
            },
            RequirementTerm::Effects(contract) => {
                let mut effects = BTreeSet::new();
                validate_effect_contract(contract, ownership.effects(), &mut effects)?;
                ownership.check_effects(Some(guard.scope), &effects)?;
                usage.effects.extend(effects);
                Ok(())
            },
        })?;
        let map_keys = guard.requirements.map_key_parameters.iter().copied().collect::<BTreeSet<_>>();
        if map_keys.len() != guard.requirements.map_key_parameters.len() { return Err(SchemeBuildError::MapKey); }
        if guard.requirements.conversion_parameters.iter().any(|(parameter, conversions)| conversions.is_empty() || !conversions.is_satisfiable(map_keys.contains(parameter))) { return Err(SchemeBuildError::Conversion); }
        pending.extend(guard.requirements.guards.iter().rev().map(|child| (child, Some(guard.scope))));
    }
    ownership.check_used(&usage.types, &usage.rows, &usage.effects, row_requirements)?;
    used_types.extend(usage.types.into_iter().filter(|parameter| root_types.contains(parameter)));
    used_rows.extend(usage.rows.into_iter().filter(|parameter| root_rows.contains(parameter)));
    used_effects.extend(usage.effects.into_iter().filter(|parameter| root_effects.contains(parameter)));
    Ok(())
}

fn validate_effect_contract(
    contract: &EffectContract<SchemeEffectParameterId>,
    effect_ids: &BTreeSet<SchemeEffectParameterId>,
    used_effects: &mut BTreeSet<SchemeEffectParameterId>,
) -> Result<(), SchemeBuildError> {
    if let EffectContract::Known(expression) = contract {
        for (id, _) in expression.variables() {
            if !effect_ids.contains(id) {
                return Err(SchemeBuildError::UnboundEffectParameter);
            }
            used_effects.insert(*id);
        }
    }
    Ok(())
}

fn validate_wrapper_group_contracts(
    wrappers: &[super::wrapper_operation::WrapperRequirement<SchemeType, SchemeEffectParameterId>],
) -> Result<(), SchemeBuildError> {
    use super::wrapper_operation::{WrapperBoundary, WrapperCompletionContributions, WrapperGroupRef, WrapperRequirementKind};
    let mut found_field = false;
    for (index, wrapper) in wrappers.iter().enumerate() {
        match &wrapper.kind {
            WrapperRequirementKind::ProcCompletion { group: WrapperGroupRef::Ordinal(group), returned, contributions } => {
                if found_field || *group as usize != index || !matches!(returned, SchemeType::Result(..)) || matches!(contributions, WrapperCompletionContributions::Pending) {
                    return Err(SchemeBuildError::UnresolvedWrapper);
                }
            }
            WrapperRequirementKind::ProcCompletion { .. } => return Err(SchemeBuildError::UnresolvedWrapper),
            WrapperRequirementKind::Field { boundary: WrapperBoundary::ProcCompletion { returned, group: WrapperGroupRef::Ordinal(group) }, .. } => {
                found_field = true;
                let Some(owner_return) = wrappers.get(*group as usize).and_then(|owner| match &owner.kind {
                    WrapperRequirementKind::ProcCompletion { group: WrapperGroupRef::Ordinal(owner), returned, contributions: WrapperCompletionContributions::Ready(_) } if owner == group => Some(returned),
                    _ => None,
                }) else { return Err(SchemeBuildError::UnresolvedWrapper); };
                if owner_return != returned { return Err(SchemeBuildError::UnresolvedWrapper); }
            }
            WrapperRequirementKind::Field { boundary: WrapperBoundary::ProcCompletion { .. }, .. } => return Err(SchemeBuildError::UnresolvedWrapper),
            _ => found_field = true,
        }
    }
    Ok(())
}

impl CallableRequirements {
    pub(crate) fn guard(&self, id: super::guarded_requirements::GuardScopeId) -> Option<&super::guarded_requirements::GuardRequirements> {
        let mut guards = self.guards.as_slice();
        for _ in 0..=super::constraints::MAX_TYPE_DEPTH {
            let end = guards.partition_point(|guard| guard.scope <= id);
            let guard = guards.get(end.checked_sub(1)?)?;
            if guard.scope == id { return Some(guard); }
            guards = &guard.requirements.guards;
        }
        None
    }

    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>()
            .saturating_add(self.map_key_parameters.capacity() * size_of::<SchemeTypeParameterId>())
            .saturating_add(self.conversion_parameters.len() * size_of::<(SchemeTypeParameterId, ValueConversionSet)>())
            .saturating_add(self.effect_parameters.capacity() * size_of::<SchemeEffectParameterId>())
            .saturating_add(self.effect_predicates.capacity() * size_of::<EffectPredicate<SchemeEffectParameterId>>())
            .saturating_add(self.applications.capacity() * size_of::<CallableApplication<SchemeType, SchemeEffectParameterId>>());
        bytes = bytes.saturating_add(self.iteration_parameters.len() * size_of::<(SchemeTypeParameterId, IterationRequirement<SchemeType>)>());
        for iteration in self.iteration_parameters.values() {
            for ty in iteration.types() { bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<SchemeType>())); }
        }
        bytes = bytes.saturating_add(self.operations.capacity() * size_of::<super::sealed_operation::SealedOperation<SchemeType>>());
        for operation in &self.operations {
            bytes = bytes.saturating_add(operation.owned_fields_bytes());
            for ty in operation.types() { bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<SchemeType>())); }
        }
        bytes = bytes.saturating_add(self.methods.capacity() * size_of::<MethodFamilyRequirement<SchemeType>>());
        for method in &self.methods {
            bytes = bytes.saturating_add(method.argument_slots.capacity() * size_of::<u32>());
            bytes = bytes.saturating_add(method.arguments.capacity() * size_of::<SchemeType>());
            for ty in std::iter::once(&method.receiver).chain(method.arguments.iter()).chain(std::iter::once(&method.result)) {
                bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<SchemeType>()));
            }
        }
        bytes = bytes.saturating_add(self.wrappers.capacity() * size_of::<super::wrapper_operation::WrapperRequirement<SchemeType, SchemeEffectParameterId>>());
        for wrapper in &self.wrappers {
            for ty in wrapper.types() { bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<SchemeType>())); }
            match &wrapper.kind {
                super::wrapper_operation::WrapperRequirementKind::Field { effects, .. } => bytes = bytes.saturating_add(effects.retained_bytes().saturating_sub(size_of::<EffectContract<SchemeEffectParameterId>>())),
                super::wrapper_operation::WrapperRequirementKind::Capture { contributions, .. } => bytes = bytes.saturating_add(contributions.len() * size_of::<super::wrapper_operation::WrapperErrorContribution<SchemeType>>()),
                super::wrapper_operation::WrapperRequirementKind::ProcCompletion { contributions: super::wrapper_operation::WrapperCompletionContributions::Ready(contributions), .. } => bytes = bytes.saturating_add(contributions.len() * size_of::<super::wrapper_operation::WrapperCompletionContribution<SchemeType>>()),
                _ => {}
            }
        }
        bytes = bytes.saturating_add(self.guards.capacity() * size_of::<Arc<super::guarded_requirements::GuardRequirements>>());
        for guard in &self.guards {
            bytes = bytes.saturating_add(size_of::<super::guarded_requirements::GuardRequirements>());
            bytes = bytes.saturating_add(guard.source.retained_bytes().saturating_sub(size_of::<SchemeType>()))
                .saturating_add(guard.view.retained_bytes().saturating_sub(size_of::<SchemeType>()))
                .saturating_add(guard.type_parameters.capacity() * size_of::<SchemeTypeParameterId>())
                .saturating_add(guard.row_parameters.capacity() * size_of::<SchemeRowParameter>())
                .saturating_add(guard.reads.capacity() * size_of::<super::guarded_requirements::GuardReadRequirement>())
                .saturating_add(guard.requirements.retained_bytes());
            for row in &guard.row_parameters { bytes = bytes.saturating_add(row.lacks.len() * size_of::<Name>()); }
            for read in &guard.reads { bytes = bytes.saturating_add(read.source.retained_bytes().saturating_sub(size_of::<SchemeType>())).saturating_add(read.view.retained_bytes().saturating_sub(size_of::<SchemeType>())); }
        }
        bytes = bytes.saturating_add(self.guard_calls.capacity() * size_of::<Arc<super::guarded_requirements::GuardCallRequirement>>());
        for call in &self.guard_calls {
            bytes = bytes.saturating_add(size_of::<super::guarded_requirements::GuardCallRequirement>())
                .saturating_add(call.type_arguments.len() * size_of::<SchemeTypeParameterId>())
                .saturating_add(call.type_arguments.values().map(SchemeType::retained_bytes).sum::<usize>())
                .saturating_add(call.row_arguments.len() * size_of::<SchemeRowParameterId>())
                .saturating_add(call.row_arguments.values().map(RowEvidence::retained_bytes).sum::<usize>())
                .saturating_add(call.effect_arguments.len() * size_of::<SchemeEffectParameterId>())
                .saturating_add(call.effect_arguments.values().map(EffectContract::retained_bytes).sum::<usize>());
        }
        for predicate in &self.effect_predicates {
            for contract in predicate.contracts() {
                bytes = bytes.saturating_add(contract.retained_bytes().saturating_sub(size_of::<EffectContract<SchemeEffectParameterId>>()));
            }
        }
        for application in &self.applications {
            bytes = bytes.saturating_add(std::mem::size_of_val(application.arguments()));
            match application {
                CallableApplication::Invocation(value) => {
                    bytes = bytes.saturating_add(value.effects.retained_bytes().saturating_sub(size_of::<EffectContract<SchemeEffectParameterId>>()));
                    bytes = bytes.saturating_add(std::mem::size_of_val(value.proc_returns.as_ref()));
                    bytes = bytes.saturating_add(value.proc_returns.iter().map(|envelope| std::mem::size_of_val(envelope.propagated_errors.as_ref())).sum::<usize>());
                }
                CallableApplication::KnownArguments(value) => bytes = bytes.saturating_add(std::mem::size_of_val(value.parameters.as_ref())),
            }
            for ty in application.types() {
                bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<SchemeType>()));
            }
        }
        bytes
    }

    fn map_ground_types(&self, map: &mut impl FnMut(&mut Type)) -> Self {
        let mut mapped = self.clone();
        for guard in &mut mapped.guards {
            let guard = Arc::make_mut(guard);
            guard.source.map_ground_types(map);
            guard.view.map_ground_types(map);
            for read in &mut guard.reads { read.source.map_ground_types(map); read.view.map_ground_types(map); }
            guard.requirements = Box::new(guard.requirements.map_ground_types(map));
        }
        for operation in &mut mapped.operations {
            for ty in operation.types_mut() { ty.map_ground_types(map); }
        }
        for method in &mut mapped.methods {
            method.receiver.map_ground_types(map);
            method.result.map_ground_types(map);
            for argument in &mut method.arguments { argument.map_ground_types(map); }
        }
        mapped.wrappers = self.wrappers.iter().map(|wrapper| wrapper.try_map(map, |map, ty| {
            let mut ty = ty.clone(); ty.map_ground_types(map); Ok::<_, std::convert::Infallible>(ty)
        }, |_, contract| Ok(contract.clone())).unwrap()).collect();
        for iteration in mapped.iteration_parameters.values_mut() {
            iteration.item.map_ground_types(map);
            match &mut iteration.policy { IterationPolicy::IterablePropagates(error) | IterationPolicy::PipelinePropagates(error) => error.map_ground_types(map), _ => {} }
        }
        for application in &mut mapped.applications {
            application.for_each_type_mut(|ty| ty.map_ground_types(map));
        }
        mapped
    }
}

impl CallableMonotype {
    pub(crate) fn new(
        params: Vec<SchemeParameter>,
        return_type: SchemeType,
        return_schema: Option<SchemaExpectation<SchemeType>>,
    ) -> Self {
        Self { params: params.into(), return_type, return_schema }
    }

    pub(crate) fn params(&self) -> &[SchemeParameter] {
        &self.params
    }

    pub(crate) fn return_type(&self) -> &SchemeType {
        &self.return_type
    }

    pub(crate) fn return_schema(&self) -> Option<&SchemaExpectation<SchemeType>> {
        self.return_schema.as_ref()
    }

    pub(crate) fn has_schema_contract(&self) -> bool {
        self.return_schema.is_some() || self.params.iter().any(|parameter| parameter.schema_expectation.is_some())
    }

    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>() + self.params.len() * size_of::<SchemeParameter>();
        let mut counted_schema_children = BTreeSet::new();
        for parameter in self.params.iter() {
            bytes = bytes
                .saturating_add(parameter.name.as_str().len())
                .saturating_add(
                    parameter
                        .ty
                        .retained_bytes()
                        .saturating_sub(size_of::<SchemeType>()),
                );
            if let Some(expectation) = &parameter.schema_expectation {
                bytes = bytes.saturating_add(schema_expectation_retained_bytes(
                    expectation,
                    &mut counted_schema_children,
                    SchemeType::retained_bytes,
                ));
            }
        }
        bytes = bytes
            .saturating_add(
                self.return_type
                    .retained_bytes()
                    .saturating_sub(size_of::<SchemeType>()),
            )
            .saturating_add(self.return_schema.as_ref().map_or(0, |expectation| {
                schema_expectation_retained_bytes(expectation, &mut counted_schema_children, SchemeType::retained_bytes)
            }));
        bytes
    }

    fn map_ground_types(&self, map: &mut impl FnMut(&mut Type)) -> Self {
        let mut params = self.params.to_vec();
        for parameter in &mut params {
            parameter.ty.map_ground_types(map);
            if let Some(schema) = &mut parameter.schema_expectation { schema.map_ground_types(map); }
        }
        let mut return_type = self.return_type.clone();
        return_type.map_ground_types(map);
        let mut return_schema = self.return_schema.clone();
        if let Some(schema) = &mut return_schema { schema.map_ground_types(map); }
        Self { params: params.into(), return_type, return_schema }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SchemeBindingScope<'a> {
    pub(super) types: &'a [SchemeTypeParameterId],
    pub(super) rows: &'a [SchemeRowParameter],
    pub(super) requirements: &'a CallableRequirements,
}

impl CallInstantiation {
    #[cfg(test)]
    pub(crate) fn new(
        callable: &CallableScheme,
        type_arguments: BTreeMap<SchemeTypeParameterId, SchemeType>,
        row_arguments: BTreeMap<SchemeRowParameterId, RowEvidence>,
        effect_arguments: BTreeMap<SchemeEffectParameterId, EffectContract<SchemeEffectParameterId>>,
        application_bindings: Vec<CallableApplicationBinding>,
        operation_bindings: Vec<SealedOperationBinding>,
        wrapper_bindings: Vec<super::wrapper_operation::WrapperBinding>,
        caller_types: &[SchemeTypeParameterId],
        caller_rows: &[SchemeRowParameter],
        caller_effects: &[SchemeEffectParameterId],
        caller_applications: &[CallableApplication<SchemeType, SchemeEffectParameterId>],
        caller_operations: &[super::sealed_operation::SealedOperation<SchemeType>],
        caller_wrappers: &[super::wrapper_operation::WrapperRequirement<SchemeType, SchemeEffectParameterId>],
        caller_map_keys: &[SchemeTypeParameterId],
        caller_conversions: &BTreeMap<SchemeTypeParameterId, ValueConversionSet>,
    ) -> Result<Self, SchemeBuildError> {
        Self::with_method_bindings(
            callable, type_arguments, row_arguments, effect_arguments, application_bindings, operation_bindings,
            Vec::new(), wrapper_bindings, super::guarded_requirements::GuardBindings::default(), caller_types, caller_rows, caller_effects, caller_applications, caller_operations, &[], &[], caller_wrappers, caller_map_keys, caller_conversions,
        )
    }

    pub(crate) fn with_method_bindings(
        callable: &CallableScheme,
        type_arguments: BTreeMap<SchemeTypeParameterId, SchemeType>,
        row_arguments: BTreeMap<SchemeRowParameterId, RowEvidence>,
        effect_arguments: BTreeMap<SchemeEffectParameterId, EffectContract<SchemeEffectParameterId>>,
        application_bindings: Vec<CallableApplicationBinding>,
        operation_bindings: Vec<SealedOperationBinding>,
        method_bindings: Vec<SealedMethodBinding>,
        wrapper_bindings: Vec<super::wrapper_operation::WrapperBinding>,
        guard_bindings: super::guarded_requirements::GuardBindings,
        caller_types: &[SchemeTypeParameterId],
        caller_rows: &[SchemeRowParameter],
        caller_effects: &[SchemeEffectParameterId],
        caller_applications: &[CallableApplication<SchemeType, SchemeEffectParameterId>],
        caller_operations: &[super::sealed_operation::SealedOperation<SchemeType>],
        caller_methods: &[MethodFamilyRequirement<SchemeType>],
        caller_guard_calls: &[Arc<super::guarded_requirements::GuardCallRequirement>],
        caller_wrappers: &[super::wrapper_operation::WrapperRequirement<SchemeType, SchemeEffectParameterId>],
        caller_map_keys: &[SchemeTypeParameterId],
        caller_conversions: &BTreeMap<SchemeTypeParameterId, ValueConversionSet>,
    ) -> Result<Self, SchemeBuildError> {
        Self::with_scope_bindings(SchemeBindingScope { types: callable.type_parameters(), rows: callable.row_parameters(), requirements: callable.requirements() }, type_arguments, row_arguments, effect_arguments, application_bindings, operation_bindings, method_bindings, wrapper_bindings, guard_bindings, caller_types, caller_rows, caller_effects, caller_applications, caller_operations, caller_methods, caller_guard_calls, caller_wrappers, caller_map_keys, caller_conversions)
    }

    pub(super) fn with_scope_bindings(
        scope: SchemeBindingScope<'_>,
        type_arguments: BTreeMap<SchemeTypeParameterId, SchemeType>,
        row_arguments: BTreeMap<SchemeRowParameterId, RowEvidence>,
        effect_arguments: BTreeMap<SchemeEffectParameterId, EffectContract<SchemeEffectParameterId>>,
        application_bindings: Vec<CallableApplicationBinding>,
        operation_bindings: Vec<SealedOperationBinding>,
        method_bindings: Vec<SealedMethodBinding>,
        wrapper_bindings: Vec<super::wrapper_operation::WrapperBinding>,
        guard_bindings: super::guarded_requirements::GuardBindings,
        caller_types: &[SchemeTypeParameterId],
        caller_rows: &[SchemeRowParameter],
        caller_effects: &[SchemeEffectParameterId],
        caller_applications: &[CallableApplication<SchemeType, SchemeEffectParameterId>],
        caller_operations: &[super::sealed_operation::SealedOperation<SchemeType>],
        caller_methods: &[MethodFamilyRequirement<SchemeType>],
        caller_guard_calls: &[Arc<super::guarded_requirements::GuardCallRequirement>],
        caller_wrappers: &[super::wrapper_operation::WrapperRequirement<SchemeType, SchemeEffectParameterId>],
        caller_map_keys: &[SchemeTypeParameterId],
        caller_conversions: &BTreeMap<SchemeTypeParameterId, ValueConversionSet>,
    ) -> Result<Self, SchemeBuildError> {
        if type_arguments.keys().copied().collect::<BTreeSet<_>>()
            != scope.types.iter().copied().collect()
        {
            return Err(SchemeBuildError::UnboundTypeParameter);
        }
        if guard_bindings.scopes.len() != scope.requirements.guards.len() || guard_bindings.calls.len() != scope.requirements.guard_calls.len() { return Err(SchemeBuildError::UnresolvedGuardScope); }
        for (template, binding) in scope.requirements.guards.iter().map(|guard| guard.as_ref()).zip(&guard_bindings.scopes)
            .chain(scope.requirements.guard_calls.iter().map(|call| call.template.as_ref()).zip(&guard_bindings.calls))
        {
            if let super::guarded_requirements::GuardBinding::Call(ordinal) = binding {
                let source = caller_guard_calls.get(ordinal.index()).ok_or(SchemeBuildError::UnresolvedGuardScope)?;
                if source.template.as_ref() != template { return Err(SchemeBuildError::UnresolvedGuardScope); }
            }
        }
        for parameter in &scope.requirements.map_key_parameters {
            let actual = type_arguments.get(parameter).ok_or(SchemeBuildError::UnboundTypeParameter)?;
            if !actual.is_map_key_with(caller_map_keys) { return Err(SchemeBuildError::MapKey); }
        }
        for (parameter, conversions) in &scope.requirements.conversion_parameters {
            let actual = type_arguments.get(parameter).ok_or(SchemeBuildError::UnboundTypeParameter)?;
            if !actual.satisfies_conversions(*conversions, caller_conversions) { return Err(SchemeBuildError::Conversion); }
        }
        if row_arguments.keys().copied().collect::<BTreeSet<_>>()
            != scope
                .rows
                .iter()
                .map(|parameter| parameter.id)
                .collect()
        {
            return Err(SchemeBuildError::UnboundRowParameter);
        }
        if effect_arguments.keys().copied().collect::<BTreeSet<_>>()
            != scope.requirements.effect_parameters.iter().copied().collect()
        {
            return Err(SchemeBuildError::UnboundEffectParameter);
        }
        if application_bindings.iter().any(|binding| matches!(binding, CallableApplicationBinding::Forwarded(ordinal) if caller_applications.get(*ordinal as usize).is_none())) {
            return Err(SchemeBuildError::UnresolvedCallableApplication);
        }
        if application_bindings.len() != scope.requirements.applications.len() {
            return Err(SchemeBuildError::UnresolvedCallableApplication);
        }
        if operation_bindings.len() != scope.requirements.operations.len()
            || operation_bindings.iter().any(|binding| matches!(binding, SealedOperationBinding::Forwarded(ordinal) if caller_operations.get(*ordinal as usize).is_none()))
        { return Err(SchemeBuildError::UnresolvedSealedOperation); }
        if method_bindings.len() != scope.requirements.methods.len()
            || method_bindings.iter().zip(&scope.requirements.methods).any(|(binding, method)| match binding {
                SealedMethodBinding::Forwarded(ordinal) => caller_methods.get(*ordinal as usize)
                    .is_none_or(|caller| caller.family != method.family),
                SealedMethodBinding::Selected(id) => xsh_registry::signature::api_spec()
                    .registered_method(*id)
                    .is_none_or(|(receiver, name, overload, _)| xsh_registry::signature::api_spec()
                        .method_family_id(receiver, name, overload) != Some(method.family)),
            })
        { return Err(SchemeBuildError::UnresolvedMethodFamily); }
        if wrapper_bindings.len() != scope.requirements.wrappers.len()
            || wrapper_bindings.iter().any(|binding| matches!(binding, super::wrapper_operation::WrapperBinding::Forwarded(ordinal) if caller_wrappers.get(*ordinal as usize).is_none()))
        { return Err(SchemeBuildError::UnresolvedWrapper); }
        let caller_effect_ids = caller_effects.iter().copied().collect::<BTreeSet<_>>();
        let mut used_effects = BTreeSet::new();
        for contract in effect_arguments.values() {
            validate_effect_contract(contract, &caller_effect_ids, &mut used_effects)?;
        }
        let caller_type_ids = caller_types.iter().copied().collect::<BTreeSet<_>>();
        let caller_row_parameters = caller_rows
            .iter()
            .map(|parameter| (parameter.id, parameter))
            .collect::<BTreeMap<_, _>>();
        let caller_row_ids = caller_row_parameters.keys().copied().collect::<BTreeSet<_>>();
        let mut used_types = BTreeSet::new();
        let mut used_rows = BTreeSet::new();
        let mut row_requirements = BTreeMap::new();
        for ty in type_arguments.values() {
            ty.validate(
                &caller_type_ids,
                &caller_row_ids,
                &caller_effect_ids,
                &mut used_effects,
                &mut used_types,
                &mut used_rows,
                &mut row_requirements,
            )?;
        }
        for parameter in scope.rows {
            let evidence = &row_arguments[&parameter.id];
            for ty in evidence.fields.values() {
                if let Some(error) = ty.row_build_error() {
                    return Err(error);
                }
                ty.validate(
                    &caller_type_ids,
                    &caller_row_ids,
                    &caller_effect_ids,
                    &mut used_effects,
                    &mut used_types,
                    &mut used_rows,
                    &mut row_requirements,
                )?;
            }
            if evidence
                .fields
                .keys()
                .any(|field| parameter.lacks.contains(field))
            {
                return Err(SchemeBuildError::RowLacksMismatch);
            }
            if let SchemeRowTail::Parameter(id) = evidence.tail {
                let Some(caller) = caller_row_parameters.get(&id) else {
                    return Err(SchemeBuildError::UnboundRowParameter);
                };
                if !parameter
                    .lacks
                    .iter()
                    .chain(evidence.fields.keys())
                    .all(|field| caller.lacks.contains(field))
                {
                    return Err(SchemeBuildError::RowLacksMismatch);
                }
            }
        }
        if row_requirements.iter().any(|(id, required)| {
            caller_row_parameters
                .get(id)
                .is_none_or(|caller| !required.is_subset(&caller.lacks))
        }) {
            return Err(SchemeBuildError::RowLacksMismatch);
        }
        Ok(Self {
            type_arguments, row_arguments, effect_arguments, application_bindings, operation_bindings, method_bindings, wrapper_bindings,
            guard_bindings,
        })
    }

    pub(crate) fn map_ground_types(&self, map: &mut impl FnMut(&mut Type)) -> Self {
        let mut mapped = self.clone();
        for guard in mapped.guard_bindings.scopes.iter_mut().chain(&mut mapped.guard_bindings.calls) {
            match guard {
                super::guarded_requirements::GuardBinding::NonNull(binding) | super::guarded_requirements::GuardBinding::Optional(binding) => **binding = binding.map_ground_types(map),
                super::guarded_requirements::GuardBinding::Null | super::guarded_requirements::GuardBinding::Call(_) => {},
            }
        }
        for ty in mapped.type_arguments.values_mut() { ty.map_ground_types(map); }
        for row in mapped.row_arguments.values_mut() {
            for ty in row.fields.values_mut() { ty.map_ground_types(map); }
        }
        mapped
    }

    pub(crate) fn effect_arguments(&self) -> &BTreeMap<SchemeEffectParameterId, EffectContract<SchemeEffectParameterId>> {
        &self.effect_arguments
    }

    pub(crate) fn application_bindings(&self) -> &[CallableApplicationBinding] {
        &self.application_bindings
    }

    pub(crate) fn operation_bindings(&self) -> &[SealedOperationBinding] { &self.operation_bindings }
    pub(crate) fn method_bindings(&self) -> &[SealedMethodBinding] { &self.method_bindings }
    pub(crate) fn wrapper_bindings(&self) -> &[super::wrapper_operation::WrapperBinding] { &self.wrapper_bindings }
    pub(crate) fn guard_bindings(&self) -> &[super::guarded_requirements::GuardBinding] { &self.guard_bindings.scopes }
    pub(crate) fn guard_call_bindings(&self) -> &[super::guarded_requirements::GuardBinding] { &self.guard_bindings.calls }

    pub(crate) fn type_arguments(&self) -> &BTreeMap<SchemeTypeParameterId, SchemeType> {
        &self.type_arguments
    }

    pub(crate) fn row_arguments(&self) -> &BTreeMap<SchemeRowParameterId, RowEvidence> {
        &self.row_arguments
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            .saturating_add(self.effect_arguments.len() * size_of::<SchemeEffectParameterId>())
            .saturating_add(self.effect_arguments.values().map(EffectContract::retained_bytes).sum::<usize>())
            .saturating_add(self.operation_bindings.capacity() * size_of::<SealedOperationBinding>())
            .saturating_add(self.method_bindings.capacity() * size_of::<SealedMethodBinding>())
            .saturating_add((self.guard_bindings.scopes.capacity() + self.guard_bindings.calls.capacity()) * size_of::<super::guarded_requirements::GuardBinding>())
            .saturating_add(self.guard_bindings.scopes.iter().chain(&self.guard_bindings.calls).map(|binding| match binding { super::guarded_requirements::GuardBinding::NonNull(binding) | super::guarded_requirements::GuardBinding::Optional(binding) => binding.retained_bytes(), super::guarded_requirements::GuardBinding::Null | super::guarded_requirements::GuardBinding::Call(_) => 0 }).sum::<usize>())
            .saturating_add(self.wrapper_bindings.capacity() * size_of::<super::wrapper_operation::WrapperBinding>())
            .saturating_add(self.application_bindings.capacity() * size_of::<CallableApplicationBinding>())
            .saturating_add(self.application_bindings.iter().map(|binding| {
                match binding {
                    CallableApplicationBinding::Selected(binding) => (binding.argument_slots.capacity() + binding.omitted_slots.capacity()) * size_of::<usize>(),
                    CallableApplicationBinding::Forwarded(_) => 0,
                }
            }).sum::<usize>())
            .saturating_add(
                self.type_arguments.len() * size_of::<SchemeTypeParameterId>(),
            )
            .saturating_add(self.type_arguments.values().map(SchemeType::retained_bytes).sum::<usize>())
            .saturating_add(
                self.row_arguments.len() * size_of::<SchemeRowParameterId>(),
            )
            .saturating_add(
                self.row_arguments
                    .values()
                    .map(RowEvidence::retained_bytes)
                    .sum::<usize>(),
            )
    }
}

impl RowEvidence {
    pub(crate) fn new(
        fields: BTreeMap<Name, SchemeType>,
        tail: SchemeRowTail,
    ) -> Result<Self, SchemeBuildError> {
        for ty in fields.values() {
            if let Some(error) = ty.row_build_error() {
                return Err(error);
            }
        }
        Ok(Self { fields, tail })
    }

    pub(crate) fn fields(&self) -> &BTreeMap<Name, SchemeType> {
        &self.fields
    }

    pub(crate) fn tail(&self) -> SchemeRowTail {
        self.tail
    }

    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            .saturating_add(self.fields.len() * size_of::<(Name, SchemeType)>())
            .saturating_add(self.fields.iter().map(|(name, ty)| {
                name.as_str().len() + ty.retained_bytes().saturating_sub(size_of::<SchemeType>())
            }).sum::<usize>())
    }
}

impl SchemeBodyFacts {
    pub(crate) fn new(
        scheme: Arc<TypeScheme>,
        expression_types: BTreeMap<ExprId, SchemeType>,
        tail_types: BTreeMap<StmtId, SchemeType>,
    ) -> Result<Self, SchemeBuildError> {
        Self::new_scoped(scheme, expression_types, tail_types, super::guarded_requirements::GuardBodyScopes::default())
    }

    pub(crate) fn new_scoped(
        scheme: Arc<TypeScheme>,
        expression_types: BTreeMap<ExprId, SchemeType>,
        tail_types: BTreeMap<StmtId, SchemeType>,
        guard_scopes: super::guarded_requirements::GuardBodyScopes,
    ) -> Result<Self, SchemeBuildError> {
        let type_ids = scheme.type_parameters().iter().copied().collect::<BTreeSet<_>>();
        let row_ids = scheme
            .row_parameters()
            .iter()
            .map(|parameter| parameter.id)
            .collect::<BTreeSet<_>>();
        let mut used_types = BTreeSet::new();
        let mut used_rows = BTreeSet::new();
        let mut row_requirements = BTreeMap::new();
        let effect_ids = scheme.callable().requirements().effect_parameters.iter().copied().collect();
        let mut used_effects = BTreeSet::new();
        let mut ownership = super::guarded_requirements::GuardOwnership::new(&type_ids, &row_ids, &effect_ids);
        ownership.collect(&scheme.callable().requirements().guards)?;
        for scope in guard_scopes.expressions.values().chain(guard_scopes.statements.values()) { ownership.check_scope(*scope)?; }
        if guard_scopes.expressions.keys().chain(guard_scopes.sites.keys()).any(|expression| !expression_types.contains_key(expression)) { return Err(SchemeBuildError::UnresolvedGuardScope); }
        for site in guard_scopes.sites.values().flatten() {
            ownership.check_scope(match site { super::guarded_requirements::GuardBodySite::Source(scope) | super::guarded_requirements::GuardBodySite::Comparison(scope) | super::guarded_requirements::GuardBodySite::Read { scope, .. } => *scope })?;
        }
        for (ty, guard) in expression_types.iter().map(|(expression, ty)| (ty, guard_scopes.expressions.get(expression).copied()))
            .chain(tail_types.iter().map(|(statement, ty)| (ty, guard_scopes.statements.get(statement).copied()))) {
            let mut usage = GuardUsage::default();
            validate_guard_type(ty, guard, &ownership, &mut usage, &mut row_requirements)?;
            used_effects.extend(usage.effects);
            used_types.extend(usage.types);
            used_rows.extend(usage.rows);
        }
        Ok(Self {
            scheme,
            expression_types,
            constructor_producers: BTreeMap::new(),
            constructor_instances: BTreeMap::new(),
            tail_types,
            registered_calls: BTreeMap::new(),
            read_bindings: BTreeSet::new(),
            proc_return_sites: BTreeMap::new(),
            operation_sites: BTreeMap::new(),
            method_sites: BTreeMap::new(),
            wrapper_sites: BTreeMap::new(), guard_scopes,
        })
    }

    pub(crate) fn guard_scopes(&self) -> &super::guarded_requirements::GuardBodyScopes { &self.guard_scopes }

    pub(crate) fn with_proc_return_sites(mut self, sites: BTreeMap<Span, SchemeProcReturnSite>) -> Result<Self, SchemeBuildError> {
        for site in sites.values() {
            let application = self.scheme.callable().requirements().applications.get(site.application as usize)
                .and_then(CallableApplication::invocation).ok_or(SchemeBuildError::UnresolvedCallableApplication)?;
            let envelope = application.proc_returns.get(site.envelope as usize)
                .ok_or(SchemeBuildError::UnresolvedCallableApplication)?;
            if self.scheme.kind() != crate::sema::types::CallableKind::Proc || envelope.result != *self.scheme.return_type() {
                return Err(SchemeBuildError::UnresolvedCallableApplication);
            }
        }
        self.proc_return_sites = sites;
        Ok(self)
    }

    pub(crate) fn map_ground_types(&self, scheme: Arc<TypeScheme>, map: &mut impl FnMut(&mut Type)) -> Self {
        let mut mapped = self.clone();
        mapped.scheme = scheme;
        for ty in mapped.expression_types.values_mut().chain(mapped.tail_types.values_mut()) { ty.map_ground_types(map); }
        for plan in mapped.registered_calls.values_mut() {
            for check in Arc::make_mut(&mut Arc::make_mut(plan).argument_checks).iter_mut().flatten() {
                if let RegisteredArgumentCheck::Unsigned(ty) = check { map(ty); }
            }
        }
        mapped
    }

    pub(crate) fn proc_return_site(&self, span: Span) -> Option<SchemeProcReturnSite> { self.proc_return_sites.get(&span).copied() }
    pub(crate) fn proc_return_site_count(&self) -> usize { self.proc_return_sites.len() }

    pub(crate) fn requirement_pack_at(&self, expression: ExprId) -> Result<&CallableRequirements, SchemeBuildError> {
        let root = self.scheme.callable().requirements();
        match self.guard_scopes.expressions.get(&expression) {
            Some(scope) => root.guard(*scope).map(|guard| guard.requirements.as_ref()).ok_or(SchemeBuildError::UnresolvedGuardScope),
            None => Ok(root),
        }
    }

    pub(crate) fn with_operation_sites(mut self, sites: BTreeMap<ExprId, u32>) -> Result<Self, SchemeBuildError> {
        if sites.iter().any(|(expression, ordinal)| !self.expression_types.contains_key(expression) || self.requirement_pack_at(*expression).map_or(true, |requirements| requirements.operations.get(*ordinal as usize).is_none())) {
            return Err(SchemeBuildError::UnresolvedSealedOperation);
        }
        self.operation_sites = sites;
        Ok(self)
    }

    pub(crate) fn operation_site(&self, expression: ExprId) -> Option<u32> { self.operation_sites.get(&expression).copied() }
    pub(crate) fn with_wrapper_sites(mut self, sites: BTreeMap<ExprId, u32>) -> Result<Self, SchemeBuildError> {
        if sites.iter().any(|(expression, ordinal)| !self.expression_types.contains_key(expression) || self.requirement_pack_at(*expression).map_or(true, |requirements| requirements.wrappers.get(*ordinal as usize).is_none())) { return Err(SchemeBuildError::UnresolvedWrapper); }
        self.wrapper_sites = sites;
        Ok(self)
    }
    pub(crate) fn wrapper_site(&self, expression: ExprId) -> Option<u32> { self.wrapper_sites.get(&expression).copied() }
    pub(crate) fn has_proc_completion_source(&self) -> bool {
        if self.scheme.kind() != crate::sema::types::CallableKind::Proc { return false; }
        let wrappers = &self.scheme.callable().requirements().wrappers;
        self.wrapper_sites.values().any(|ordinal| {
            let Some(requirement) = wrappers.get(*ordinal as usize) else { return false; };
            let (group, returned) = match &requirement.kind {
                super::wrapper_operation::WrapperRequirementKind::Field {
                    boundary: super::wrapper_operation::WrapperBoundary::ProcCompletion {
                        returned, group: super::wrapper_operation::WrapperGroupRef::Ordinal(group),
                    }, ..
                }
                | super::wrapper_operation::WrapperRequirementKind::KeyValidation {
                    boundary: super::wrapper_operation::WrapperBoundary::ProcCompletion {
                        returned, group: super::wrapper_operation::WrapperGroupRef::Ordinal(group),
                    }, ..
                } => (*group, returned),
                _ => return false,
            };
            matches!(wrappers.get(group as usize).map(|requirement| &requirement.kind), Some(
                super::wrapper_operation::WrapperRequirementKind::ProcCompletion {
                    group: super::wrapper_operation::WrapperGroupRef::Ordinal(actual_group),
                    returned: group_return,
                    contributions: super::wrapper_operation::WrapperCompletionContributions::Ready(_),
                }
            ) if *actual_group == group && group_return == returned)
        })
    }

    pub(crate) fn proc_completion_accepts_returned_result(&self, source: &SchemeType) -> bool {
        if self.scheme.kind() != crate::sema::types::CallableKind::Proc || self.scheme.explicit_return() {
            return false;
        }
        let wrappers = &self.scheme.callable().requirements().wrappers;
        self.wrapper_sites.values().any(|ordinal| {
            let Some(requirement) = wrappers.get(*ordinal as usize) else { return false; };
            let (field_return, group) = match &requirement.kind {
                super::wrapper_operation::WrapperRequirementKind::Field {
                    boundary: super::wrapper_operation::WrapperBoundary::ProcCompletion {
                        returned, group: super::wrapper_operation::WrapperGroupRef::Ordinal(group),
                    }, ..
                }
                | super::wrapper_operation::WrapperRequirementKind::KeyValidation {
                    boundary: super::wrapper_operation::WrapperBoundary::ProcCompletion {
                        returned, group: super::wrapper_operation::WrapperGroupRef::Ordinal(group),
                    }, ..
                } => (returned, group),
                _ => return false,
            };
            let Some(group_requirement) = wrappers.get(*group as usize) else { return false; };
            let super::wrapper_operation::WrapperRequirementKind::ProcCompletion {
                group: super::wrapper_operation::WrapperGroupRef::Ordinal(actual_group),
                returned,
                contributions: super::wrapper_operation::WrapperCompletionContributions::Ready(contributions),
            } = &group_requirement.kind else { return false; };
            *actual_group == *group
                && returned == self.scheme.return_type()
                && field_return == returned
                && contributions.iter().any(|contribution| {
                    matches!(contribution, super::wrapper_operation::WrapperCompletionContribution::ReturnedResult(actual) if actual == source)
                })
        })
    }


    pub(crate) fn with_method_sites(mut self, sites: BTreeMap<ExprId, u32>) -> Result<Self, SchemeBuildError> {
        if sites.iter().any(|(expression, ordinal)| !self.expression_types.contains_key(expression) || self.requirement_pack_at(*expression).map_or(true, |requirements| requirements.methods.get(*ordinal as usize).is_none())) {
            return Err(SchemeBuildError::UnresolvedMethodFamily);
        }
        self.method_sites = sites;
        Ok(self)
    }

    pub(crate) fn method_site(&self, expression: ExprId) -> Option<u32> { self.method_sites.get(&expression).copied() }

    pub(crate) fn with_constructor_producers(mut self, constructors: BTreeMap<ExprId, NonmaterialConstructor>) -> Self {
        self.constructor_producers = constructors;
        self
    }

    pub(crate) fn with_constructor_instances(mut self, constructors: BTreeMap<ExprId, super::constants::CheckedRecordConstructor<SchemeType>>) -> Result<Self, SchemeBuildError> {
        let type_ids = self.scheme.type_parameters().iter().copied().collect();
        let row_ids = self.scheme.row_parameters().iter().map(|row| row.id).collect();
        let effect_ids = self.scheme.callable().requirements().effect_parameters.iter().copied().collect();
        for (&expression, constructor) in &constructors {
            if self.expression_types.get(&expression) != Some(&constructor.ty) { return Err(SchemeBuildError::UnresolvedSchema); }
            let mut used_effects = BTreeSet::new();
            let mut used_types = BTreeSet::new();
            let mut used_rows = BTreeSet::new();
            let mut row_requirements = BTreeMap::new();
            for ty in constructor.instance.arguments.iter().chain(std::iter::once(&constructor.ty)) {
                ty.validate(&type_ids, &row_ids, &effect_ids, &mut used_effects, &mut used_types, &mut used_rows, &mut row_requirements)?;
            }
        }
        self.constructor_instances = constructors;
        Ok(self)
    }

    pub(crate) fn constructor_instances(&self) -> &BTreeMap<ExprId, super::constants::CheckedRecordConstructor<SchemeType>> {
        &self.constructor_instances
    }

    pub(crate) fn constructor_producers(&self) -> &BTreeMap<ExprId, NonmaterialConstructor> {
        &self.constructor_producers
    }

    pub(crate) fn with_registered_calls(mut self, calls: BTreeMap<ExprId, Arc<RegisteredCallPlan>>) -> Self {
        self.registered_calls = calls;
        self
    }

    pub(crate) fn with_read_bindings(mut self, bindings: BTreeSet<super::check::BindingId>) -> Self {
        self.read_bindings = bindings;
        self
    }

    pub(crate) fn read_bindings(&self) -> &BTreeSet<super::check::BindingId> {
        &self.read_bindings
    }

    pub(crate) fn registered_calls(&self) -> &BTreeMap<ExprId, Arc<RegisteredCallPlan>> {
        &self.registered_calls
    }

    pub(crate) fn scheme(&self) -> &Arc<TypeScheme> {
        &self.scheme
    }

    pub(crate) fn expression_types(&self) -> &BTreeMap<ExprId, SchemeType> {
        &self.expression_types
    }

    pub(crate) fn tail_types(&self) -> &BTreeMap<StmtId, SchemeType> {
        &self.tail_types
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            .saturating_add(self.scheme.retained_bytes())
            .saturating_add(self.expression_types.len() * size_of::<(ExprId, SchemeType)>())
            .saturating_add(
                self.expression_types
                    .values()
                    .map(SchemeType::retained_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(self.constructor_producers.len() * size_of::<(ExprId, NonmaterialConstructor)>())
            .saturating_add(self.constructor_instances.len() * size_of::<(ExprId, super::constants::CheckedRecordConstructor<SchemeType>)>())
            .saturating_add(self.constructor_instances.values().map(|constructor| constructor.ty.retained_bytes()
                .saturating_add(constructor.instance.arguments.capacity() * size_of::<SchemeType>())
                .saturating_add(constructor.instance.arguments.iter().map(SchemeType::retained_bytes).sum::<usize>())).sum::<usize>())
            .saturating_add(self.read_bindings.len() * size_of::<super::check::BindingId>())
            .saturating_add(self.proc_return_site_count() * size_of::<(Span, SchemeProcReturnSite)>())
            .saturating_add(self.operation_sites.len() * size_of::<(ExprId, u32)>())
            .saturating_add(self.method_sites.len() * size_of::<(ExprId, u32)>())
            .saturating_add(self.wrapper_sites.len() * size_of::<(ExprId, u32)>())
            .saturating_add(self.guard_scopes.expressions.len() * size_of::<(ExprId, super::guarded_requirements::GuardScopeId)>())
            .saturating_add(self.guard_scopes.statements.len() * size_of::<(StmtId, super::guarded_requirements::GuardScopeId)>())
            .saturating_add(self.guard_scopes.sites.len() * size_of::<(ExprId, Vec<super::guarded_requirements::GuardBodySite>)>())
            .saturating_add(self.guard_scopes.sites.values().map(|sites| sites.capacity() * size_of::<super::guarded_requirements::GuardBodySite>()).sum::<usize>())
            .saturating_add(self.registered_calls.len() * size_of::<(ExprId, Arc<RegisteredCallPlan>)>())
            .saturating_add(self.registered_calls.values().map(|plan| plan.retained_bytes()).sum::<usize>())
            .saturating_add(self.tail_types.len() * size_of::<(StmtId, SchemeType)>())
            .saturating_add(
                self.tail_types
                    .values()
                    .map(SchemeType::retained_bytes)
                    .sum::<usize>(),
            )
    }
}

impl SchemaExpectation<SchemeType> {
    pub(crate) fn closed_ground(&self) -> Option<SchemaExpectation> {
        self.try_map(&mut |ty| ty.closed_ground_type().ok_or(())).ok()
    }

    pub(crate) fn map_ground_types(&mut self, map: &mut impl FnMut(&mut Type)) {
        let mut pending = vec![self];
        while let Some(schema) = pending.pop() {
            for instance in &mut schema.instances {
                for argument in &mut instance.arguments { argument.map_ground_types(map); }
            }
            pending.extend(Arc::make_mut(&mut schema.children).values_mut());
        }
    }
}

pub(crate) fn schema_expectation_retained_bytes<T>(
    expectation: &SchemaExpectation<T>,
    counted_children: &mut BTreeSet<usize>,
    argument_bytes: fn(&T) -> usize,
) -> usize {
    use std::mem::size_of;

    let mut bytes = expectation.instances.capacity() * size_of::<SchemaInstance<T>>();
    for instance in &expectation.instances {
        bytes = bytes.saturating_add(instance.arguments.capacity() * size_of::<T>());
        for argument in &instance.arguments {
            bytes = bytes.saturating_add(
                argument_bytes(argument)
                    .saturating_sub(size_of::<T>()),
            );
        }
    }
    if counted_children.insert(Arc::as_ptr(&expectation.children) as usize) {
        bytes = bytes.saturating_add(size_of::<BTreeMap<SchemaComponent, SchemaExpectation<T>>>());
        bytes = bytes.saturating_add(
            expectation.children.len() * size_of::<(SchemaComponent, SchemaExpectation<T>)>(),
        );
        for (component, child) in expectation.children.iter() {
            if let SchemaComponent::Field(name) = component {
                bytes = bytes.saturating_add(name.as_str().len());
            }
            bytes = bytes.saturating_add(schema_expectation_retained_bytes(
                child,
                counted_children,
                argument_bytes,
            ));
        }
    }
    bytes
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SchemeValueRole { Body, RecordRow }

impl SchemeType {
    pub(crate) fn satisfies_conversion(&self, conversion: ValueConversion, parameters: &impl Fn(SchemeTypeParameterId) -> Option<ValueConversionSet>) -> bool {
        match self {
            Self::Parameter(parameter) => parameters(*parameter).is_some_and(|required| required.contains(conversion)),
            Self::Validated(_, inner) => inner.satisfies_conversion(conversion, parameters),
            Self::List(item) if matches!(conversion, ValueConversion::ArgvList | ValueConversion::ArgvOrList) => item.satisfies_conversion(ValueConversion::Argv, parameters),
            Self::Union(members) if matches!(conversion, ValueConversion::Argv | ValueConversion::ArgvOrList | ValueConversion::ProcessWord) => members.iter().all(|member| member.satisfies_conversion(if conversion == ValueConversion::ProcessWord { conversion } else { ValueConversion::Argv }, parameters)),
            actual => actual.closed_ground_type().is_some_and(|actual| conversion.accepts(&actual)),
        }
    }

    pub(crate) fn satisfies_conversions(&self, conversions: ValueConversionSet, parameters: &BTreeMap<SchemeTypeParameterId, ValueConversionSet>) -> bool {
        ValueConversion::ALL.into_iter().all(|conversion| !conversions.contains(conversion) || self.satisfies_conversion(conversion, &|parameter| parameters.get(&parameter).copied()))
    }

    pub(crate) fn is_map_key_with(&self, parameters: &[SchemeTypeParameterId]) -> bool {
        match self {
            Self::Parameter(parameter) => parameters.contains(parameter),
            actual => actual.closed_ground_type().is_some_and(|actual| actual.is_map_key()),
        }
    }

    pub(crate) fn closed_ground_type(&self) -> Option<Type> {
        fn project(ty: &SchemeType, depth: usize) -> Option<Type> {
            if depth > crate::sema::constraints::MAX_TYPE_DEPTH {
                return None;
            }
            let nested = |ty: &SchemeType| project(ty, depth + 1);
            match ty {
                SchemeType::Ground(ty)
                    if !ty.contains_inference()
                        && !ty.contains_recovery()
                        && !SchemeType::contains_builtin_parameter(ty) =>
                {
                    Some(ty.clone())
                }
                SchemeType::Ground(_) | SchemeType::Parameter(_) => None,
                SchemeType::SchemeCallable(id) => Some(Type::SchemeCallable(*id)),
                SchemeType::Callable { kind, monotype, effects } => {
                    if monotype.has_schema_contract() {
                        return None;
                    }
                    Some(Type::Callable(Arc::new(super::types::TypedCallable {
                        kind: *kind,
                        sig: super::types::CallableType {
                            params: monotype.params().iter().map(|parameter| Some(super::types::CallableParamType {
                                name: parameter.name,
                                ty: nested(&parameter.ty)?,
                                defaulted: parameter.defaulted,
                                rest: parameter.rest,
                            })).collect::<Option<Vec<_>>>()?,
                            return_ty: Box::new(nested(monotype.return_type())?),
                            effects: super::types::CallableEffects::from_clause(effects.concrete().ok()?),
                        },
                    })))
                }
                SchemeType::List(inner) => Some(Type::List(Box::new(nested(inner)?))),
                SchemeType::Map(key, value) => Some(Type::Map(
                    Box::new(nested(key)?),
                    Box::new(nested(value)?),
                )),
                SchemeType::Stream(inner) => Some(Type::Stream(Box::new(nested(inner)?))),
                SchemeType::Set(inner) => Some(Type::Set(Box::new(nested(inner)?))),
                SchemeType::Result(ok, error) => Some(Type::Result(
                    Box::new(nested(ok)?),
                    Box::new(nested(error)?),
                )),
                SchemeType::Optional(inner) => Some(Type::Optional(Box::new(nested(inner)?))),
                SchemeType::Record { fields, tail: SchemeRowTail::Closed } => {
                    let fields = fields
                        .iter()
                        .map(|(&name, ty)| Some((name, nested(ty)?)))
                        .collect::<Option<BTreeMap<_, _>>>()?;
                    Some(Type::Record(Arc::new(fields)))
                }
                SchemeType::Record { tail: SchemeRowTail::Parameter(_), .. } => None,
                SchemeType::Union(members) => Some(Type::Union(
                    members.iter().map(nested).collect::<Option<Vec<_>>>()?,
                )),
                SchemeType::Validated(validation, base) => {
                    let base = nested(base)?;
                    crate::sema::validated::ValidatedType::new(*validation, base)
                        .ok()
                        .map(|validated| Type::Validated(Box::new(validated)))
                }
            }
        }

        project(self, 0)
    }

    fn contains_builtin_parameter(ty: &Type) -> bool {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match ty {
                Type::BuiltinParameter(_) => return true,
                Type::List(inner)
                | Type::Stream(inner)
                | Type::Set(inner)
                | Type::Optional(inner) => pending.push(inner),
                Type::Map(key, value) | Type::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                Type::Module(module) => {
                    for export in module.exports.values() {
                        match export {
                            ModuleExportType::Value { ty, .. } => pending.push(ty),
                            ModuleExportType::Callable { sig, .. } => {
                                pending.push(&sig.return_ty);
                                pending.extend(sig.params.iter().map(|parameter| &parameter.ty));
                            }
                            ModuleExportType::GenericCallable { .. } => {}
                        }
                    }
                }
                Type::Record(fields) => pending.extend(fields.values()),
                Type::Union(members) => pending.extend(members),
                Type::Validated(validated) => pending.push(validated.base()),
                Type::Callable(callable) => {
                    pending.push(&callable.sig.return_ty);
                    pending.extend(callable.sig.params.iter().map(|parameter| &parameter.ty));
                }
                _ => {}
            }
        }
        false
    }

    pub(crate) fn contains_parameter(&self) -> bool {
        match self {
            Self::Parameter(_) => true,
            Self::Callable { monotype, effects, .. } => matches!(effects, EffectContract::Known(expression) if expression.closed().is_none())
                || monotype.params().iter().any(|parameter| parameter.ty.contains_parameter())
                || monotype.return_type().contains_parameter(),
            Self::List(inner)
            | Self::Stream(inner)
            | Self::Set(inner)
            | Self::Optional(inner)
            | Self::Validated(_, inner) => inner.contains_parameter(),
            Self::Map(key, value) | Self::Result(key, value) => {
                key.contains_parameter() || value.contains_parameter()
            }
            Self::Record { fields, tail } => {
                matches!(tail, SchemeRowTail::Parameter(_))
                    || fields.values().any(Self::contains_parameter)
            }
            Self::Union(members) => members.iter().any(Self::contains_parameter),
            Self::Ground(_) | Self::SchemeCallable(_) => false,
        }
    }

    pub(crate) fn matches_body_return(&self, expected: &Self) -> bool {
        if self == expected { return true; }
        if let Self::Optional(inner) = expected {
            if self == &Self::Ground(Type::Null) { return true; }
            if !matches!(self, Self::Optional(_)) { return self.matches_body_return(inner); }
        }
        if let (Self::Result(actual_ok, actual_error), Self::Result(expected_ok, expected_error)) = (self, expected) {
            return actual_ok.matches_body_return(expected_ok) && actual_error.matches_body_return(expected_error);
        }
        match (self.closed_ground_type(), expected.closed_ground_type()) {
            (Some(actual), Some(expected)) => super::types::tail_type_matches_expected(&expected, &actual),
            _ => false,
        }
    }

    pub(crate) fn may_hold_resource(&self) -> bool {
        let mut pending = vec![self];
        while let Some(ty) = pending.pop() {
            match ty {
                Self::Ground(ty) if ty.may_hold_resource() => return true,
                Self::Ground(_) => {}
                Self::Parameter(_) => return true,
                Self::Callable { .. } | Self::SchemeCallable(_) => return true,
                Self::List(inner)
                | Self::Set(inner)
                | Self::Optional(inner)
                | Self::Validated(_, inner) => pending.push(inner),
                Self::Stream(_) => return true,
                Self::Map(key, value) | Self::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                Self::Record { .. } => return true,
                Self::Union(members) => pending.extend(members),
            }
        }
        false
    }

    pub(crate) fn map_ground_types(&mut self, map: &mut impl FnMut(&mut Type)) {
        match self {
            Self::Ground(ty) => map(ty),
            Self::SchemeCallable(id) => {
                let mut ty = Type::SchemeCallable(*id);
                map(&mut ty);
                *self = match ty { Type::SchemeCallable(id) => Self::SchemeCallable(id), ty => Self::Ground(ty) };
            }
            Self::Callable { monotype, .. } => *monotype = Arc::new(monotype.map_ground_types(map)),
            Self::List(inner)
            | Self::Stream(inner)
            | Self::Set(inner)
            | Self::Optional(inner)
            | Self::Validated(_, inner) => inner.map_ground_types(map),
            Self::Map(key, value) => {
                key.map_ground_types(map);
                value.map_ground_types(map);
            }
            Self::Result(ok, err) => {
                ok.map_ground_types(map);
                err.map_ground_types(map);
            }
            Self::Record { fields, .. } => {
                for field in fields.values_mut() {
                    field.map_ground_types(map);
                }
            }
            Self::Union(members) => {
                for member in members {
                    member.map_ground_types(map);
                }
            }
            Self::Parameter(_) => {}
        }
    }

    fn body_build_error(&self) -> Option<SchemeBuildError> { self.build_error(SchemeValueRole::Body) }
    fn row_build_error(&self) -> Option<SchemeBuildError> { self.build_error(SchemeValueRole::RecordRow) }

    fn build_error(&self, role: SchemeValueRole) -> Option<SchemeBuildError> {
        match self {
            Self::Ground(ty) if role == SchemeValueRole::Body && ty.contains_any() => Some(SchemeBuildError::DynamicType),
            Self::Ground(ty) if ty.contains_recovery() => Some(SchemeBuildError::RecoveryType),
            Self::Ground(ty) if ty.contains_inference() => Some(SchemeBuildError::UnresolvedType),
            Self::Ground(_) | Self::Parameter(_) | Self::SchemeCallable(_) => None,
            Self::Callable { monotype, .. } => monotype.params().iter()
                .find_map(|parameter| parameter.ty.build_error(role))
                .or_else(|| monotype.return_type().build_error(role)),
            Self::List(inner)
            | Self::Stream(inner)
            | Self::Set(inner)
            | Self::Optional(inner)
            | Self::Validated(_, inner) => inner.build_error(role),
            Self::Map(key, value) | Self::Result(key, value) => {
                key.build_error(role).or_else(|| value.build_error(role))
            }
            Self::Record { fields, .. } => {
                fields.values().find_map(|ty| ty.build_error(role))
            }
            Self::Union(members) => members.iter().find_map(|ty| ty.build_error(role)),
        }
    }

    fn validate(
        &self,
        type_ids: &BTreeSet<SchemeTypeParameterId>,
        row_ids: &BTreeSet<SchemeRowParameterId>,
        effect_ids: &BTreeSet<SchemeEffectParameterId>,
        used_effects: &mut BTreeSet<SchemeEffectParameterId>,
        used_types: &mut BTreeSet<SchemeTypeParameterId>,
        used_rows: &mut BTreeSet<SchemeRowParameterId>,
        row_requirements: &mut BTreeMap<SchemeRowParameterId, BTreeSet<Name>>,
    ) -> Result<(), SchemeBuildError> {
        match self {
            Self::Ground(ty) => validate_ground(ty),
            Self::SchemeCallable(_) => Ok(()),
            Self::Callable { monotype, effects, .. } => {
                validate_effect_contract(effects, effect_ids, used_effects)?;
                for parameter in monotype.params() {
                    if let Some(schema) = &parameter.schema_expectation { validate_schema_types(schema, &mut |ty| ty.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements))?; }
                    parameter.ty.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements)?;
                }
                if let Some(schema) = monotype.return_schema() { validate_schema_types(schema, &mut |ty| ty.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements))?; }
                monotype.return_type().validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements)
            }
            Self::Parameter(id) => {
                if type_ids.contains(id) {
                    used_types.insert(*id);
                    Ok(())
                } else {
                    Err(SchemeBuildError::UnboundTypeParameter)
                }
            }
            Self::List(ty) | Self::Stream(ty) | Self::Set(ty) | Self::Optional(ty) => {
                ty.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements)
            }
            Self::Map(key, value) | Self::Result(key, value) => {
                key.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements)?;
                value.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements)
            }
            Self::Record { fields, tail } => {
                for ty in fields.values() {
                    ty.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements)?;
                }
                if let SchemeRowTail::Parameter(id) = tail {
                    if row_ids.contains(id) {
                        used_rows.insert(*id);
                        row_requirements
                            .entry(*id)
                            .or_default()
                            .extend(fields.keys().copied());
                        Ok(())
                    } else {
                        Err(SchemeBuildError::UnboundRowParameter)
                    }
                } else {
                    Ok(())
                }
            }
            Self::Union(types) => {
                for ty in types {
                    ty.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements)?;
                }
                Ok(())
            }
            Self::Validated(validation, ty) => {
                let valid_base = match validation {
                    Validation::NonEmpty => matches!(ty.as_ref(), Self::List(_)),
                    Validation::RelPath => matches!(ty.as_ref(), Self::Ground(Type::Path)),
                    Validation::Nominal(_) => matches!(ty.as_ref(), Self::Record { .. }),
                    Validation::Range(range) => match ty.as_ref() {
                        Self::Ground(Type::Int) => true,
                        Self::Ground(Type::UInt) => range.low() >= 0,
                        _ => false,
                    },
                };
                if !valid_base {
                    return Err(SchemeBuildError::InvalidValidatedBase);
                }
                ty.validate(type_ids, row_ids, effect_ids, used_effects, used_types, used_rows, row_requirements)
            }
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>();
        match self {
            Self::Ground(ty) => {
                bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<Type>()));
            }
            Self::Parameter(_) | Self::SchemeCallable(_) => {}
            Self::Callable { monotype, effects, .. } => {
                bytes = bytes.saturating_add(monotype.retained_bytes())
                    .saturating_add(effects.retained_bytes());
            }
            Self::List(ty)
            | Self::Stream(ty)
            | Self::Set(ty)
            | Self::Optional(ty)
            | Self::Validated(_, ty) => bytes = bytes.saturating_add(ty.retained_bytes()),
            Self::Map(key, value) | Self::Result(key, value) => {
                bytes = bytes
                    .saturating_add(key.retained_bytes())
                    .saturating_add(value.retained_bytes());
            }
            Self::Record { fields, .. } => {
                bytes = bytes.saturating_add(fields.len() * size_of::<(Name, Self)>());
                for (name, ty) in fields {
                    bytes = bytes
                        .saturating_add(name.as_str().len())
                        .saturating_add(ty.retained_bytes().saturating_sub(size_of::<Self>()));
                }
            }
            Self::Union(types) => {
                bytes = bytes.saturating_add(types.capacity() * size_of::<Self>());
                for ty in types {
                    bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<Self>()));
                }
            }
        }
        bytes
    }
}

fn validate_ground(ty: &Type) -> Result<(), SchemeBuildError> {
    if ty.contains_inference() {
        Err(SchemeBuildError::UnresolvedType)
    } else if ty.contains_recovery() {
        Err(SchemeBuildError::RecoveryType)
    } else {
        Ok(())
    }
}

fn validate_schema_types(schema: &SchemaExpectation<SchemeType>, validate: &mut impl FnMut(&SchemeType) -> Result<(), SchemeBuildError>) -> Result<(), SchemeBuildError> {
    let mut pending = vec![schema];
    while let Some(schema) = pending.pop() {
        for instance in &schema.instances {
            for argument in &instance.arguments { validate(argument)?; }
        }
        pending.extend(schema.children.values());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{EffectContract, SchemeBuildError, SchemeType, TypeScheme};
    use crate::sema::constants::{SchemaComponent, SchemaExpectation, SchemaInstance};
    use crate::sema::schemes::{
        CallInstantiation, CallableScheme, RowEvidence, SchemeParameter,
        SchemeRowParameter, SchemeRowParameterId, SchemeRowTail,
    };
    use crate::sema::types::{ModuleExportType, ModuleType, Type};
    use crate::symbol::Name;
    use crate::syntax::arena::TypeDefId;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    #[test]
    fn symbolic_result_returns_preserve_binders_error_domains_and_data() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let value = SchemeType::Parameter(super::SchemeTypeParameterId(0));
            let other = SchemeType::Parameter(super::SchemeTypeParameterId(1));
            let family = Name::intern("Failure");
            let foreign = Name::intern("Foreign");
            let variant = SchemeType::Ground(Type::ErrorVariant { family, variant: Name::intern("Rejected") });
            let error = SchemeType::Ground(Type::Error);
            let family_error = SchemeType::Ground(Type::ErrorFamily(family));
            let foreign_error = SchemeType::Ground(Type::ErrorFamily(foreign));
            let result = |ok, error| SchemeType::Result(Box::new(ok), Box::new(error));
            let actual = result(value.clone(), variant.clone());
            let expected = result(value.clone(), error.clone());
            assert!(actual.matches_body_return(&expected));
            assert!(actual.matches_body_return(&result(value.clone(), family_error)));
            assert!(!actual.matches_body_return(&result(other, error.clone())));
            assert!(!actual.matches_body_return(&result(value.clone(), foreign_error)));
            assert!(!result(value.clone(), SchemeType::Ground(Type::Str)).matches_body_return(&expected));
            assert!(!actual.matches_body_return(&value));
            assert!(!value.matches_body_return(&expected));
            assert!(!variant.matches_body_return(&expected));
            assert!(!result(actual.clone(), error).matches_body_return(&expected));
            assert!(!SchemeType::List(Box::new(actual)).matches_body_return(&SchemeType::List(Box::new(expected))));
        });
    }


    #[test]
    fn callable_requirement_scope_keeps_discarded_application_results_bound() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            use super::{CallableRequirements, SchemeEffectParameterId, SchemeTypeParameterId};
            use crate::sema::callable_application::{CallableApplication, CallableApplicationArgument, CallableKindRequirement};
            use crate::sema::arguments::StaticArgumentKind;
            use crate::sema::effects::{EffectContract, EffectPredicate, EffectSet};
            let f = SchemeTypeParameterId(0);
            let value = SchemeTypeParameterId(1);
            let discarded = SchemeTypeParameterId(2);
            let effect = SchemeEffectParameterId(0);
            let span = crate::source::Span::new(crate::source::SourceId::new(0), 0, 1);
            let requirements = CallableRequirements {
                map_key_parameters: Vec::new(),
                guards: Vec::new(), guard_calls: Vec::new(),
                conversion_parameters: BTreeMap::new(),
                effect_parameters: vec![effect],
                iteration_parameters: BTreeMap::new(), operations: Vec::new(), methods: Vec::new(), wrappers: Vec::new(),
                effect_predicates: vec![EffectPredicate::Covered(EffectContract::variable(effect), EffectContract::finite(EffectSet::all()))],
                applications: vec![CallableApplication::Invocation(crate::sema::callable_application::CallableInvocation {
                    callee: SchemeType::Parameter(f),
                    arguments: vec![CallableApplicationArgument { kind: StaticArgumentKind::Positional, ty: SchemeType::Parameter(value), span }].into(),
                    proc_returns: Arc::from([]),
                    result: SchemeType::Parameter(discarded), effects: EffectContract::variable(effect),
                    kind: CallableKindRequirement::Effectful, span,
                })],
            };
            let make = |requirements| CallableScheme::new(vec![f, value, discarded], Vec::new(),
                vec![SchemeParameter { name: Name::intern("action"), ty: SchemeType::Parameter(f), schema_expectation: None, defaulted: false, rest: false },
                    SchemeParameter { name: Name::intern("value"), ty: SchemeType::Parameter(value), schema_expectation: None, defaulted: false, rest: false }],
                SchemeType::Parameter(f), None, false, requirements);
            let callable = make(requirements.clone()).unwrap();
            assert_eq!(callable.requirements(), &requirements);
            assert_eq!(callable.params()[0].ty, *callable.return_type());
            let mut missing = requirements.clone();
            missing.applications.clear();
            assert_eq!(make(missing), Err(SchemeBuildError::UnusedTypeParameter));
            let mut foreign = requirements.clone();
            foreign.applications[0].invocation_mut().unwrap().result = SchemeType::Parameter(SchemeTypeParameterId(9));
            assert_eq!(make(foreign), Err(SchemeBuildError::UnboundTypeParameter));
            let mut foreign = requirements.clone();
            foreign.applications[0].invocation_mut().unwrap().effects = EffectContract::variable(SchemeEffectParameterId(9));
            assert_eq!(make(foreign), Err(SchemeBuildError::UnboundEffectParameter));
            let mut duplicate = requirements.clone();
            duplicate.effect_parameters.push(effect);
            assert_eq!(make(duplicate), Err(SchemeBuildError::DuplicateEffectParameter));
            let mut unused = requirements;
            unused.effect_parameters.push(SchemeEffectParameterId(1));
            assert_eq!(make(unused), Err(SchemeBuildError::UnusedEffectParameter));
        });
    }

    #[test]
    fn callable_requirement_scope_validates_rows_in_application_arguments() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            use super::{CallableRequirements, SchemeTypeParameterId};
            use crate::sema::callable_application::{CallableApplication, CallableApplicationArgument, CallableKindRequirement};
            use crate::sema::arguments::StaticArgumentKind;
            use crate::sema::effects::{EffectContract, EffectSet};
            let f = SchemeTypeParameterId(0);
            let row = SchemeRowParameterId(0);
            let field = Name::intern("value");
            let span = crate::source::Span::new(crate::source::SourceId::new(0), 0, 1);
            let requirements = CallableRequirements {
                map_key_parameters: Vec::new(),
                applications: vec![CallableApplication::Invocation(crate::sema::callable_application::CallableInvocation {
                    callee: SchemeType::Parameter(f),
                    arguments: vec![CallableApplicationArgument { kind: StaticArgumentKind::Named(Name::intern("payload")),
                        ty: SchemeType::Record { fields: BTreeMap::from([(field, SchemeType::Ground(Type::Int))]), tail: SchemeRowTail::Parameter(row) }, span }].into(),
                    proc_returns: Arc::from([]),
                    result: SchemeType::Ground(Type::Unit), effects: EffectContract::finite(EffectSet::default()), kind: CallableKindRequirement::Pure, span,
                })],
                ..CallableRequirements::default()
            };
            let make = |lacks| CallableScheme::new(vec![f], vec![SchemeRowParameter { id: row, lacks }],
                vec![SchemeParameter { name: Name::intern("action"), ty: SchemeType::Parameter(f), schema_expectation: None, defaulted: false, rest: false }],
                SchemeType::Parameter(f), None, false, requirements.clone());
            assert_eq!(make(BTreeSet::new()), Err(SchemeBuildError::RowLacksMismatch));
            let callable = make(BTreeSet::from([field])).unwrap();
            assert!(callable.retained_bytes() > std::mem::size_of_val(&callable));
            assert_eq!(callable.requirements().applications[0].arguments()[0].kind, StaticArgumentKind::Named(Name::intern("payload")));
        });
    }
    #[test]
    fn scheme_resource_potential_preserves_hidden_record_payloads() {
        let record = SchemeType::Record { fields: BTreeMap::new(), tail: SchemeRowTail::Closed };
        assert!(record.may_hold_resource());
        assert!(SchemeType::List(Box::new(record)).may_hold_resource());
        assert!(!SchemeType::List(Box::new(SchemeType::Ground(Type::Int))).may_hold_resource());
    }

    #[test]
    fn generic_proc_schemes_reject_unresolved_effects() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let binder = super::SchemeTypeParameterId(0);
            let callable = Arc::new(super::CallableScheme::new(vec![binder], Vec::new(),
                vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Parameter(binder),
                    schema_expectation: None, defaulted: false, rest: false }],
                SchemeType::Result(Box::new(SchemeType::Parameter(binder)), Box::new(SchemeType::Ground(Type::Error))),
                None, false, crate::sema::schemes::CallableRequirements::default()).unwrap());
            assert_eq!(TypeScheme::with_callable(crate::sema::types::CallableKind::Proc, Arc::clone(&callable), super::CheckedEffects::unknown(), true), Err(SchemeBuildError::UnresolvedEffects));
            assert!(TypeScheme::with_callable(crate::sema::types::CallableKind::Proc, callable, super::CheckedEffects::finite([]), true).is_ok());
        });
    }

    #[test]
    fn nested_callable_ground_projection_preserves_schema_contracts() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let callback = |parameter_schema, return_schema| SchemeType::Callable {
                kind: crate::sema::types::CallableKind::Pure,
                monotype: Arc::new(super::CallableMonotype::new(
                    vec![SchemeParameter {
                        name: Name::intern("value"),
                        ty: SchemeType::Ground(Type::Int),
                        schema_expectation: parameter_schema,
                        defaulted: false,
                        rest: false,
                    }],
                    SchemeType::Ground(Type::Int),
                    return_schema,
                )),
                effects: EffectContract::finite(super::super::effects::EffectSet::default()),
            };
            assert!(matches!(callback(None, None).closed_ground_type(), Some(Type::Callable(_))));
            let schema = SchemaExpectation {
                instances: vec![SchemaInstance { definition: TypeDefId::from_index(0), arguments: vec![SchemeType::Ground(Type::Int)] }],
                ..SchemaExpectation::default()
            };
            assert_eq!(callback(Some(schema.clone()), None).closed_ground_type(), None);
            assert_eq!(callback(None, Some(schema)).closed_ground_type(), None);
        });
    }

    #[test]
    fn scoped_schema_arguments_use_the_callable_binders_without_runtime_reification() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let binder = super::SchemeTypeParameterId(0);
            let parameter = SchemeParameter { name: Name::intern("value"), ty: SchemeType::Parameter(binder), schema_expectation: None, defaulted: false, rest: false };
            let schema = |argument| SchemaExpectation {
                instances: vec![SchemaInstance { definition: TypeDefId::from_index(0), arguments: vec![argument] }],
                ..SchemaExpectation::default()
            };
            let result = SchemeType::Record { fields: BTreeMap::from([(Name::intern("value"), SchemeType::Parameter(binder))]), tail: SchemeRowTail::Closed };
            let callable = CallableScheme::new(vec![binder], Vec::new(), vec![parameter.clone()], result.clone(),
                Some(schema(SchemeType::Parameter(binder))), false, super::CallableRequirements::default()).unwrap();
            assert!(callable.return_schema().unwrap().closed_ground().is_none());
            let foreign = CallableScheme::new(vec![binder], Vec::new(), vec![parameter], result,
                Some(schema(SchemeType::Parameter(super::SchemeTypeParameterId(1)))), false, super::CallableRequirements::default());
            assert_eq!(foreign, Err(SchemeBuildError::UnboundTypeParameter));
        });
    }

    #[test]
    fn nested_callable_schema_rejects_solver_references() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = crate::sema::constraints::TypeConstraints::default();
            let variable = constraints.fresh(crate::source::Span::at(crate::source::SourceId::new(0), 0));
            let schema = SchemaExpectation {
                instances: vec![SchemaInstance { definition: TypeDefId::from_index(0), arguments: vec![SchemeType::Ground(variable)] }],
                ..SchemaExpectation::default()
            };
            for schema_in_parameter in [false, true] {
                let callback = SchemeType::Callable {
                    kind: crate::sema::types::CallableKind::Pure,
                    monotype: Arc::new(super::CallableMonotype::new(
                        vec![SchemeParameter {
                            name: Name::intern("value"), ty: SchemeType::Ground(Type::Int),
                            schema_expectation: schema_in_parameter.then(|| schema.clone()), defaulted: false, rest: false,
                        }],
                        SchemeType::Ground(Type::Int),
                        (!schema_in_parameter).then(|| schema.clone()),
                    )),
                    effects: EffectContract::finite(super::super::effects::EffectSet::default()),
                };
                let result = TypeScheme::new(crate::sema::types::CallableKind::Pure, Vec::new(), Vec::new(),
                    vec![SchemeParameter { name: Name::intern("action"), ty: callback, schema_expectation: None, defaulted: false, rest: false }],
                    SchemeType::Ground(Type::Int), None, false, None, false);
                assert_eq!(result, Err(SchemeBuildError::UnresolvedType));
            }
        });
    }

    #[test]
    fn closed_scheme_types_project_every_ground_shape() {
        use crate::sema::validated::{IntRange, Validation};

        crate::symbol::SymbolOwner::new().with_current(|| {
            let int = || SchemeType::Ground(Type::Int);
            let path = || SchemeType::Ground(Type::Path);
            let record = || SchemeType::Record {
                fields: BTreeMap::from([(Name::intern("path"), path())]),
                tail: SchemeRowTail::Closed,
            };
            let cases = [
                (
                    SchemeType::List(Box::new(path())),
                    Type::List(Box::new(Type::Path)),
                ),
                (
                    SchemeType::Map(Box::new(int()), Box::new(path())),
                    Type::Map(Box::new(Type::Int), Box::new(Type::Path)),
                ),
                (
                    SchemeType::Stream(Box::new(int())),
                    Type::Stream(Box::new(Type::Int)),
                ),
                (SchemeType::Set(Box::new(path())), Type::Set(Box::new(Type::Path))),
                (
                    SchemeType::Result(Box::new(int()), Box::new(path())),
                    Type::Result(Box::new(Type::Int), Box::new(Type::Path)),
                ),
                (
                    SchemeType::Optional(Box::new(path())),
                    Type::Optional(Box::new(Type::Path)),
                ),
                (
                    SchemeType::Record {
                        fields: BTreeMap::from([(Name::intern("path"), path())]),
                        tail: SchemeRowTail::Closed,
                    },
                    Type::Record(Arc::new(BTreeMap::from([(Name::intern("path"), Type::Path)]))),
                ),
                (
                    SchemeType::Union(vec![int(), path()]),
                    Type::Union(vec![Type::Int, Type::Path]),
                ),
                (
                    SchemeType::Validated(
                        Validation::NonEmpty,
                        Box::new(SchemeType::List(Box::new(path()))),
                    ),
                    Type::non_empty(Type::Path),
                ),
                (
                    SchemeType::Validated(Validation::RelPath, Box::new(path())),
                    Type::rel_path(),
                ),
                (
                    SchemeType::Validated(
                        Validation::Nominal(Name::intern("Package")),
                        Box::new(record()),
                    ),
                    Type::nominal(
                        Name::intern("Package"),
                        Type::Record(Arc::new(BTreeMap::from([(Name::intern("path"), Type::Path)]))),
                    ).unwrap(),
                ),
                (
                    SchemeType::Validated(
                        Validation::Range(IntRange::new(1, 8).unwrap()),
                        Box::new(int()),
                    ),
                    Type::bounded(IntRange::new(1, 8).unwrap(), Type::Int).unwrap(),
                ),
            ];
            for (scheme, expected) in cases {
                assert_eq!(scheme.closed_ground_type(), Some(expected));
            }
        });
    }

    #[test]
    fn closed_scheme_type_projection_accepts_any_and_rejects_open_shapes() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            assert_eq!(
                SchemeType::Ground(Type::Any).closed_ground_type(),
                Some(Type::Any),
            );
            assert_eq!(
                SchemeType::Ground(Type::Invalid).closed_ground_type(),
                None,
            );
            assert_eq!(
                SchemeType::List(Box::new(SchemeType::Parameter(
                    super::SchemeTypeParameterId(0),
                )))
                .closed_ground_type(),
                None,
            );
            assert_eq!(
                SchemeType::Record {
                    fields: BTreeMap::from([(Name::intern("path"), SchemeType::Ground(Type::Path))]),
                    tail: SchemeRowTail::Parameter(super::SchemeRowParameterId(0)),
                }
                .closed_ground_type(),
                None,
            );
        });
    }

    #[test]
    fn closed_scheme_type_projection_rejects_builtin_parameters() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let builtin_parameter = Type::BuiltinParameter(
                crate::sema::types::BuiltinTypeParameter::Element,
            );
            assert_eq!(
                SchemeType::Ground(builtin_parameter.clone()).closed_ground_type(),
                None,
            );
            assert_eq!(
                SchemeType::List(Box::new(SchemeType::Ground(Type::Record(Arc::new(
                    BTreeMap::from([(
                        Name::intern("items"),
                        Type::List(Box::new(builtin_parameter)),
                    )]),
                )))))
                .closed_ground_type(),
                None,
            );
        });
    }

    fn rejects_recovery_ground(ty: Type) {
        assert!(matches!(
            TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(ty),
                None,
                false,
                Some(Vec::new()),
                false,
            ),
            Err(SchemeBuildError::RecoveryType)
        ));
    }

    #[test]
    fn type_scheme_rejects_recovery_nested_in_sets_and_module_exports() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            rejects_recovery_ground(Type::Set(Box::new(Type::Unknown)));
            rejects_recovery_ground(Type::Set(Box::new(Type::Invalid)));
            rejects_recovery_ground(Type::Module(Arc::new(ModuleType::open(BTreeMap::from([(
                Name::intern("value"),
                ModuleExportType::Value {
                    ty: Type::Unknown,
                    optional: false,
                },
            )])))));
        });
    }

    #[test]
    fn checked_scheme_reuses_its_callable_shape() {
        let callable = Arc::new(
            super::super::schemes::CallableScheme::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(Type::Int),
                None,
                false,
                super::CallableRequirements::default(),
            )
            .unwrap(),
        );
        let scheme = TypeScheme::with_callable(
            crate::sema::types::CallableKind::Pure,
            Arc::clone(&callable),
            super::CheckedEffects::unknown(),
            false,
        )
        .unwrap();
        assert!(Arc::ptr_eq(scheme.callable(), &callable));
    }

    #[test]
    fn scheme_ground_type_mapping_visits_both_map_components() {
        let scheme = TypeScheme::new(
            crate::sema::types::CallableKind::Pure,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SchemeType::Map(
                Box::new(SchemeType::Ground(Type::Int)),
                Box::new(SchemeType::Ground(Type::Int)),
            ),
            None,
            false,
            None,
            false,
        )
        .unwrap();

        let mapped = scheme.map_ground_types(|ty| {
            if *ty == Type::Int {
                *ty = Type::Str;
            }
        });

        assert_eq!(
            mapped.return_type(),
            &SchemeType::Map(
                Box::new(SchemeType::Ground(Type::Str)),
                Box::new(SchemeType::Ground(Type::Str)),
            )
        );
    }

    #[test]
    fn authored_dynamic_body_types_preserve_checked_scope_validation() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let scheme = Arc::new(TypeScheme::new(
                crate::sema::types::CallableKind::Pure, Vec::new(), Vec::new(),
                vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Ground(Type::Any), schema_expectation: None, defaulted: false, rest: false }],
                SchemeType::Ground(Type::Int), None, true, Some(Vec::new()), false,
            ).unwrap());
            let expression = crate::syntax::arena::ExprId::from_index(0);
            let make = |ty| super::SchemeBodyFacts::new(Arc::clone(&scheme), BTreeMap::from([(expression, ty)]), BTreeMap::new());
            for actual in [SchemeType::Ground(Type::Any), SchemeType::Ground(Type::List(Box::new(Type::Any))), SchemeType::Result(Box::new(SchemeType::Ground(Type::Any)), Box::new(SchemeType::Ground(Type::Error)))] {
                assert_eq!(make(actual.clone()).unwrap().expression_types().get(&expression), Some(&actual));
            }
            let inference = crate::sema::constraints::TypeConstraints::default().fresh(crate::source::Span::at(crate::source::SourceId::new(0), 0));
            for (ty, expected) in [
                (SchemeType::Ground(Type::List(Box::new(Type::Unknown))), SchemeBuildError::RecoveryType),
                (SchemeType::Ground(Type::Optional(Box::new(inference))), SchemeBuildError::UnresolvedType),
                (SchemeType::Parameter(super::SchemeTypeParameterId(9)), SchemeBuildError::UnboundTypeParameter),
                (SchemeType::Record { fields: BTreeMap::new(), tail: SchemeRowTail::Parameter(SchemeRowParameterId(9)) }, SchemeBuildError::UnboundRowParameter),
                (SchemeType::Callable { kind: crate::sema::types::CallableKind::Proc, monotype: Arc::new(super::CallableMonotype::new(Vec::new(), SchemeType::Ground(Type::Int), None)), effects: EffectContract::variable(super::SchemeEffectParameterId(9)) }, SchemeBuildError::UnboundEffectParameter),
            ] {
                assert_eq!(make(ty).unwrap_err(), expected);
            }
        });
    }

    #[test]
    fn row_evidence_retains_opaque_data_and_call_instantiations_require_valid_evidence() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let field = Name::intern("dynamic");
            let opaque = SchemeType::Ground(Type::Set(Box::new(Type::Any)));
            let data = RowEvidence::new(BTreeMap::from([(field, opaque.clone())]), SchemeRowTail::Closed).unwrap();
            assert_eq!(data.fields(), &BTreeMap::from([(field, opaque)]));
            for (invalid, expected) in [
                (Type::Unknown, SchemeBuildError::RecoveryType),
                (Type::Record(Arc::new(BTreeMap::from([(field, Type::Any), (Name::intern("bad"), Type::Invalid)]))), SchemeBuildError::RecoveryType),
            ] {
                assert_eq!(RowEvidence::new(BTreeMap::from([(field, SchemeType::Ground(invalid))]), SchemeRowTail::Closed), Err(expected));
            }

            let row_id = SchemeRowParameterId(0);
            let callable = CallableScheme::new(
                Vec::new(),
                vec![SchemeRowParameter {
                    id: row_id,
                    lacks: BTreeSet::new(),
                }],
                vec![SchemeParameter {
                    name: Name::intern("record"),
                    ty: SchemeType::Record {
                        fields: BTreeMap::new(),
                        tail: SchemeRowTail::Parameter(row_id),
                    },
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Ground(Type::Int),
                None,
                false,
                super::CallableRequirements::default(),
            )
            .unwrap();
            let evidence = RowEvidence::new(
                BTreeMap::from([(field, SchemeType::Ground(Type::Int))]),
                SchemeRowTail::Closed,
            )
            .unwrap();
            assert!(CallInstantiation::new(
                &callable,
                BTreeMap::new(),
                BTreeMap::from([(row_id, evidence)]), BTreeMap::new(), Vec::new(), Vec::new(), Vec::new(),
                &[],
                &[],
                &[],
            &[], &[], &[], &[], &BTreeMap::new())
            .is_ok());
            assert!(TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(Type::Any),
                None,
                true,
                None,
                false,
            )
            .is_ok());
        });
    }

    #[test]
    fn call_instantiation_checks_nested_record_row_requirements() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let type_id = super::SchemeTypeParameterId(0);
            let row_id = SchemeRowParameterId(0);
            let field = Name::intern("name");
            let callable = CallableScheme::new(
                vec![type_id],
                Vec::new(),
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Parameter(type_id),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(type_id),
                None,
                false,
                super::CallableRequirements::default(),
            )
            .unwrap();
            let type_argument = SchemeType::Record {
                fields: BTreeMap::from([(field, SchemeType::Ground(Type::Int))]),
                tail: SchemeRowTail::Parameter(row_id),
            };

            assert_eq!(
                CallInstantiation::new(
                    &callable,
                    BTreeMap::from([(type_id, type_argument)]),
                    BTreeMap::new(), BTreeMap::new(), Vec::new(), Vec::new(), Vec::new(),
                    &[],
                    &[SchemeRowParameter {
                        id: row_id,
                        lacks: BTreeSet::new(),
                    }],
                    &[],
                &[], &[], &[], &[], &BTreeMap::new()),
                Err(SchemeBuildError::RowLacksMismatch),
            );
        });
    }

    #[test]
    fn type_scheme_retained_bytes_include_row_and_schema_payloads() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let field = Name::intern("record_field");
            let parameter_name = Name::intern("parameter_name");
            let plain = TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(Type::Int),
                None,
                false,
                None,
                false,
            )
            .unwrap();
            let expectation = SchemaExpectation {
                instances: vec![SchemaInstance {
                    definition: TypeDefId::from_index(0),
                    arguments: vec![SchemeType::List(Box::new(SchemeType::Ground(Type::Str)))],
                }],
                children: Arc::new(BTreeMap::from([(
                    SchemaComponent::Field(field),
                    SchemaExpectation::default(),
                )])),
            };
            let scheme = TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                vec![SchemeRowParameter {
                    id: SchemeRowParameterId(0),
                    lacks: BTreeSet::from([field]),
                }],
                vec![SchemeParameter {
                    name: parameter_name,
                    ty: SchemeType::Record {
                        fields: BTreeMap::new(),
                        tail: SchemeRowTail::Parameter(SchemeRowParameterId(0)),
                    },
                    schema_expectation: Some(expectation.clone()),
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Ground(Type::Int),
                Some(expectation),
                false,
                None,
                false,
            )
            .unwrap();

            let required_payload_bytes = field.as_str().len()
                + parameter_name.as_str().len()
                + 2 * std::mem::size_of::<Name>()
                + std::mem::size_of::<SchemaInstance<SchemeType>>()
                + std::mem::size_of::<Type>()
                + std::mem::size_of::<(SchemaComponent, SchemaExpectation<SchemeType>)>();
            assert!(scheme.retained_bytes() >= plain.retained_bytes() + required_payload_bytes);
        });
    }

    #[test]
    fn call_instantiation_retained_bytes_include_keys_and_evidence_payloads() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let type_id = super::SchemeTypeParameterId(0);
            let row_id = SchemeRowParameterId(0);
            let field = Name::intern("payload");
            let extra = Name::intern("extra");
            let callable = CallableScheme::new(
                vec![type_id],
                vec![SchemeRowParameter {
                    id: row_id,
                    lacks: BTreeSet::from([field]),
                }],
                vec![SchemeParameter {
                    name: Name::intern("record"),
                    ty: SchemeType::Record {
                        fields: BTreeMap::from([(
                            field,
                            SchemeType::Parameter(type_id),
                        )]),
                        tail: SchemeRowTail::Parameter(row_id),
                    },
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(type_id),
                None,
                false,
                super::CallableRequirements::default(),
            )
            .unwrap();
            let type_argument = Type::List(Box::new(Type::Str));
            let row_evidence = RowEvidence::new(
                BTreeMap::from([(extra, SchemeType::Ground(Type::Bool))]),
                SchemeRowTail::Closed,
            )
            .unwrap();
            let instantiation = CallInstantiation::new(
                &callable,
                BTreeMap::from([(type_id, SchemeType::Ground(type_argument.clone()))]),
                BTreeMap::from([(row_id, row_evidence)]), BTreeMap::new(), Vec::new(), Vec::new(), Vec::new(),
                &[],
                &[],
                &[],
                &[], &[], &[], &[], &BTreeMap::new())
            .unwrap();

            let required_bytes = std::mem::size_of::<CallInstantiation>()
                + std::mem::size_of::<super::SchemeTypeParameterId>()
                + type_argument.retained_bytes()
                + std::mem::size_of::<SchemeRowParameterId>()
                + instantiation.row_arguments().get(&row_id).unwrap().retained_bytes();
            assert!(instantiation.retained_bytes() >= required_bytes);
        });
    }
}
