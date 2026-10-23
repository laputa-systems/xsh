use super::schema::{
    FullSchemeSchemaComponentTag, FullSchemeTypeTag, FullSchemeValidationTag,
};
use super::{
    FullBuilder, FullStore, IrBuildError, IrFunctionId, IrRange, IR_NONE, SignatureId, TypeId,
};
use crate::sema::constants::{SchemaComponent, SchemaExpectation};
use crate::sema::schemes::{
    CallInstantiation, RowEvidence, SchemeRowTail,
    SchemeType, SchemeTypeParameterId, SchemeRowParameterId, SchemeEffectParameterId, TypeScheme,
};
use crate::sema::types::Type;
use crate::sema::validated::Validation;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(super) struct FullSchemeId(u32);

impl FullSchemeId {
    pub(super) fn new(index: usize) -> Result<Self, IrBuildError> {
        let raw = u32::try_from(index)
            .ok()
            .filter(|raw| *raw != IR_NONE)
            .ok_or_else(|| IrBuildError::format("scheme_overflow", None, 0, 0))?;
        Ok(Self(raw))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }

    pub(super) fn from_raw(raw: u32) -> Result<Self, super::IrVerifyError> {
        (raw != IR_NONE)
            .then_some(Self(raw))
            .ok_or_else(|| super::IrVerifyError::new("full scheme id is invalid"))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(super) struct SchemeTypeId(u32);

impl SchemeTypeId {
    pub(super) fn new(index: usize) -> Result<Self, IrBuildError> {
        let raw = u32::try_from(index)
            .ok()
            .filter(|raw| *raw != IR_NONE)
            .ok_or_else(|| IrBuildError::format("scheme_type_overflow", None, 0, 0))?;
        Ok(Self(raw))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }

    pub(super) fn raw(self) -> u32 {
        self.0
    }

    pub(super) fn from_raw(raw: u32) -> Result<Self, super::IrVerifyError> {
        (raw != IR_NONE)
            .then_some(Self(raw))
            .ok_or_else(|| super::IrVerifyError::new("scheme type id is invalid"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullCallableContract {
    Ground(SignatureId),
    Scheme(FullSchemeId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum FullSchemeBodyFactKind {
    Operation,
    Method,
    Wrapper,
    CompletionSource,
    Expression,
    Tail,
    LocalImmutable,
    LocalMutable,
    EmptyList,
    EmptyMap,
    RecordProducer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullScheme {
    pub(super) type_parameters: IrRange,
    pub(super) row_parameters: IrRange,
    pub(super) effect_parameters: IrRange,
    pub(super) params: IrRange,
    pub(super) return_type: SchemeTypeId,
    pub(super) return_schema: Option<FullSchemeSchemaExpectationId>,
    pub(super) effects: FullEffectContract,
    pub(super) effect_predicates: IrRange,
    pub(super) applications: IrRange,
    pub(super) operations: IrRange,
    pub(super) methods: IrRange,
    pub(super) wrappers: IrRange,
    pub(super) body_facts: IrRange,
    pub(super) callable_kind: crate::sema::types::CallableKind,
    pub(super) explicit_return: bool,
    pub(super) inferred_effects: bool,
    pub(super) reserved: [u8; 3],
}

impl FullScheme {
    pub(super) fn is_quantified(&self, store: &super::FullStore) -> Result<bool, super::IrVerifyError> {
        let types = self.type_parameters.bounds(store.scheme_type_parameters.len())
            .ok_or_else(|| super::IrVerifyError::new("generic callable type binder range is invalid"))?;
        let rows = self.row_parameters.bounds(store.scheme_row_parameters.len())
            .ok_or_else(|| super::IrVerifyError::new("generic callable row binder range is invalid"))?;
        let effects = self.effect_parameters.bounds(store.scheme_effect_parameters.len())
            .ok_or_else(|| super::IrVerifyError::new("generic callable effect binder range is invalid"))?;
        Ok(!types.is_empty() || !rows.is_empty() || !effects.is_empty())
    }
}


#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
#[repr(C)]
pub(super) struct FullEffectContract {
    pub(super) direct: IrRange,
    pub(super) terms: IrRange,
    pub(super) known: bool,
    pub(super) reserved: [u8; 3],
}

impl FullEffectContract {
    pub(super) fn concrete(self, store: &FullStore) -> Result<Option<Vec<crate::syntax::node::Effect>>, super::IrVerifyError> {
        let direct = self.direct.bounds(store.scheme_effects.len())
            .ok_or_else(|| super::IrVerifyError::new("scheme direct effect range is invalid"))?;
        let terms = self.terms.bounds(store.scheme_effect_terms.len())
            .ok_or_else(|| super::IrVerifyError::new("scheme effect term range is invalid"))?;
        if self.reserved != [0; 3] || !self.known && (!direct.is_empty() || !terms.is_empty()) {
            return Err(super::IrVerifyError::new("scheme effect contract metadata is invalid"));
        }
        if !terms.is_empty() {
            return Err(super::IrVerifyError::new("symbolic scheme effects require an invocation substitution"));
        }
        if direct.len() > crate::syntax::node::Effect::ALL.len() {
            return Err(super::IrVerifyError::new("scheme direct effects exceed the capability set"));
        }
        let effects = &store.scheme_effects[direct];
        for (index, effect) in effects.iter().enumerate() {
            if effects[..index].contains(effect) {
                return Err(super::IrVerifyError::new("scheme direct effects are duplicated"));
            }
        }
        Ok(self.known.then(|| effects.to_vec()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullEffectParameter {
    pub(super) id: u32,
    pub(super) binder: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullEffectTerm {
    pub(super) binder: u32,
    pub(super) mask: u8,
    pub(super) reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullEffectPredicate {
    Equal(FullEffectContract, FullEffectContract),
    Included(FullEffectContract, FullEffectContract),
    Covered(FullEffectContract, FullEffectContract),
    Excluded(FullEffectContract, u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullEffectArgument {
    pub(super) binder: u32,
    pub(super) expression: FullEffectContract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeApplication {
    pub(super) callee: SchemeTypeId,
    pub(super) result: SchemeTypeId,
    pub(super) effects: FullEffectContract,
    pub(super) arguments: IrRange,
    pub(super) proc_returns: IrRange,
    pub(super) kind: crate::sema::callable_application::CallableKindRequirement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullApplicationArgumentKind {
    Positional,
    Named(u32),
    Splice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullApplicationArgument {
    pub(super) ty: SchemeTypeId,
    pub(super) kind: FullApplicationArgumentKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullProcReturnEnvelope {
    pub(super) result: SchemeTypeId,
    pub(super) propagated_errors: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullApplicationBinding {
    Selected { argument_slots: IrRange, omitted_slots: IrRange },
    Forwarded(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeApplicationReturn {
    pub(super) instruction: u32,
    pub(super) function: super::IrFunctionId,
    pub(super) application: u32,
    pub(super) envelope: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeTypeParameter {
    pub(super) id: u32,
    pub(super) binder: u32,
    pub(super) iteration_item: u32,
    pub(super) iteration_error: u32,
    pub(super) map_key: bool,
    pub(super) conversions: u8,
    pub(super) iteration_source_kind: crate::sema::schemes::IterationSourceKind,
    pub(super) reserved: [u8; 1],
}

impl FullSchemeTypeParameter {
    pub(super) fn iteration(self) -> Result<Option<crate::sema::schemes::IterationRequirement<SchemeTypeId>>, super::IrVerifyError> {
        use crate::sema::schemes::{IterationFailure, IterationRequirement, IterationSourceKind};
        if self.iteration_item == IR_NONE {
            if self.iteration_error != IR_NONE || self.iteration_source_kind != IterationSourceKind::Direct {
                return Err(super::IrVerifyError::new("scheme iteration requirement has no source item"));
            }
            return Ok(None);
        }
        let item = SchemeTypeId::from_raw(self.iteration_item)?;
        let failure = if self.iteration_error == IR_NONE { IterationFailure::Infallible }
            else { IterationFailure::Propagates(SchemeTypeId::from_raw(self.iteration_error)?) };
        Ok(Some(IterationRequirement { item, failure, source_kind: self.iteration_source_kind }))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeRowParameter {
    pub(super) id: u32,
    pub(super) binder: u32,
    pub(super) lacks: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeParam {
    pub(super) ty: SchemeTypeId,
    pub(super) schema: Option<FullSchemeSchemaExpectationId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeCallableMonotype {
    pub(super) params: IrRange,
    pub(super) return_type: SchemeTypeId,
    pub(super) return_schema: Option<FullSchemeSchemaExpectationId>,
    pub(super) effects: FullEffectContract,
    pub(super) kind: crate::sema::types::CallableKind,
    pub(super) reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeCallableParameter {
    pub(super) name: u32,
    pub(super) ty: SchemeTypeId,
    pub(super) schema: Option<FullSchemeSchemaExpectationId>,
    pub(super) flags: u8,
    pub(super) reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeTypeNode {
    pub(super) first: u32,
    pub(super) second: u32,
    pub(super) children: IrRange,
    pub(super) tag: FullSchemeTypeTag,
    pub(super) reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeField {
    pub(super) name: u32,
    pub(super) ty: SchemeTypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeValidation {
    pub(super) name: u32,
    pub(super) low: i64,
    pub(super) high: i64,
    pub(super) tag: FullSchemeValidationTag,
    pub(super) reserved: [u8; 7],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(super) struct FullSchemeSchemaExpectationId(u32);

impl FullSchemeSchemaExpectationId {
    fn new(index: usize) -> Result<Self, IrBuildError> {
        let raw = u32::try_from(index)
            .ok()
            .filter(|raw| *raw != IR_NONE)
            .ok_or_else(|| IrBuildError::format("scheme_schema_overflow", None, 0, 0))?;
        Ok(Self(raw))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeSchemaExpectation {
    pub(super) instances: IrRange,
    pub(super) children: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeSchemaInstance {
    pub(super) definition: u32,
    pub(super) arguments: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeSchemaChild {
    pub(super) component: FullSchemeSchemaComponentTag,
    pub(super) reserved: [u8; 3],
    pub(super) name: u32,
    pub(super) expectation: FullSchemeSchemaExpectationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullCallInstantiation {
    pub(super) instruction: u32,
    pub(super) arguments: FullInstantiation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullInstantiation {
    pub(super) type_arguments: IrRange,
    pub(super) row_arguments: IrRange,
    pub(super) effect_arguments: IrRange,
    pub(super) application_bindings: IrRange,
    pub(super) operation_bindings: IrRange,
    pub(super) method_bindings: IrRange,
    pub(super) wrapper_bindings: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullWrapper {
    Field { source: SchemeTypeId, result: SchemeTypeId, detail: u32 },
    Fallback { source: SchemeTypeId, result: SchemeTypeId, backup: SchemeTypeId },
    Handler { source: SchemeTypeId, result: SchemeTypeId, detail: u32 },
    Capture { result: SchemeTypeId, detail: u32 },
    ProcCompletion { returned: SchemeTypeId, group: u32, detail: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullWrapperBoundary {
    Captured,
    Return(SchemeTypeId),
    ErrorReturn(SchemeTypeId),
    ProcCompletion { returned: SchemeTypeId, group: u32 },
    Producer,
    TopLevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullWrapperField {
    pub(super) name: u32,
    pub(super) boundary: FullWrapperBoundary,
    pub(super) effects: FullEffectContract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullWrapperHandler {
    pub(super) error: SchemeTypeId,
    pub(super) handler: SchemeTypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullWrapperCapture {
    pub(super) contributions: IrRange,
    pub(super) expected: bool,
    pub(super) reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullWrapperCompletion {
    pub(super) contributions: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullWrapperContribution {
    Explicit(SchemeTypeId),
    Source(SchemeTypeId),
    PropagatedError(SchemeTypeId),
    ReturnedResult(SchemeTypeId),
    FieldSource(SchemeTypeId),
    ApplicationSource(SchemeTypeId),
}

pub(super) use crate::sema::wrapper_operation::WrapperBinding as FullWrapperBinding;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullSealedOperation {
    Binary { left: SchemeTypeId, right: SchemeTypeId, result: SchemeTypeId, operator: xsh_registry::signature::ArithmeticOperator },
    Index { receiver: SchemeTypeId, key: SchemeTypeId, result: SchemeTypeId, syntax: crate::sema::sealed_operation::IndexSyntax, constant_key: u32 },
    Slice { receiver: SchemeTypeId, result: SchemeTypeId, bounds: crate::sema::sealed_operation::SliceBounds },
    RecordOverride { receiver: SchemeTypeId, result: SchemeTypeId, overrides: IrRange },
}

impl FullSealedOperation {
    pub(super) fn inputs(self, store: &super::FullStore) -> Result<impl Iterator<Item = SchemeTypeId> + '_, super::IrVerifyError> {
        let (fixed, overrides): (_, &[FullSchemeField]) = match self {
            Self::Binary { left, right, .. } => ([Some(left), Some(right)], &[]),
            Self::Index { receiver, key, .. } => ([Some(receiver), Some(key)], &[]),
            Self::Slice { receiver, .. } => ([Some(receiver), None], &[]),
            Self::RecordOverride { receiver, overrides, .. } => {
                let range = overrides.bounds(store.scheme_fields.len()).ok_or_else(|| super::IrVerifyError::new("record override field range is invalid"))?;
                ([Some(receiver), None], &store.scheme_fields[range])
            }
        };
        Ok(fixed.into_iter().flatten().chain(overrides.iter().map(|field| field.ty)))
    }
    pub(super) fn result(self) -> SchemeTypeId {
        match self { Self::Binary { result, .. } | Self::Index { result, .. } | Self::Slice { result, .. } | Self::RecordOverride { result, .. } => result }
    }
    pub(super) fn types(self, store: &super::FullStore) -> Result<impl Iterator<Item = SchemeTypeId> + '_, super::IrVerifyError> {
        Ok(self.inputs(store)?.chain(std::iter::once(self.result())))
    }
}

pub(super) use crate::sema::schemes::SealedOperationBinding as FullOperationBinding;
pub(super) use crate::sema::schemes::SealedMethodBinding as FullMethodBinding;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeMethod {
    pub(super) family: xsh_registry::signature::MethodFamilyId,
    pub(super) reserved: [u8; 3],
    pub(super) receiver: SchemeTypeId,
    pub(super) argument_slots: IrRange,
    pub(super) arguments: IrRange,
    pub(super) result: SchemeTypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeApplicationCall {
    pub(super) instruction: u32,
    pub(super) application: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullTypeArgument {
    pub(super) binder: u32,
    pub(super) ty: SchemeTypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullRowArgument {
    pub(super) binder: u32,
    pub(super) fields: IrRange,
    pub(super) tail: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullRowEvidenceField {
    pub(super) name: u32,
    pub(super) ty: SchemeTypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeBodyFact {
    pub(super) instruction: u32,
    pub(super) function: IrFunctionId,
    pub(super) value: FullSchemeBodyFactValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullSchemeBodyFactValue {
    Operation(u32),
    Method(u32),
    Wrapper(u32),
    CompletionLiftedValue(u32),
    CompletionReturnedResult(u32),
    CompletionAuthoredOk(u32),
    CompletionAuthoredErr(u32),
    Expression(SchemeTypeId),
    Tail(SchemeTypeId),
    LocalImmutable(SchemeTypeId),
    LocalMutable(SchemeTypeId),
    EmptyList,
    EmptyMap,
    RecordProducer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FullSchemeBodyCompletionSource {
    pub(super) producer: u32,
    pub(super) form: crate::runtime::eval::SchemeCompletionSourceForm,
}

impl FullSchemeBodyFact {
    pub(super) fn kind(self) -> FullSchemeBodyFactKind {
        match self.value {
            FullSchemeBodyFactValue::Operation(_) => FullSchemeBodyFactKind::Operation,
            FullSchemeBodyFactValue::Method(_) => FullSchemeBodyFactKind::Method,
            FullSchemeBodyFactValue::Wrapper(_) => FullSchemeBodyFactKind::Wrapper,
            FullSchemeBodyFactValue::CompletionLiftedValue(_)
            | FullSchemeBodyFactValue::CompletionReturnedResult(_)
            | FullSchemeBodyFactValue::CompletionAuthoredOk(_)
            | FullSchemeBodyFactValue::CompletionAuthoredErr(_) => FullSchemeBodyFactKind::CompletionSource,
            FullSchemeBodyFactValue::Expression(_) => FullSchemeBodyFactKind::Expression,
            FullSchemeBodyFactValue::Tail(_) => FullSchemeBodyFactKind::Tail,
            FullSchemeBodyFactValue::LocalImmutable(_) => FullSchemeBodyFactKind::LocalImmutable,
            FullSchemeBodyFactValue::LocalMutable(_) => FullSchemeBodyFactKind::LocalMutable,
            FullSchemeBodyFactValue::EmptyList => FullSchemeBodyFactKind::EmptyList,
            FullSchemeBodyFactValue::EmptyMap => FullSchemeBodyFactKind::EmptyMap,
            FullSchemeBodyFactValue::RecordProducer => FullSchemeBodyFactKind::RecordProducer,
        }
    }

    pub(super) fn ty(self) -> Option<SchemeTypeId> {
        match self.value {
            FullSchemeBodyFactValue::Expression(ty) | FullSchemeBodyFactValue::Tail(ty)
            | FullSchemeBodyFactValue::LocalImmutable(ty) | FullSchemeBodyFactValue::LocalMutable(ty) => Some(ty),
            FullSchemeBodyFactValue::EmptyList | FullSchemeBodyFactValue::EmptyMap | FullSchemeBodyFactValue::RecordProducer
            | FullSchemeBodyFactValue::Operation(_) | FullSchemeBodyFactValue::Method(_) | FullSchemeBodyFactValue::Wrapper(_)
            | FullSchemeBodyFactValue::CompletionLiftedValue(_) | FullSchemeBodyFactValue::CompletionReturnedResult(_)
            | FullSchemeBodyFactValue::CompletionAuthoredOk(_) | FullSchemeBodyFactValue::CompletionAuthoredErr(_) => None,
        }
    }

}

impl FullSchemeBodyFactValue {
    pub(super) fn completion_source(self) -> Option<FullSchemeBodyCompletionSource> {
        let (producer, form) = match self {
            Self::CompletionLiftedValue(producer) => (producer, crate::runtime::eval::SchemeCompletionSourceForm::LiftedValue),
            Self::CompletionReturnedResult(producer) => (producer, crate::runtime::eval::SchemeCompletionSourceForm::ReturnedResult),
            Self::CompletionAuthoredOk(producer) => (producer, crate::runtime::eval::SchemeCompletionSourceForm::AuthoredOk),
            Self::CompletionAuthoredErr(producer) => (producer, crate::runtime::eval::SchemeCompletionSourceForm::AuthoredErr),
            _ => return None,
        };
        Some(FullSchemeBodyCompletionSource { producer, form })
    }
}

impl FullStore {
    pub(super) fn scheme_callable_monotype(&self, index: u32) -> Result<&FullSchemeCallableMonotype, super::IrVerifyError> {
        self.scheme_callable_monotypes.get(index as usize)
            .ok_or_else(|| super::IrVerifyError::new("scheme callable monotype id is out of bounds"))
    }

    pub(super) fn scheme(&self, id: FullSchemeId) -> Result<&FullScheme, super::IrVerifyError> {
        self.schemes
            .get(id.index())
            .ok_or_else(|| super::IrVerifyError::new("full scheme id is out of bounds"))
    }

    pub(super) fn scheme_type(
        &self,
        id: SchemeTypeId,
    ) -> Result<&FullSchemeTypeNode, super::IrVerifyError> {
        self.scheme_types
            .get(id.index())
            .ok_or_else(|| super::IrVerifyError::new("scheme type id is out of bounds"))
    }
}

impl FullBuilder {
    pub(super) fn intern_effect_contract(
        &mut self,
        contract: &crate::sema::effects::EffectContract<SchemeEffectParameterId>,
        parameters: &BTreeMap<SchemeEffectParameterId, u32>,
    ) -> Result<FullEffectContract, IrBuildError> {
        let direct_start = self.store.scheme_effects.len();
        let terms_start = self.store.scheme_effect_terms.len();
        let known = match contract {
            crate::sema::effects::EffectContract::Unknown => false,
            crate::sema::effects::EffectContract::Known(expression) => {
                let mut terms = expression.variables().map(|(parameter, mask)| {
                    let binder = *parameters.get(parameter)
                        .ok_or_else(|| IrBuildError::format("scheme_effect_parameter_scope", None, 0, 0))?;
                    Ok(FullEffectTerm { binder, mask: mask.bits(), reserved: [0; 3] })
                }).collect::<Result<Vec<_>, IrBuildError>>()?;
                terms.sort_by_key(|term| term.binder);
                self.store.scheme_effects.extend(expression.direct().iter());
                self.store.scheme_effect_terms.extend(terms);
                true
            }
        };
        Ok(FullEffectContract {
            direct: super::table_range(direct_start, self.store.scheme_effects.len())?,
            terms: super::table_range(terms_start, self.store.scheme_effect_terms.len())?,
            known,
            reserved: [0; 3],
        })
    }

    fn intern_effect_predicate(
        &mut self,
        predicate: &crate::sema::effects::EffectPredicate<SchemeEffectParameterId>,
        parameters: &BTreeMap<SchemeEffectParameterId, u32>,
    ) -> Result<FullEffectPredicate, IrBuildError> {
        use crate::sema::effects::EffectPredicate;
        Ok(match predicate {
            EffectPredicate::Equal(left, right) => FullEffectPredicate::Equal(
                self.intern_effect_contract(left, parameters)?, self.intern_effect_contract(right, parameters)?),
            EffectPredicate::Included(left, right) => FullEffectPredicate::Included(
                self.intern_effect_contract(left, parameters)?, self.intern_effect_contract(right, parameters)?),
            EffectPredicate::Covered(left, right) => FullEffectPredicate::Covered(
                self.intern_effect_contract(left, parameters)?, self.intern_effect_contract(right, parameters)?),
            EffectPredicate::Excluded(expression, excluded) => FullEffectPredicate::Excluded(
                self.intern_effect_contract(expression, parameters)?, excluded.bits()),
        })
    }

    pub(super) fn scheme_effect_parameter_map(
        &self, scheme: FullSchemeId,
    ) -> Result<BTreeMap<SchemeEffectParameterId, u32>, IrBuildError> {
        let record = self.store.schemes.get(scheme.index())
            .ok_or_else(|| IrBuildError::format("scheme_id", None, 0, 0))?;
        let range = record.effect_parameters.bounds(self.store.scheme_effect_parameters.len())
            .ok_or_else(|| IrBuildError::format("scheme_effect_parameters", None, 0, 0))?;
        Ok(self.store.scheme_effect_parameters[range].iter()
            .map(|parameter| (SchemeEffectParameterId(parameter.id), parameter.binder)).collect())
    }

    pub(super) fn intern_scheme(
        &mut self,
        scheme: &TypeScheme,
    ) -> Result<FullSchemeId, IrBuildError> {
        let type_parameters = scheme
            .type_parameters()
            .iter()
            .copied()
            .enumerate()
            .map(|(binder, id)| {
                Ok((id, u32::try_from(binder).map_err(|_| {
                    IrBuildError::format("scheme_type_binder_overflow", None, 0, 0)
                })?))
            })
            .collect::<Result<BTreeMap<_, _>, IrBuildError>>()?;
        let row_parameters = scheme
            .row_parameters()
            .iter()
            .enumerate()
            .map(|(binder, parameter)| {
                Ok((
                    parameter.id,
                    u32::try_from(binder).map_err(|_| {
                        IrBuildError::format("scheme_row_binder_overflow", None, 0, 0)
                    })?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, IrBuildError>>()?;

        let effect_parameters = scheme.callable().requirements().effect_parameters.iter().copied().enumerate()
            .map(|(binder, id)| Ok((id, u32::try_from(binder).map_err(|_| IrBuildError::format("scheme_effect_binder_overflow", None, 0, 0))?)))
            .collect::<Result<BTreeMap<_, _>, IrBuildError>>()?;
        let effect_parameters_start = self.store.scheme_effect_parameters.len();
        for &id in &scheme.callable().requirements().effect_parameters {
            self.store.scheme_effect_parameters.push(FullEffectParameter { id: id.0, binder: effect_parameters[&id] });
        }
        let effect_parameters_range = super::table_range(effect_parameters_start, self.store.scheme_effect_parameters.len())?;
        let type_parameters_start = self.store.scheme_type_parameters.len();
        for (binder, id) in scheme.type_parameters().iter().copied().enumerate() {
            let iteration = scheme.callable().requirements().iteration_parameters.get(&id).map(|iteration| {
                let item = self.intern_scheme_type(&iteration.item, &type_parameters, &row_parameters, &effect_parameters)?;
                let failure = match &iteration.failure {
                    crate::sema::schemes::IterationFailure::Infallible => crate::sema::schemes::IterationFailure::Infallible,
                    crate::sema::schemes::IterationFailure::Propagates(error) => crate::sema::schemes::IterationFailure::Propagates(self.intern_scheme_type(error, &type_parameters, &row_parameters, &effect_parameters)?),
                };
                Ok::<_, IrBuildError>(crate::sema::schemes::IterationRequirement { item, failure, source_kind: iteration.source_kind })
            }).transpose()?;
            self.store.scheme_type_parameters.push(FullSchemeTypeParameter {
                id: id.0,
                binder: u32::try_from(binder).map_err(|_| {
                    IrBuildError::format("scheme_type_binder_overflow", None, 0, 0)
                })?,
                iteration_item: iteration.map_or(IR_NONE, |iteration| iteration.item.raw()),
                iteration_error: iteration.map_or(IR_NONE, |iteration| match iteration.failure {
                    crate::sema::schemes::IterationFailure::Infallible => IR_NONE,
                    crate::sema::schemes::IterationFailure::Propagates(error) => error.raw(),
                }),
                iteration_source_kind: iteration.map_or(crate::sema::schemes::IterationSourceKind::Direct, |iteration| iteration.source_kind),
                map_key: scheme.callable().requirements().map_key_parameters.contains(&id),
                conversions: scheme.callable().requirements().conversion_parameters.get(&id).copied().unwrap_or_default().bits(),
                reserved: [0; 1],
            });
        }
        let type_parameters_range = super::table_range(
            type_parameters_start,
            self.store.scheme_type_parameters.len(),
        )?;

        let row_parameters_start = self.store.scheme_row_parameters.len();
        for (binder, parameter) in scheme.row_parameters().iter().enumerate() {
            let lacks_start = self.store.scheme_row_lacks.len();
            let mut lacks = parameter
                .lacks
                .iter()
                .copied()
                .collect::<Vec<_>>();
            lacks.sort_by(|left, right| left.as_str().as_str().cmp(right.as_str().as_str()));
            for name in lacks {
                let name = self.intern_string(name.as_str().as_str())?.raw();
                self.store.scheme_row_lacks.push(name);
            }
            self.store.scheme_row_parameters.push(FullSchemeRowParameter {
                id: parameter.id.0,
                binder: u32::try_from(binder).map_err(|_| {
                    IrBuildError::format("scheme_row_binder_overflow", None, 0, 0)
                })?,
                lacks: super::table_range(lacks_start, self.store.scheme_row_lacks.len())?,
            });
        }
        let row_parameters_range = super::table_range(
            row_parameters_start,
            self.store.scheme_row_parameters.len(),
        )?;

        let params_start = self.store.scheme_params.len();
        for parameter in scheme.params() {
            let ty = self.intern_scheme_type(&parameter.ty, &type_parameters, &row_parameters, &effect_parameters)?;
            let schema = parameter
                .schema_expectation
                .as_ref()
                .map(|expectation| self.intern_scheme_schema(expectation))
                .transpose()?;
            self.store
                .scheme_params
                .push(FullSchemeParam { ty, schema });
        }
        let params = super::table_range(params_start, self.store.scheme_params.len())?;
        let closed_return = scheme.return_type().closed_ground_type().map(SchemeType::Ground);
        let return_type = self.intern_scheme_type(
            closed_return.as_ref().unwrap_or_else(|| scheme.return_type()),
            &type_parameters,
            &row_parameters,
            &effect_parameters,
        )?;
        let return_schema = scheme
            .return_schema()
            .map(|expectation| self.intern_scheme_schema(expectation))
            .transpose()?;
        let effects = self.intern_effect_contract(scheme.effects().contract(), &effect_parameters)?;
        let effect_predicates_start = self.store.scheme_effect_predicates.len();
        for predicate in &scheme.callable().requirements().effect_predicates {
            let predicate = self.intern_effect_predicate(predicate, &effect_parameters)?;
            self.store.scheme_effect_predicates.push(predicate);
        }
        let effect_predicates = super::table_range(effect_predicates_start, self.store.scheme_effect_predicates.len())?;
        let mut applications = Vec::new();
        for application in &scheme.callable().requirements().applications {
            let callee = self.intern_scheme_type(&application.callee, &type_parameters, &row_parameters, &effect_parameters)?;
            let result = self.intern_scheme_type(&application.result, &type_parameters, &row_parameters, &effect_parameters)?;
            let effects = self.intern_effect_contract(&application.effects, &effect_parameters)?;
            let mut arguments = Vec::new();
            for argument in application.arguments.iter() {
                let ty = self.intern_scheme_type(&argument.ty, &type_parameters, &row_parameters, &effect_parameters)?;
                let kind = match argument.kind {
                    crate::sema::arguments::StaticArgumentKind::Positional => FullApplicationArgumentKind::Positional,
                    crate::sema::arguments::StaticArgumentKind::Named(name) => FullApplicationArgumentKind::Named(self.intern_string(name.as_str().as_str())?.raw()),
                    crate::sema::arguments::StaticArgumentKind::Splice => FullApplicationArgumentKind::Splice,
                };
                arguments.push(FullApplicationArgument { ty, kind });
            }
            let arguments_start = self.store.scheme_application_arguments.len();
            self.store.scheme_application_arguments.extend(arguments);
            let arguments = super::table_range(arguments_start, self.store.scheme_application_arguments.len())?;
            let mut proc_returns = Vec::new();
            for envelope in application.proc_returns.iter() {
                let result = self.intern_scheme_type(&envelope.result, &type_parameters, &row_parameters, &effect_parameters)?;
                let mut errors = Vec::new();
                for error in envelope.propagated_errors.iter() {
                    errors.push(self.intern_scheme_type(error, &type_parameters, &row_parameters, &effect_parameters)?);
                }
                let errors_start = self.store.scheme_children.len();
                self.store.scheme_children.extend(errors);
                proc_returns.push(FullProcReturnEnvelope { result,
                    propagated_errors: super::table_range(errors_start, self.store.scheme_children.len())? });
            }
            let returns_start = self.store.scheme_proc_returns.len();
            self.store.scheme_proc_returns.extend(proc_returns);
            applications.push(FullSchemeApplication { callee, result, effects, arguments,
                proc_returns: super::table_range(returns_start, self.store.scheme_proc_returns.len())?, kind: application.kind });
        }
        let applications_start = self.store.scheme_applications.len();
        self.store.scheme_applications.extend(applications);
        let applications = super::table_range(applications_start, self.store.scheme_applications.len())?;
        let operations_start = self.store.scheme_operations.len();
        for operation in &scheme.callable().requirements().operations {
            let operation = operation.try_map(|ty| self.intern_scheme_type(ty, &type_parameters, &row_parameters, &effect_parameters))?;
            let encoded = match operation {
                crate::sema::sealed_operation::SealedOperation::Binary(operation) => FullSealedOperation::Binary { left: operation.left, right: operation.right, result: operation.result, operator: operation.operator },
                crate::sema::sealed_operation::SealedOperation::Index { receiver, key, result, syntax, constant_key, .. } => FullSealedOperation::Index { receiver, key, result, syntax, constant_key: constant_key.map(|name| self.intern_string(name.as_str().as_str()).map(|id| id.raw())).transpose()?.unwrap_or(super::IR_NONE) },
                crate::sema::sealed_operation::SealedOperation::Slice { receiver, result, bounds, .. } => FullSealedOperation::Slice { receiver, result, bounds },
                crate::sema::sealed_operation::SealedOperation::RecordOverride { receiver, overrides, result, .. } => {
                    let mut fields = Vec::with_capacity(overrides.len());
                    for field in overrides.iter() { fields.push(FullSchemeField { name: self.intern_string(field.name.as_str().as_str())?.raw(), ty: field.ty }); }
                    let start = self.store.scheme_fields.len();
                    self.store.scheme_fields.extend(fields);
                    FullSealedOperation::RecordOverride { receiver, result, overrides: super::table_range(start, self.store.scheme_fields.len())? }
                }
            };
            self.store.scheme_operations.push(encoded);
        }
        let operations = super::table_range(operations_start, self.store.scheme_operations.len())?;
        let methods_start = self.store.scheme_methods.len();
        for method in &scheme.callable().requirements().methods {
            let receiver = self.intern_scheme_type(&method.receiver, &type_parameters, &row_parameters, &effect_parameters)?;
            let result = self.intern_scheme_type(&method.result, &type_parameters, &row_parameters, &effect_parameters)?;
            let argument_slots_start = self.store.scheme_method_slots.len();
            self.store.scheme_method_slots.extend(method.argument_slots.iter().copied());
            let argument_slots = super::table_range(argument_slots_start, self.store.scheme_method_slots.len())?;
            let arguments_start = self.store.scheme_method_arguments.len();
            for argument in &method.arguments {
                let argument = self.intern_scheme_type(argument, &type_parameters, &row_parameters, &effect_parameters)?;
                self.store.scheme_method_arguments.push(argument);
            }
            let arguments = super::table_range(arguments_start, self.store.scheme_method_arguments.len())?;
            self.store.scheme_methods.push(FullSchemeMethod {
                family: method.family,
                reserved: [0; 3],
                receiver,
                argument_slots,
                arguments,
                result,
            });
        }
        let methods = super::table_range(methods_start, self.store.scheme_methods.len())?;
        let wrappers_start = self.store.scheme_wrappers.len();
        for (ordinal, requirement) in scheme.callable().requirements().wrappers.iter().enumerate() {
            let ordinal = u32::try_from(ordinal).map_err(|_| IrBuildError::format("wrapper_group_overflow", None, 0, ordinal))?;
            let encoded = self.intern_wrapper(requirement, ordinal, &type_parameters, &row_parameters, &effect_parameters)?;
            self.store.scheme_wrappers.push(encoded);
        }
        let wrappers = super::table_range(wrappers_start, self.store.scheme_wrappers.len())?;
        let callable_kind = scheme.kind();
        let id = FullSchemeId::new(self.store.schemes.len())?;
        self.store.schemes.push(FullScheme {
            type_parameters: type_parameters_range,
            row_parameters: row_parameters_range,
            effect_parameters: effect_parameters_range,
            params,
            return_type,
            return_schema,
            effects,
            effect_predicates,
            applications, operations, methods, wrappers,
            body_facts: IrRange::default(),
            callable_kind,
            explicit_return: scheme.explicit_return(),
            inferred_effects: scheme.inferred_effects(),
            reserved: [0; 3],
        });
        Ok(id)
    }

    pub(in crate::runtime::eval) fn record_call_instantiation(
        &mut self,
        instruction: usize,
        instantiation: &CallInstantiation,
    ) -> Result<(), IrBuildError> {
        if self
            .store
            .call_instantiations
            .last()
            .is_some_and(|previous| previous.instruction as usize >= instruction)
        {
            return Err(IrBuildError::format(
                "scheme_call_instantiation_order",
                None,
                0,
                instruction,
            ));
        }
        let arguments = self.encode_instantiation(instantiation)?;
        self.store.call_instantiations.push(FullCallInstantiation {
            instruction: u32::try_from(instruction)
                .map_err(|_| IrBuildError::format("scheme_call_instruction", None, 0, instruction))?,
            arguments,
        });
        Ok(())
    }

    pub(super) fn encode_instantiation(&mut self, instantiation: &CallInstantiation) -> Result<FullInstantiation, IrBuildError> {
        let type_parameters = self
            .active_scheme
            .map(|scheme| self.scheme_type_parameter_map(scheme))
            .transpose()?
            .unwrap_or_default();
        let row_parameters = self
            .active_scheme
            .map(|scheme| self.scheme_row_parameter_map(scheme))
            .transpose()?
            .unwrap_or_default();
        let effect_parameters = self.active_scheme.map(|scheme| self.scheme_effect_parameter_map(scheme))
            .transpose()?.unwrap_or_default();
        let type_arguments_start = self.store.call_type_arguments.len();
        for (binder, ty) in instantiation.type_arguments() {
            let ty = self.intern_scheme_type(ty, &type_parameters, &row_parameters, &effect_parameters)?;
            self.store.call_type_arguments.push(FullTypeArgument {
                binder: binder.0,
                ty,
            });
        }
        let type_arguments = super::table_range(
            type_arguments_start,
            self.store.call_type_arguments.len(),
        )?;
        let row_arguments_start = self.store.call_row_arguments.len();
        for (binder, evidence) in instantiation.row_arguments() {
            let fields = self.intern_row_evidence(evidence, &type_parameters, &row_parameters, &effect_parameters)?;
            let tail = match evidence.tail() {
                SchemeRowTail::Closed => IR_NONE,
                SchemeRowTail::Parameter(id) => *row_parameters.get(&id).ok_or_else(|| {
                    IrBuildError::format(
                        "scheme_call_row_tail_out_of_scope",
                        None,
                        0,
                        id.0 as usize,
                    )
                })?,
            };
            self.store.call_row_arguments.push(FullRowArgument {
                binder: binder.0,
                fields,
                tail,
            });
        }
        let row_arguments = super::table_range(
            row_arguments_start,
            self.store.call_row_arguments.len(),
        )?;
        let effect_arguments_start = self.store.call_effect_arguments.len();
        for (binder, expression) in instantiation.effect_arguments() {
            let expression = self.intern_effect_contract(expression, &effect_parameters)?;
            self.store.call_effect_arguments.push(FullEffectArgument { binder: binder.0, expression });
        }
        let effect_arguments = super::table_range(effect_arguments_start, self.store.call_effect_arguments.len())?;
        let bindings_start = self.store.call_application_bindings.len();
        for binding in instantiation.application_bindings() {
            let binding = match binding {
                crate::sema::schemes::CallableApplicationBinding::Selected(binding) => {
                    let arguments_start = self.store.call_application_slots.len();
                    for &slot in &binding.argument_slots {
                        self.store.call_application_slots.push(u32::try_from(slot).map_err(|_| IrBuildError::format("application_slot_overflow", None, 0, slot))?);
                    }
                    let argument_slots = super::table_range(arguments_start, self.store.call_application_slots.len())?;
                    let omitted_start = self.store.call_application_slots.len();
                    for &slot in &binding.omitted_slots {
                        self.store.call_application_slots.push(u32::try_from(slot).map_err(|_| IrBuildError::format("application_slot_overflow", None, 0, slot))?);
                    }
                    FullApplicationBinding::Selected { argument_slots,
                        omitted_slots: super::table_range(omitted_start, self.store.call_application_slots.len())? }
                }
                crate::sema::schemes::CallableApplicationBinding::Forwarded(ordinal) => FullApplicationBinding::Forwarded(*ordinal),
            };
            self.store.call_application_bindings.push(binding);
        }
        let application_bindings = super::table_range(bindings_start, self.store.call_application_bindings.len())?;
        let operations_start = self.store.call_operation_bindings.len();
        self.store.call_operation_bindings.extend(instantiation.operation_bindings().iter().copied());
        let operation_bindings = super::table_range(operations_start, self.store.call_operation_bindings.len())?;
        let method_bindings_start = self.store.call_method_bindings.len();
        self.store.call_method_bindings.extend(instantiation.method_bindings().iter().copied());
        let method_bindings = super::table_range(method_bindings_start, self.store.call_method_bindings.len())?;
        let wrappers_start = self.store.call_wrapper_bindings.len();
        self.store.call_wrapper_bindings.extend(instantiation.wrapper_bindings().iter().copied());
        let wrapper_bindings = super::table_range(wrappers_start, self.store.call_wrapper_bindings.len())?;
        Ok(FullInstantiation {
            type_arguments, row_arguments, effect_arguments,
            application_bindings, operation_bindings, method_bindings, wrapper_bindings,
        })
    }

    pub(super) fn record_scheme_body_fact(
        &mut self,
        function: IrFunctionId,
        scheme: FullSchemeId,
        instruction: usize,
        ty: &SchemeType,
        kind: FullSchemeBodyFactKind,
    ) -> Result<(), IrBuildError> {
        let type_parameters = self.scheme_type_parameter_map(scheme)?;
        let row_parameters = self.scheme_row_parameter_map(scheme)?;
        let effect_parameters = self.scheme_effect_parameter_map(scheme)?;
        let ty = self.intern_scheme_type(ty, &type_parameters, &row_parameters, &effect_parameters)?;
        self.store.scheme_body_facts.push(FullSchemeBodyFact {
            instruction: u32::try_from(instruction)
                .map_err(|_| IrBuildError::format("scheme_body_instruction", None, 0, instruction))?,
            function,
            value: match kind {
                FullSchemeBodyFactKind::Expression => FullSchemeBodyFactValue::Expression(ty),
                FullSchemeBodyFactKind::Tail => FullSchemeBodyFactValue::Tail(ty),
                FullSchemeBodyFactKind::LocalImmutable => FullSchemeBodyFactValue::LocalImmutable(ty),
                FullSchemeBodyFactKind::LocalMutable => FullSchemeBodyFactValue::LocalMutable(ty),
                FullSchemeBodyFactKind::EmptyList | FullSchemeBodyFactKind::EmptyMap | FullSchemeBodyFactKind::RecordProducer | FullSchemeBodyFactKind::Operation | FullSchemeBodyFactKind::Method | FullSchemeBodyFactKind::Wrapper | FullSchemeBodyFactKind::CompletionSource => return Err(IrBuildError::format("typed_constructor_fact", None, 0, instruction)),
            },
        });
        Ok(())
    }

    pub(super) fn record_scheme_constructor_fact(&mut self, function: IrFunctionId, instruction: usize, kind: crate::sema::schemes::NonmaterialConstructor) -> Result<(), IrBuildError> {
        let value = match kind {
            crate::sema::schemes::NonmaterialConstructor::EmptyList => FullSchemeBodyFactValue::EmptyList,
            crate::sema::schemes::NonmaterialConstructor::EmptyMap => FullSchemeBodyFactValue::EmptyMap,
            crate::sema::schemes::NonmaterialConstructor::Record => FullSchemeBodyFactValue::RecordProducer,
        };
        self.store.scheme_body_facts.push(FullSchemeBodyFact {
            instruction: u32::try_from(instruction).map_err(|_| IrBuildError::format("constructor_fact_instruction", None, 0, instruction))?, function, value,
        });
        Ok(())
    }

    pub(super) fn finish_scheme_body_facts(
        &mut self,
        scheme: FullSchemeId,
        facts_start: usize,
    ) -> Result<(), IrBuildError> {
        let facts = self
            .store
            .scheme_body_facts
            .get_mut(facts_start..)
            .ok_or_else(|| IrBuildError::format("scheme_body_fact_range", None, 0, facts_start))?;
        facts.sort_by_key(|fact| (fact.instruction, fact.kind() as u8));
        let range = super::table_range(facts_start, self.store.scheme_body_facts.len())?;
        let stored = self
            .store
            .schemes
            .get_mut(scheme.index())
            .ok_or_else(|| IrBuildError::format("scheme_id", None, 0, scheme.index()))?;
        if stored.body_facts != IrRange::default() {
            return Err(IrBuildError::format(
                "scheme_body_facts_duplicate",
                None,
                0,
                scheme.index(),
            ));
        }
        stored.body_facts = range;
        Ok(())
    }

    pub(in crate::runtime::eval) fn record_checked_call_result(
        &mut self,
        instruction: usize,
        ty: &Type,
    ) -> Result<(), IrBuildError> {
        let type_id = self.intern_scheme_ground_type(ty)?;
        let instruction = u32::try_from(instruction).map_err(|_| {
            IrBuildError::format("scheme_checked_call_instruction", None, 0, instruction)
        })?;
        if let Some(previous) = self.store.checked_expr_types.last() {
            if previous.instruction == instruction {
                if previous.type_id != type_id {
                    return Err(IrBuildError::format(
                        "scheme_checked_call_result_conflict",
                        None,
                        0,
                        instruction as usize,
                    ));
                }
                return Ok(());
            }
            if previous.instruction > instruction {
                return Err(IrBuildError::format(
                    "scheme_checked_call_result_order",
                    None,
                    0,
                    instruction as usize,
                ));
            }
        }
        self.store.checked_expr_types.push(super::FullCheckedExprType {
            instruction,
            type_id,
            purpose: super::FullCheckedExprPurpose::CheckedContract { function_ref: None, with_error_source: false },
        });
        Ok(())
    }

    fn intern_wrapper(
        &mut self,
        requirement: &crate::sema::wrapper_operation::WrapperRequirement<SchemeType, SchemeEffectParameterId>,
        ordinal: u32,
        types: &BTreeMap<SchemeTypeParameterId, u32>, rows: &BTreeMap<SchemeRowParameterId, u32>, effects: &BTreeMap<SchemeEffectParameterId, u32>,
    ) -> Result<FullWrapper, IrBuildError> {
        use crate::sema::wrapper_operation::{WrapperRequirementKind as K, WrapperBoundary as B, WrapperErrorContribution as C, WrapperCompletionContribution as PC, WrapperCompletionContributions as CC, WrapperGroupRef};
        let index = |value| u32::try_from(value).map_err(|_| IrBuildError::format("wrapper_detail_overflow", None, 0, value));
        Ok(match &requirement.kind {
            K::Field { source, result, name, boundary, effects: contract } => {
                let source = self.intern_scheme_type(source, types, rows, effects)?;
                let result = self.intern_scheme_type(result, types, rows, effects)?;
                let name = self.intern_string(name.as_str().as_str())?.raw();
                let boundary = match boundary {
                    B::Captured => FullWrapperBoundary::Captured,
                    B::Return(ty) => FullWrapperBoundary::Return(self.intern_scheme_type(ty, types, rows, effects)?),
                    B::ErrorReturn(ty) => FullWrapperBoundary::ErrorReturn(self.intern_scheme_type(ty, types, rows, effects)?),
                    B::ProcCompletion { returned, group: WrapperGroupRef::Ordinal(group) } => FullWrapperBoundary::ProcCompletion { returned: self.intern_scheme_type(returned, types, rows, effects)?, group: *group },
                    B::ProcCompletion { .. } => return Err(IrBuildError::format("wrapper_group_scope", None, 0, ordinal as usize)),
                    B::Producer => FullWrapperBoundary::Producer,
                    B::TopLevel => FullWrapperBoundary::TopLevel,
                };
                let effects = self.intern_effect_contract(contract, effects)?;
                let detail = index(self.store.wrapper_fields.len())?;
                self.store.wrapper_fields.push(FullWrapperField { name, boundary, effects });
                FullWrapper::Field { source, result, detail }
            }
            K::Fallback { source, result, fallback } => FullWrapper::Fallback {
                source: self.intern_scheme_type(source, types, rows, effects)?, result: self.intern_scheme_type(result, types, rows, effects)?, backup: self.intern_scheme_type(fallback, types, rows, effects)?,
            },
            K::Handler { source, result, error, handler } => {
                let source = self.intern_scheme_type(source, types, rows, effects)?;
                let result = self.intern_scheme_type(result, types, rows, effects)?;
                let error = self.intern_scheme_type(error, types, rows, effects)?;
                let handler = self.intern_scheme_type(handler, types, rows, effects)?;
                let detail = index(self.store.wrapper_handlers.len())?;
                self.store.wrapper_handlers.push(FullWrapperHandler { error, handler });
                FullWrapper::Handler { source, result, detail }
            }
            K::Capture { contributions, result, expected } => {
                let result = self.intern_scheme_type(result, types, rows, effects)?;
                let mut encoded = Vec::with_capacity(contributions.len());
                for term in contributions.iter() {
                    encoded.push(match term {
                        C::Explicit(ty) => FullWrapperContribution::Explicit(self.intern_scheme_type(ty, types, rows, effects)?),
                        C::Source(ty) => FullWrapperContribution::Source(self.intern_scheme_type(ty, types, rows, effects)?),
                    });
                }
                let start = self.store.wrapper_contributions.len();
                self.store.wrapper_contributions.extend(encoded);
                let contributions = super::table_range(start, self.store.wrapper_contributions.len())?;
                let detail = index(self.store.wrapper_captures.len())?;
                self.store.wrapper_captures.push(FullWrapperCapture { contributions, expected: *expected, reserved: [0; 3] });
                FullWrapper::Capture { result, detail }
            }
            K::ProcCompletion { group: WrapperGroupRef::Ordinal(group), returned, contributions: CC::Ready(contributions) } => {
                if *group != ordinal { return Err(IrBuildError::format("wrapper_group_scope", None, 0, ordinal as usize)); }
                let returned = self.intern_scheme_type(returned, types, rows, effects)?;
                let mut encoded = Vec::with_capacity(contributions.len());
                for term in contributions.iter() {
                    encoded.push(match term {
                        PC::PropagatedError(ty) => FullWrapperContribution::PropagatedError(self.intern_scheme_type(ty, types, rows, effects)?),
                        PC::ReturnedResult(ty) => FullWrapperContribution::ReturnedResult(self.intern_scheme_type(ty, types, rows, effects)?),
                        PC::FieldSource(ty) => FullWrapperContribution::FieldSource(self.intern_scheme_type(ty, types, rows, effects)?),
                        PC::ApplicationSource(ty) => FullWrapperContribution::ApplicationSource(self.intern_scheme_type(ty, types, rows, effects)?),
                    });
                }
                let start = self.store.wrapper_contributions.len();
                self.store.wrapper_contributions.extend(encoded);
                let contributions = super::table_range(start, self.store.wrapper_contributions.len())?;
                let detail = index(self.store.wrapper_completions.len())?;
                self.store.wrapper_completions.push(FullWrapperCompletion { contributions });
                FullWrapper::ProcCompletion { returned, group: *group, detail }
            }
            K::ProcCompletion { .. } => return Err(IrBuildError::format("wrapper_group_scope", None, 0, ordinal as usize)),
        })
    }

    pub(super) fn intern_scheme_type(
        &mut self,
        ty: &SchemeType,
        type_parameters: &BTreeMap<SchemeTypeParameterId, u32>,
        row_parameters: &BTreeMap<SchemeRowParameterId, u32>,
        effect_parameters: &BTreeMap<SchemeEffectParameterId, u32>,
    ) -> Result<SchemeTypeId, IrBuildError> {
        let node = match ty {
            SchemeType::SchemeCallable(source) => FullSchemeTypeNode { first: self.intern_scheme_callable_value(*source)?.0, second: 0, children: IrRange::default(), tag: FullSchemeTypeTag::SchemeCallable, reserved: [0; 3] },
            SchemeType::Callable { kind, monotype, effects } => {
                let mut parameters = Vec::with_capacity(monotype.params().len());
                for parameter in monotype.params() {
                    parameters.push(FullSchemeCallableParameter {
                        name: self.intern_string(parameter.name.as_str().as_str())?.raw(),
                        ty: self.intern_scheme_type(&parameter.ty, type_parameters, row_parameters, effect_parameters)?,
                        schema: parameter.schema_expectation.as_ref().map(|schema| self.intern_scheme_schema(schema)).transpose()?,
                        flags: u8::from(parameter.rest) | u8::from(parameter.defaulted) << 1,
                        reserved: [0; 3],
                    });
                }
                let params_start = self.store.scheme_callable_parameters.len();
                self.store.scheme_callable_parameters.extend(parameters);
                let params = super::table_range(params_start, self.store.scheme_callable_parameters.len())?;
                let return_type = self.intern_scheme_type(monotype.return_type(), type_parameters, row_parameters, effect_parameters)?;
                let return_schema = monotype.return_schema().map(|schema| self.intern_scheme_schema(schema)).transpose()?;
                let effects = self.intern_effect_contract(effects, effect_parameters)?;
                let index = u32::try_from(self.store.scheme_callable_monotypes.len()).ok().filter(|index| *index != IR_NONE)
                    .ok_or_else(|| IrBuildError::format("scheme_callable_monotype_overflow", None, 0, 0))?;
                self.store.scheme_callable_monotypes.push(FullSchemeCallableMonotype {
                    params,
                    return_type,
                    return_schema,
                    effects,
                    kind: *kind,
                    reserved: [0; 3],
                });
                FullSchemeTypeNode { first: index, second: 0, children: IrRange::default(), tag: FullSchemeTypeTag::Callable, reserved: [0; 3] }
            }
            SchemeType::Ground(ty) => FullSchemeTypeNode {
                first: self.intern_scheme_ground_type(ty)?.raw(),
                second: 0,
                children: IrRange::default(),
                tag: FullSchemeTypeTag::Ground,
                reserved: [0; 3],
            },
            SchemeType::Parameter(parameter) => FullSchemeTypeNode {
                first: *type_parameters.get(parameter).ok_or_else(|| {
                    IrBuildError::format("scheme_type_parameter_scope", None, 0, 0)
                })?,
                second: 0,
                children: IrRange::default(),
                tag: FullSchemeTypeTag::Parameter,
                reserved: [0; 3],
            },
            SchemeType::List(inner) => self.unary_scheme_node(
                inner,
                type_parameters,
                row_parameters,
                effect_parameters,
                FullSchemeTypeTag::List,
            )?,
            SchemeType::Stream(inner) => self.unary_scheme_node(
                inner,
                type_parameters,
                row_parameters,
                effect_parameters,
                FullSchemeTypeTag::Stream,
            )?,
            SchemeType::Set(inner) => self.unary_scheme_node(
                inner,
                type_parameters,
                row_parameters,
                effect_parameters,
                FullSchemeTypeTag::Set,
            )?,
            SchemeType::Optional(inner) => self.unary_scheme_node(
                inner,
                type_parameters,
                row_parameters,
                effect_parameters,
                FullSchemeTypeTag::Optional,
            )?,
            SchemeType::Map(key, value) => self.binary_scheme_node(
                key,
                value,
                type_parameters,
                row_parameters,
                effect_parameters,
                FullSchemeTypeTag::Map,
            )?,
            SchemeType::Result(ok, error) => self.binary_scheme_node(
                ok,
                error,
                type_parameters,
                row_parameters,
                effect_parameters,
                FullSchemeTypeTag::Result,
            )?,
            SchemeType::Record { fields, tail } => {
                let mut encoded_fields = Vec::with_capacity(fields.len());
                for (name, field_type) in fields {
                    let ty = self.intern_scheme_type(
                        field_type,
                        type_parameters,
                        row_parameters,
                        effect_parameters,
                    )?;
                    encoded_fields.push(FullSchemeField {
                        name: self.intern_string(name.as_str().as_str())?.raw(),
                        ty,
                    });
                }
                let fields_start = self.store.scheme_fields.len();
                self.store.scheme_fields.extend(encoded_fields);
                let tail = match tail {
                    SchemeRowTail::Closed => IR_NONE,
                    SchemeRowTail::Parameter(parameter) => {
                        *row_parameters.get(parameter).ok_or_else(|| {
                            IrBuildError::format("scheme_row_parameter_scope", None, 0, 0)
                        })?
                    }
                };
                FullSchemeTypeNode {
                    first: tail,
                    second: 0,
                    children: super::table_range(fields_start, self.store.scheme_fields.len())?,
                    tag: FullSchemeTypeTag::Record,
                    reserved: [0; 3],
                }
            }
            SchemeType::Union(members) => {
                let mut children = Vec::with_capacity(members.len());
                for member in members {
                    children.push(self.intern_scheme_type(
                        member,
                        type_parameters,
                        row_parameters,
                        effect_parameters,
                    )?);
                }
                let children_start = self.store.scheme_children.len();
                self.store.scheme_children.extend(children);
                FullSchemeTypeNode {
                    first: 0,
                    second: 0,
                    children: super::table_range(
                        children_start,
                        self.store.scheme_children.len(),
                    )?,
                    tag: FullSchemeTypeTag::Union,
                    reserved: [0; 3],
                }
            }
            SchemeType::Validated(validation, inner) => {
                let ty = self.intern_scheme_type(inner, type_parameters, row_parameters, effect_parameters)?;
                let validation = self.intern_scheme_validation(*validation)?;
                FullSchemeTypeNode {
                    first: ty.raw(),
                    second: validation,
                    children: IrRange::default(),
                    tag: FullSchemeTypeTag::Validated,
                    reserved: [0; 3],
                }
            }
        };
        let id = SchemeTypeId::new(self.store.scheme_types.len())?;
        self.store.scheme_types.push(node);
        Ok(id)
    }

    fn unary_scheme_node(
        &mut self,
        inner: &SchemeType,
        type_parameters: &BTreeMap<SchemeTypeParameterId, u32>,
        row_parameters: &BTreeMap<SchemeRowParameterId, u32>,
        effect_parameters: &BTreeMap<SchemeEffectParameterId, u32>,
        tag: FullSchemeTypeTag,
    ) -> Result<FullSchemeTypeNode, IrBuildError> {
        let inner = self.intern_scheme_type(inner, type_parameters, row_parameters, effect_parameters)?;
        Ok(FullSchemeTypeNode {
            first: inner.raw(),
            second: 0,
            children: IrRange::default(),
            tag,
            reserved: [0; 3],
        })
    }

    fn binary_scheme_node(
        &mut self,
        first: &SchemeType,
        second: &SchemeType,
        type_parameters: &BTreeMap<SchemeTypeParameterId, u32>,
        row_parameters: &BTreeMap<SchemeRowParameterId, u32>,
        effect_parameters: &BTreeMap<SchemeEffectParameterId, u32>,
        tag: FullSchemeTypeTag,
    ) -> Result<FullSchemeTypeNode, IrBuildError> {
        let first = self.intern_scheme_type(first, type_parameters, row_parameters, effect_parameters)?;
        let second = self.intern_scheme_type(second, type_parameters, row_parameters, effect_parameters)?;
        Ok(FullSchemeTypeNode {
            first: first.raw(),
            second: second.raw(),
            children: IrRange::default(),
            tag,
            reserved: [0; 3],
        })
    }

    fn intern_scheme_ground_type(&mut self, ty: &Type) -> Result<TypeId, IrBuildError> {
        if ty.contains_inference() || ty.contains_recovery() {
            return Err(IrBuildError::format(
                "scheme_ground_type_not_closed",
                None,
                0,
                self.store.tags.len(),
            ));
        }
        self.intern_checked_type(&super::executable_type(ty))
    }

    fn intern_scheme_validation(&mut self, validation: Validation) -> Result<u32, IrBuildError> {
        let row = match validation {
            Validation::NonEmpty => FullSchemeValidation {
                name: IR_NONE,
                low: 0,
                high: 0,
                tag: FullSchemeValidationTag::NonEmpty,
                reserved: [0; 7],
            },
            Validation::RelPath => FullSchemeValidation {
                name: IR_NONE,
                low: 0,
                high: 0,
                tag: FullSchemeValidationTag::RelPath,
                reserved: [0; 7],
            },
            Validation::Nominal(name) => FullSchemeValidation {
                name: self.intern_string(name.as_str().as_str())?.raw(),
                low: 0,
                high: 0,
                tag: FullSchemeValidationTag::Nominal,
                reserved: [0; 7],
            },
            Validation::Range(range) => FullSchemeValidation {
                name: IR_NONE,
                low: range.low(),
                high: range.high(),
                tag: FullSchemeValidationTag::Range,
                reserved: [0; 7],
            },
        };
        let index = u32::try_from(self.store.scheme_validations.len())
            .map_err(|_| IrBuildError::format("scheme_validation_overflow", None, 0, 0))?;
        if index == IR_NONE {
            return Err(IrBuildError::format("scheme_validation_overflow", None, 0, 0));
        }
        self.store.scheme_validations.push(row);
        Ok(index)
    }

    fn intern_scheme_schema(
        &mut self,
        expectation: &SchemaExpectation,
    ) -> Result<FullSchemeSchemaExpectationId, IrBuildError> {
        let id = FullSchemeSchemaExpectationId::new(
            self.store.scheme_schema_expectations.len(),
        )?;
        self.store
            .scheme_schema_expectations
            .push(FullSchemeSchemaExpectation {
                instances: IrRange::default(),
                children: IrRange::default(),
            });
        let instances_start = self.store.scheme_schema_instances.len();
        for instance in &expectation.instances {
            let arguments_start = self.store.scheme_schema_arguments.len();
            for ty in &instance.arguments {
                let ty = self.intern_scheme_ground_type(ty)?;
                self.store.scheme_schema_arguments.push(ty);
            }
            self.store.scheme_schema_instances.push(FullSchemeSchemaInstance {
                definition: u32::try_from(instance.definition.index()).map_err(|_| {
                    IrBuildError::format("scheme_schema_definition", None, 0, 0)
                })?,
                arguments: super::table_range(
                    arguments_start,
                    self.store.scheme_schema_arguments.len(),
                )?,
            });
        }
        let instances = super::table_range(
            instances_start,
            self.store.scheme_schema_instances.len(),
        )?;
        let mut children = Vec::with_capacity(expectation.children.len());
        for (component, child) in expectation.children.iter() {
            let (component, name) = match component {
                SchemaComponent::Field(name) => (
                    FullSchemeSchemaComponentTag::Field,
                    self.intern_string(name.as_str().as_str())?.raw(),
                ),
                SchemaComponent::Item => (FullSchemeSchemaComponentTag::Item, IR_NONE),
                SchemaComponent::Value => (FullSchemeSchemaComponentTag::Value, IR_NONE),
                SchemaComponent::Key => (FullSchemeSchemaComponentTag::Key, IR_NONE),
                SchemaComponent::Optional => (FullSchemeSchemaComponentTag::Optional, IR_NONE),
                SchemaComponent::Success => (FullSchemeSchemaComponentTag::Success, IR_NONE),
                SchemaComponent::Error => (FullSchemeSchemaComponentTag::Error, IR_NONE),
            };
            children.push(FullSchemeSchemaChild {
                component,
                reserved: [0; 3],
                name,
                expectation: self.intern_scheme_schema(child)?,
            });
        }
        let children_start = self.store.scheme_schema_children.len();
        self.store.scheme_schema_children.extend(children);
        self.store.scheme_schema_expectations[id.index()] = FullSchemeSchemaExpectation {
            instances,
            children: super::table_range(
                children_start,
                self.store.scheme_schema_children.len(),
            )?,
        };
        Ok(id)
    }

    pub(super) fn scheme_type_parameter_map(
        &self,
        scheme: FullSchemeId,
    ) -> Result<BTreeMap<SchemeTypeParameterId, u32>, IrBuildError> {
        let record = self
            .store
            .schemes
            .get(scheme.index())
            .ok_or_else(|| IrBuildError::format("scheme_id", None, 0, 0))?;
        let rows = record
            .type_parameters
            .bounds(self.store.scheme_type_parameters.len())
            .ok_or_else(|| IrBuildError::format("scheme_type_parameters", None, 0, 0))?;
        Ok(self.store.scheme_type_parameters[rows]
            .iter()
            .map(|parameter| (SchemeTypeParameterId(parameter.id), parameter.binder))
            .collect())
    }

    pub(super) fn scheme_row_parameter_map(
        &self,
        scheme: FullSchemeId,
    ) -> Result<BTreeMap<SchemeRowParameterId, u32>, IrBuildError> {
        let record = self
            .store
            .schemes
            .get(scheme.index())
            .ok_or_else(|| IrBuildError::format("scheme_id", None, 0, 0))?;
        let rows = record
            .row_parameters
            .bounds(self.store.scheme_row_parameters.len())
            .ok_or_else(|| IrBuildError::format("scheme_row_parameters", None, 0, 0))?;
        Ok(self.store.scheme_row_parameters[rows]
            .iter()
            .map(|parameter| (SchemeRowParameterId(parameter.id), parameter.binder))
            .collect())
    }

    fn intern_row_evidence(
        &mut self,
        evidence: &RowEvidence,
        type_parameters: &BTreeMap<SchemeTypeParameterId, u32>,
        row_parameters: &BTreeMap<SchemeRowParameterId, u32>,
        effect_parameters: &BTreeMap<SchemeEffectParameterId, u32>,
    ) -> Result<IrRange, IrBuildError> {
        let start = self.store.call_row_fields.len();
        let mut fields = evidence.fields().iter().collect::<Vec<_>>();
        fields.sort_by(|(left, _), (right, _)| {
            left.as_str().as_str().cmp(right.as_str().as_str())
        });
        for (name, ty) in fields {
            let name = self.intern_string(name.as_str().as_str())?.raw();
            let ty = self.intern_scheme_type(ty, type_parameters, row_parameters, effect_parameters)?;
            self.store.call_row_fields.push(FullRowEvidenceField {
                name,
                ty,
            });
        }
        super::table_range(start, self.store.call_row_fields.len())
    }
}

#[cfg(test)]
mod tests {
    use super::super::FullCodec;
    use super::{FullSchemeBodyFactKind, FullSchemeTypeTag, SchemeTypeId};
    use crate::sema::schemes::{
        CallInstantiation, RowEvidence, SchemeParameter, SchemeRowParameter,
        SchemeRowParameterId, SchemeRowTail, SchemeType, SchemeTypeParameterId, TypeScheme,
    };
    use crate::sema::types::Type;
    use crate::symbol::Name;
    use crate::syntax::node::Effect;
    use std::collections::{BTreeMap, BTreeSet};
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn factless_pattern_id_encodes_without_an_instruction_tag() {
        let scratch = Rc::new(RefCell::new(super::super::BuildScratch::default()));
        let pattern = scratch
            .borrow_mut()
            .pattern(super::super::BuildPatternRow::Wildcard);
        let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
        builder.active_scratch = Some(scratch);
        let mut output = Vec::new();

        pattern.encode(&mut builder, &mut output).unwrap();

        assert_eq!(output, [0]);
        assert!(builder.store.tags.is_empty());
        assert_eq!(builder.store.patterns, [super::super::FullPatternTag::Wildcard]);
        assert_eq!(builder.store.pattern_data.len(), 1);
    }

    #[test]
    fn build_instruction_resolution_skips_empty_rows_and_keeps_forwarded_ids() {
        assert_eq!(
            super::super::emitted_build_instruction(&[], 0, 0, false).unwrap(),
            None
        );
        assert!(super::super::emitted_build_instruction(&[], 0, 0, true).is_err());
        assert_eq!(
            super::super::emitted_build_instruction(&[3, 1], 1, 2, true).unwrap(),
            Some(1)
        );
    }

    #[test]
    fn generic_binders_encode_outside_the_ground_type_pool() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let binder = SchemeTypeParameterId(7);
            let scheme = TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                vec![binder],
                Vec::new(),
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Parameter(binder),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(binder),
                None,
                false,
                Some(vec![Effect::Error]),
                false,
            )
            .unwrap();
            let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
            let scheme_id = builder.intern_scheme(&scheme).unwrap();
            let encoded = builder.store.scheme(scheme_id).unwrap();
            let params = encoded
                .params
                .bounds(builder.store.scheme_params.len())
                .unwrap();
            let parameter = builder.store.scheme_params[params.start];
            let param_node = builder
                .store
                .scheme_type(parameter.ty)
                .unwrap();
            let return_node = builder.store.scheme_type(encoded.return_type).unwrap();
            assert_eq!(param_node.tag, FullSchemeTypeTag::Parameter);
            assert_eq!(param_node.first, 0);
            assert_eq!(return_node.tag, FullSchemeTypeTag::Parameter);
            assert_eq!(return_node.first, 0);
            assert_eq!(builder.store.semantic.type_count(), 0);
            assert_eq!(encoded.type_parameters.len, 1);
            assert_eq!(SchemeTypeId::new(0).unwrap().index(), 0);
        });
    }

    #[test]
    fn explicit_any_is_kept_in_ground_scheme_contracts_but_not_substitutions() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let scheme = TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                Vec::new(),
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Ground(Type::Any),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Ground(Type::Any),
                None,
                false,
                None,
                false,
            )
            .unwrap();
            let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
            let scheme_id = builder.intern_scheme(&scheme).unwrap();
            let encoded = builder.store.scheme(scheme_id).unwrap();
            let params = encoded
                .params
                .bounds(builder.store.scheme_params.len())
                .unwrap();
            let parameter = builder.store.scheme_params[params.start];
            let parameter_node = builder.store.scheme_type(parameter.ty).unwrap();
            let return_node = builder.store.scheme_type(encoded.return_type).unwrap();
            assert_eq!(parameter_node.tag, FullSchemeTypeTag::Ground);
            assert_eq!(return_node.tag, FullSchemeTypeTag::Ground);
            assert_eq!(
                builder
                    .store
                    .semantic
                    .to_type(super::TypeId::from_raw(parameter_node.first).unwrap())
                    .unwrap(),
                Type::Any
            );
            assert_eq!(
                builder
                    .store
                    .semantic
                    .to_type(super::TypeId::from_raw(return_node.first).unwrap())
                    .unwrap(),
                Type::Any
            );
        });
    }

    #[test]
    fn row_tail_lacks_and_call_substitutions_use_local_binders() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let type_parameter = SchemeTypeParameterId(7);
            let row_parameter = SchemeRowParameterId(19);
            let mut lacks = BTreeSet::new();
            lacks.insert(Name::intern("required"));
            let scheme = TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                vec![type_parameter],
                vec![SchemeRowParameter {
                    id: row_parameter,
                    lacks,
                }],
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Record {
                        fields: BTreeMap::from([(
                            Name::intern("required"),
                            SchemeType::Parameter(type_parameter),
                        )]),
                        tail: SchemeRowTail::Parameter(row_parameter),
                    },
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(type_parameter),
                None,
                false,
                None,
                false,
            )
            .unwrap();
            let evidence = RowEvidence::new(BTreeMap::from([(
                Name::intern("extra"),
                SchemeType::Ground(Type::Str),
            )]), SchemeRowTail::Closed)
            .unwrap();
            let instantiation = CallInstantiation::new(
                scheme.callable(),
                BTreeMap::from([(type_parameter, SchemeType::Ground(Type::Bool))]),
                BTreeMap::from([(row_parameter, evidence)]), BTreeMap::new(), Vec::new(), Vec::new(), Vec::new(),
                &[],
                &[],
                &[],
             &[], &[], &[], &[], &BTreeMap::new())
            .unwrap();
            let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
            let scheme_id = builder.intern_scheme(&scheme).unwrap();
            builder.record_call_instantiation(4, &instantiation).unwrap();
            let encoded = builder.store.scheme(scheme_id).unwrap();
            let type_params = encoded
                .type_parameters
                .bounds(builder.store.scheme_type_parameters.len())
                .unwrap();
            let row_params = encoded
                .row_parameters
                .bounds(builder.store.scheme_row_parameters.len())
                .unwrap();
            assert_eq!(builder.store.scheme_type_parameters[type_params.start].id, 7);
            assert_eq!(builder.store.scheme_type_parameters[type_params.start].binder, 0);
            assert_eq!(builder.store.scheme_row_parameters[row_params.start].id, 19);
            assert_eq!(builder.store.scheme_row_parameters[row_params.start].binder, 0);
            let params = encoded
                .params
                .bounds(builder.store.scheme_params.len())
                .unwrap();
            let param_type = builder.store.scheme_type(
                builder.store.scheme_params[params.start].ty,
            )
            .unwrap();
            assert_eq!(param_type.tag, FullSchemeTypeTag::Record);
            assert_eq!(param_type.first, 0);
            let fields = param_type
                .children
                .bounds(builder.store.scheme_fields.len())
                .unwrap();
            assert_eq!(
                builder.store.string(builder.store.scheme_fields[fields.start].name).unwrap(),
                "required"
            );
            let lacks = builder.store.scheme_row_parameters[row_params.start]
                .lacks
                .bounds(builder.store.scheme_row_lacks.len())
                .unwrap();
            assert_eq!(
                builder.store.string(builder.store.scheme_row_lacks[lacks.start]).unwrap(),
                "required"
            );
            let call = builder.store.call_instantiations[0];
            assert_eq!(call.instruction, 4);
            let type_args = call
                .arguments.type_arguments
                .bounds(builder.store.call_type_arguments.len())
                .unwrap();
            assert_eq!(builder.store.call_type_arguments[type_args.start].binder, 7);
            assert_eq!(
                builder
                    .store
                    .semantic
                    .to_type(super::super::TypeId::from_raw(
                        builder.store.scheme_type(
                            builder.store.call_type_arguments[type_args.start].ty,
                        ).unwrap().first,
                    ).unwrap())
                    .unwrap(),
                Type::Bool
            );
            let row_args = call
                .arguments.row_arguments
                .bounds(builder.store.call_row_arguments.len())
                .unwrap();
            let row_fields = builder.store.call_row_arguments[row_args.start]
                .fields
                .bounds(builder.store.call_row_fields.len())
                .unwrap();
            assert_eq!(builder.store.call_row_arguments[row_args.start].binder, 19);
            assert_eq!(
                builder
                    .store
                    .string(builder.store.call_row_fields[row_fields.start].name)
                    .unwrap(),
                "extra"
            );
        });
    }

    #[test]
    fn body_facts_keep_final_instruction_and_checker_result_identity() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let binder = SchemeTypeParameterId(3);
            let scheme = TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                vec![binder],
                Vec::new(),
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Parameter(binder),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(binder),
                None,
                false,
                None,
                false,
            )
            .unwrap();
            let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
            let scheme_id = builder.intern_scheme(&scheme).unwrap();
            let function = super::super::IrFunctionId::new(0).unwrap();
            let scheme_type = SchemeType::Parameter(binder);
            builder
                .record_scheme_body_fact(
                    function,
                    scheme_id,
                    17,
                    &scheme_type,
                    FullSchemeBodyFactKind::Tail,
                )
                .unwrap();
            builder
                .record_checked_call_result(21, &Type::Bool)
                .unwrap();
            builder
                .record_checked_call_result(21, &Type::Bool)
                .unwrap();
            builder.finish_scheme_body_facts(scheme_id, 0).unwrap();
            let stored = builder.store.scheme(scheme_id).unwrap();
            let facts = stored
                .body_facts
                .bounds(builder.store.scheme_body_facts.len())
                .unwrap();
            assert_eq!(builder.store.scheme_body_facts[facts.start].instruction, 17);
            assert_eq!(builder.store.scheme_body_facts[facts.start].function, function);
            assert_eq!(
                builder.store.scheme_body_facts[facts.start].kind(),
                FullSchemeBodyFactKind::Tail
            );
            assert_eq!(builder.store.checked_expr_types[0].instruction, 21);
            assert_eq!(builder.store.checked_expr_types.len(), 1);
            assert_eq!(
                builder
                    .store
                    .semantic
                    .to_type(builder.store.checked_expr_types[0].type_id)
                    .unwrap(),
                Type::Bool
            );
        });
    }
}
