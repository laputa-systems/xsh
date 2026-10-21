mod producers;
mod body;
mod effects;
mod applications;
mod references;
mod commands;
pub(super) use effects::ScopedEffectProof;
pub(super) use producers::ProducerContexts;
pub(super) use producers::{GroundProducerIndex, ProducerPrefixContext};

use super::schema::{self, FullSchemeTypeTag, FullSchemeValidationTag};
use super::schemes::{
    FullCallableContract, FullScheme, FullSchemeBodyFactKind, FullSchemeId,
    FullSchemeRowParameter, FullSchemeTypeParameter, FullTypeArgument, SchemeTypeId,
};
use super::{
    driver_owner, FullDecoder, FullFunction, FullStore, FullTag, IrBlockId, IrFunctionId, IrRange,
    IrVerifyError, TypeId, IR_NONE,
};
use super::super::semantic::TypeMaterializationBudget;
use crate::sema::types::{Type, merge_list_literal_item_ty};
use crate::sema::effects::{EffectContract, EffectPredicate};
use crate::sema::schemes::SchemeEffectParameterId;
use crate::sema::schemes::{CallableMonotype, SchemeParameter, SchemeRowParameterId, SchemeRowTail, SchemeType, SchemeTypeParameterId};
use crate::sema::validated::{IntRange, ValidatedType, Validation};
use crate::symbol::Name;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[cfg(test)]
mod scalar_predicate_tests;

const SCHEME_TYPE_WORK_PER_NODE: usize = 8;
const SCHEME_TYPE_WORK_BASE: usize = 128;

pub(super) struct SchemeScope {
    type_parameters: Vec<FullSchemeTypeParameter>,
    row_parameters: Vec<FullSchemeRowParameter>,
    type_binders: BTreeMap<u32, usize>,
    row_binders: BTreeMap<u32, usize>,
    row_lacks: Vec<BTreeSet<Name>>,
    effects: effects::EffectScope,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CallSubstitution {
    pub(super) types: Vec<Type>,
    pub(super) rows: Vec<BTreeMap<Name, Type>>,
    pub(super) effects: Vec<EffectContract<SchemeEffectParameterId>>,
}

#[derive(Clone)]
struct SchemeRowSubstitution {
    fields: BTreeMap<Name, SchemeType>,
    tail: SchemeRowTail,
}

struct SymbolicCallSubstitution {
    types: Vec<SchemeType>,
    rows: Vec<SchemeRowSubstitution>,
    effects: Vec<EffectContract<SchemeEffectParameterId>>,
}

enum SchemeTypeExpectation {
    Encoded(SchemeTypeId),
    Ground(Type),
}

enum CheckedArgumentType {
    Known(Type),
    ListItems(Vec<Type>),
    EmptyList(Option<Type>),
    EmptyMap,
}

#[derive(Clone, Default)]
struct SchemeBinderEvidence {
    types: BTreeSet<usize>,
    rows: BTreeSet<usize>,
}

pub(super) struct SchemeInstantiationBudget {
    remaining: usize,
    semantic_types: TypeMaterializationBudget,
}

impl SchemeInstantiationBudget {
    pub(super) fn new(store: &FullStore) -> Self {
        Self {
            remaining: scheme_type_work_limit(store)
                .saturating_add(store.tags.len().saturating_mul(8))
                .saturating_add(store.extra.len()),
            semantic_types: TypeMaterializationBudget::default(),
        }
    }

    pub(super) fn for_prepared_constants(store: &FullStore, verified_work: usize) -> Self {
        let mut budget = Self::new(store);
        budget.remaining = budget.remaining.saturating_add(verified_work.saturating_mul(SCHEME_TYPE_WORK_PER_NODE));
        budget
    }

    pub(super) fn charge(&mut self, amount: usize) -> Result<(), IrVerifyError> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or_else(|| IrVerifyError::new("generic call type materialization exceeds its work bound"))?;
        Ok(())
    }

    fn charge_clone(&mut self, ty: &Type) -> Result<(), IrVerifyError> {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.charge(1)?;
            match ty {
                Type::List(inner)
                | Type::Stream(inner)
                | Type::Set(inner)
                | Type::Optional(inner) => pending.push(inner),
                Type::Map(key, value) | Type::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                Type::Union(members) => pending.extend(members),
                Type::Validated(validated) => pending.push(validated.base()),
                _ => {}
            }
        }
        Ok(())
    }

    fn charge_scheme_clone(&mut self, ty: &SchemeType) -> Result<(), IrVerifyError> {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.charge(1)?;
            match ty {
                SchemeType::Ground(ty) => self.charge_clone(ty)?,
                SchemeType::Parameter(_) => {}
                SchemeType::Callable { monotype, effects, .. } => {
                    if let crate::sema::effects::EffectContract::Known(expression) = effects { self.charge(expression.variables().count())?; }
                    pending.push(monotype.return_type());
                    pending.extend(monotype.params().iter().map(|parameter| &parameter.ty));
                }
                SchemeType::List(inner)
                | SchemeType::Stream(inner)
                | SchemeType::Set(inner)
                | SchemeType::Optional(inner)
                | SchemeType::Validated(_, inner) => pending.push(inner),
                SchemeType::Map(key, value) | SchemeType::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                SchemeType::Record { fields, .. } => pending.extend(fields.values()),
                SchemeType::Union(members) => pending.extend(members),
            }
        }
        Ok(())
    }
}

struct SchemaCoverage {
    expectations: Vec<bool>,
    instances: Vec<bool>,
    arguments: Vec<bool>,
    children: Vec<bool>,
}

struct VerifiedInvocationEffects {
    contract: EffectContract<SchemeEffectParameterId>,
    predicates: Vec<EffectPredicate<SchemeEffectParameterId>>,
}

pub(super) struct VerifiedSchemeScopes {
    scopes: Vec<Option<SchemeScope>>,
    invocations: Vec<Option<VerifiedInvocationEffects>>,
}

impl VerifiedSchemeScopes {
    pub(super) fn get(&self, id: FullSchemeId) -> Result<&SchemeScope, IrVerifyError> {
        self.scopes.get(id.index()).and_then(Option::as_ref)
            .ok_or_else(|| IrVerifyError::new("generic callable has no verified binder scope"))
    }

    #[cfg(test)]
    pub(super) fn temporary_payload_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>() + self.scopes.capacity() * size_of::<Option<SchemeScope>>()
            + self.scopes.iter().flatten().map(|scope| {
                scope.type_parameters.capacity() * size_of::<FullSchemeTypeParameter>()
                    + scope.row_parameters.capacity() * size_of::<FullSchemeRowParameter>()
                    + (scope.type_binders.len() + scope.row_binders.len() + scope.effects.binders.len()) * size_of::<(u32, usize)>()
                    + scope.row_lacks.capacity() * size_of::<BTreeSet<Name>>()
                    + scope.row_lacks.iter().map(|lacks| lacks.len() * size_of::<Name>()).sum::<usize>()
                    + scope.effects.parameters.capacity() * size_of::<super::schemes::FullEffectParameter>()
            }).sum::<usize>()
            + self.invocations.capacity() * size_of::<Option<VerifiedInvocationEffects>>()
            + self.invocations.iter().flatten().map(|invocation| {
                invocation.contract.retained_bytes().saturating_sub(size_of::<EffectContract<SchemeEffectParameterId>>())
                    + invocation.predicates.capacity() * size_of::<EffectPredicate<SchemeEffectParameterId>>()
                    + invocation.predicates.iter().flat_map(EffectPredicate::contracts)
                        .map(|contract| contract.retained_bytes().saturating_sub(size_of::<EffectContract<SchemeEffectParameterId>>())).sum::<usize>()
            }).sum::<usize>()
    }
}

pub(super) fn prepare_tables(store: &FullStore) -> Result<VerifiedSchemeScopes, IrVerifyError> {
    let mut effects = effects::EffectCoverage::new(store);
    let scopes = verify_function_schemes(store, &mut effects)?;
    verify_scheme_tables(store, &scopes, &mut effects)?;
    effects.finish(&scopes)?;
    let mut budget = SchemeInstantiationBudget::new(store);
    budget.charge(scopes.len())?;
    let mut invocations = Vec::with_capacity(scopes.len());
    for (index, scope) in scopes.iter().enumerate() {
        let invocation = if let Some(scope) = scope {
            let scheme = &store.schemes[index];
            let contract = effects::decode_contract(store, scope, scheme.effects, &mut budget)?;
            budget.charge(scheme.effect_predicates.len as usize)?;
            let mut predicates = Vec::new();
            for predicate in bounded(scheme.effect_predicates, &store.scheme_effect_predicates, "scheme effect predicate range is invalid")? {
                predicates.push(effects::decode_predicate(store, scope, *predicate, &mut budget)?);
            }
            Some(VerifiedInvocationEffects { contract, predicates })
        } else { None };
        invocations.push(invocation);
    }
    Ok(VerifiedSchemeScopes { scopes, invocations })
}

pub(super) fn verify(store: &FullStore, producer_index: &GroundProducerIndex, contexts: producers::ProducerContexts<'_>, function_signatures: &Arc<super::FunctionSignatureIndex>) -> Result<Vec<(u32, super::ExecutableOwner, Arc<Type>)>, IrVerifyError> {
    let scopes = &producer_index.scheme_scopes(store)?.scopes;
    let mut requests = verify_scheme_body_facts(store, scopes, producer_index, function_signatures)?;
    verify_call_instantiations(store, scopes, &mut requests, function_signatures)?;
    producers::verify(store, producer_index, contexts, requests)
}

fn verify_function_schemes(store: &FullStore, effects: &mut effects::EffectCoverage) -> Result<Vec<Option<SchemeScope>>, IrVerifyError> {
    let mut scheme_owners = vec![None; store.schemes.len()];
    let mut scopes = (0..store.schemes.len()).map(|_| None).collect::<Vec<_>>();
    let mut type_parameter_coverage = vec![false; store.scheme_type_parameters.len()];
    let mut row_parameter_coverage = vec![false; store.scheme_row_parameters.len()];
    let mut row_lacks_coverage = vec![false; store.scheme_row_lacks.len()];
    let mut parameter_coverage = vec![false; store.scheme_params.len()];
    let mut body_fact_coverage = vec![false; store.scheme_body_facts.len()];

    for (function_index, function) in store.functions.iter().enumerate() {
        let FullCallableContract::Scheme(scheme_id) = function.contract else {
            continue;
        };
        let scheme_index = scheme_id.index();
        let Some(owner) = scheme_owners.get_mut(scheme_index) else {
            return Err(IrVerifyError::new("full function scheme id is out of bounds"));
        };
        if owner.replace(function_index).is_some() {
            return Err(IrVerifyError::new("full scheme belongs to multiple functions"));
        }
        let scheme = store.scheme(scheme_id)?;
        if scheme.reserved != [0; 3]
            || match scheme.callable_kind {
                crate::sema::types::CallableKind::Pure => store.function_metadata[function_index].flags & 1 != 0 || scheme.inferred_effects,
                crate::sema::types::CallableKind::Proc => store.function_metadata[function_index].flags & 1 == 0 || !scheme.effects.known,
                crate::sema::types::CallableKind::Stream => true,
            }
        {
            return Err(IrVerifyError::new("generic function contract is invalid"));
        }
        let type_parameters = mark_range(
            scheme.type_parameters,
            &store.scheme_type_parameters,
            &mut type_parameter_coverage,
            "scheme type parameter range is invalid or overlaps",
        )?
        .to_vec();
        let row_parameters = mark_range(
            scheme.row_parameters,
            &store.scheme_row_parameters,
            &mut row_parameter_coverage,
            "scheme row parameter range is invalid or overlaps",
        )?
        .to_vec();
        for parameter in &row_parameters {
            mark_range(
                parameter.lacks,
                &store.scheme_row_lacks,
                &mut row_lacks_coverage,
                "scheme row lacks range is invalid or overlaps",
            )?;
        }
        let params = mark_range(
            scheme.params,
            &store.scheme_params,
            &mut parameter_coverage,
            "scheme parameter range is invalid or overlaps",
        )?
        .to_vec();
        let _facts = mark_range(
            scheme.body_facts,
            &store.scheme_body_facts,
            &mut body_fact_coverage,
            "scheme body fact range is invalid or overlaps",
        )?;
        if !scheme.effects.known && scheme.effects.direct.len != 0 {
            return Err(IrVerifyError::new("generic scheme has effects without an effect clause"));
        }
        let function_params = bounded(function.params, &store.params, "function parameter range is invalid")?;
        if params.len() != function_params.len() {
            return Err(IrVerifyError::new("generic scheme parameters do not match the function"));
        }
        for (index, (function_param, scheme_param)) in
            function_params.iter().zip(&params).enumerate()
        {
            if function_param.reserved != [0; 3] {
                return Err(IrVerifyError::new("generic function parameter metadata is invalid"));
            }
            if function_param.flags & 1 != 0 {
                if index + 1 != function_params.len() {
                    return Err(IrVerifyError::new("generic rest parameter is not final"));
                }
                verify_scheme_list_shape(store, scheme_param.ty)?;
            }
        }
        let effect_scope = effects.scope(store, scheme.effect_parameters)?;
        let scope = make_scope(store, type_parameters, row_parameters, effect_scope)?;
        effects.contract(store, scheme_index, &scope, scheme.effects)?;
        effects.predicates(store, scheme_index, &scope, scheme.effect_predicates)?;
        if function.checked_return_type != IR_NONE {
            return Err(IrVerifyError::new(
                "generic function duplicates its scheme return type",
            ));
        }
        scopes[scheme_index] = Some(scope);
    }

    let mut callable_coverage = vec![false; store.scheme_callable_monotypes.len()];
    let mut callable_parameter_coverage = vec![false; store.scheme_callable_parameters.len()];
    for node in &store.scheme_types {
        if node.tag != FullSchemeTypeTag::Callable { continue; }
        let covered = callable_coverage.get_mut(node.first as usize)
            .ok_or_else(|| IrVerifyError::new("scheme callable monotype id is out of bounds"))?;
        if std::mem::replace(covered, true) {
            return Err(IrVerifyError::new("scheme callable monotype belongs to multiple type nodes"));
        }
        let callable = store.scheme_callable_monotype(node.first)?;
        if callable.reserved != [0; 3] || callable.kind == crate::sema::types::CallableKind::Stream {
            return Err(IrVerifyError::new("scheme callable monotype kind or metadata is invalid"));
        }
        if !callable.effects.known && callable.effects.direct.len != 0 {
            return Err(IrVerifyError::new("scheme callable has effects without an effect bound"));
        }
        let parameters = mark_range(callable.params, &store.scheme_callable_parameters, &mut callable_parameter_coverage,
            "scheme callable parameter range is invalid or overlaps")?;
        let mut names = BTreeSet::new();
        for (index, parameter) in parameters.iter().enumerate() {
            let name = store.string(parameter.name)?;
            if name.is_empty() || !names.insert(name) || parameter.reserved != [0; 3]
                || parameter.flags & !3 != 0 || parameter.flags & 1 != 0 && (index + 1 != parameters.len() || parameter.flags & 2 != 0)
            {
                return Err(IrVerifyError::new("scheme callable parameter layout is invalid"));
            }
            if parameter.flags & 1 != 0 { verify_scheme_list_shape(store, parameter.ty)?; }
        }
        let effects = bounded(callable.effects.direct, &store.scheme_effects, "scheme callable effect range is invalid")?;
        for (index, effect) in effects.iter().enumerate() {
            if effects[..index].contains(effect) {
                return Err(IrVerifyError::new("scheme callable effects are duplicated"));
            }
        }
    }
    if callable_coverage.iter().any(|covered| !covered)
        || callable_parameter_coverage.iter().any(|covered| !covered)
        || scheme_owners.iter().any(Option::is_none)
        || type_parameter_coverage.iter().any(|covered| !covered)
        || row_parameter_coverage.iter().any(|covered| !covered)
        || row_lacks_coverage.iter().any(|covered| !covered)
        || parameter_coverage.iter().any(|covered| !covered)
        || body_fact_coverage.iter().any(|covered| !covered)
    {
        return Err(IrVerifyError::new("full scheme tables contain unowned rows"));
    }
    Ok(scopes)
}

fn make_scope(
    store: &FullStore,
    type_parameters: Vec<FullSchemeTypeParameter>,
    row_parameters: Vec<FullSchemeRowParameter>,
    effects: effects::EffectScope,
) -> Result<SchemeScope, IrVerifyError> {
    let mut type_binders = BTreeMap::new();
    for (index, parameter) in type_parameters.iter().enumerate() {
        if parameter.id == IR_NONE
            || parameter.binder as usize != index
            || type_binders.insert(parameter.id, index).is_some()
        {
            return Err(IrVerifyError::new("scheme type binders are duplicated or out of scope"));
        }
    }
    let mut row_binders = BTreeMap::new();
    let mut row_lacks = Vec::with_capacity(row_parameters.len());
    for (index, parameter) in row_parameters.iter().enumerate() {
        if parameter.id == IR_NONE
            || parameter.binder as usize != index
            || row_binders.insert(parameter.id, index).is_some()
        {
            return Err(IrVerifyError::new("scheme row binders are duplicated or out of scope"));
        }
        let mut lacks = BTreeSet::new();
        let mut previous = None;
        for &raw in bounded(
            parameter.lacks,
            &store.scheme_row_lacks,
            "scheme row lacks range is invalid",
        )? {
            let text = store.string(raw)?;
            if text.is_empty() || previous.is_some_and(|previous: &str| previous >= text) {
                return Err(IrVerifyError::new("scheme row lacks labels are not sorted unique"));
            }
            previous = Some(text);
            if !lacks.insert(Name::intern(text)) {
                return Err(IrVerifyError::new("scheme row lacks labels are duplicated"));
            }
        }
        row_lacks.push(lacks);
    }
    Ok(SchemeScope {
        type_parameters,
        row_parameters,
        type_binders,
        row_binders,
        row_lacks,
        effects,
    })
}

fn verify_scheme_tables(
    store: &FullStore,
    scopes: &[Option<SchemeScope>],
    effects: &mut effects::EffectCoverage,
) -> Result<(), IrVerifyError> {
    let mut type_owners = vec![None; store.scheme_types.len()];
    let mut type_states = vec![0u8; store.scheme_types.len()];
    let mut field_coverage = vec![false; store.scheme_fields.len()];
    let mut child_coverage = vec![false; store.scheme_children.len()];
    let mut validation_coverage = vec![false; store.scheme_validations.len()];
    let mut schema_coverage = SchemaCoverage {
        expectations: vec![false; store.scheme_schema_expectations.len()],
        instances: vec![false; store.scheme_schema_instances.len()],
        arguments: vec![false; store.scheme_schema_arguments.len()],
        children: vec![false; store.scheme_schema_children.len()],
    };
    let mut type_binder_uses = scopes
        .iter()
        .map(|scope| scope.as_ref().map_or(Vec::new(), |scope| vec![false; scope.type_parameters.len()]))
        .collect::<Vec<_>>();
    let mut row_binder_uses = scopes
        .iter()
        .map(|scope| scope.as_ref().map_or(Vec::new(), |scope| vec![false; scope.row_parameters.len()]))
        .collect::<Vec<_>>();
    let mut validation_references = vec![false; store.scheme_validations.len()];
    let mut application_budget = SchemeInstantiationBudget::new(store);
    let mut application_coverage = vec![false; store.scheme_applications.len()];
    let mut argument_coverage = vec![false; store.scheme_application_arguments.len()];
    let mut return_coverage = vec![false; store.scheme_proc_returns.len()];
    let mut binding_coverage = vec![false; store.call_application_bindings.len()];
    let mut slot_coverage = vec![false; store.call_application_slots.len()];

    for (scheme_index, scope) in scopes.iter().enumerate() {
        let Some(scope) = scope else {
            continue;
        };
        let scheme = store
            .schemes
            .get(scheme_index)
            .ok_or_else(|| IrVerifyError::new("full scheme id is out of bounds"))?;
        let mut roots = Vec::new();
        let params = bounded(scheme.params, &store.scheme_params, "scheme parameter range is invalid")?;
        roots.extend(params.iter().map(|param| param.ty));
        roots.push(scheme.return_type);
        application_budget.charge(scheme.applications.len as usize)?;
        for application in mark_range(scheme.applications, &store.scheme_applications, &mut application_coverage,
            "scheme application range is invalid or overlaps")?
        {
            application_budget.charge(2 + application.arguments.len as usize + application.proc_returns.len as usize)?;
            roots.push(application.callee);
            roots.push(application.result);
            effects.contract(store, scheme_index, scope, application.effects)?;
            for argument in mark_range(application.arguments, &store.scheme_application_arguments, &mut argument_coverage,
                "scheme application argument range is invalid or overlaps")?
            {
                if let super::schemes::FullApplicationArgumentKind::Named(name) = argument.kind
                    && store.string(name)?.is_empty()
                { return Err(IrVerifyError::new("scheme application argument name is empty")); }
                roots.push(argument.ty);
            }
            for envelope in mark_range(application.proc_returns, &store.scheme_proc_returns, &mut return_coverage,
                "scheme application return range is invalid or overlaps")?
            {
                application_budget.charge(1 + envelope.propagated_errors.len as usize)?;
                roots.push(envelope.result);
                roots.extend(mark_range(envelope.propagated_errors, &store.scheme_children, &mut child_coverage,
                    "scheme application error range is invalid or overlaps")?.iter().copied());
            }
        }
        for fact in bounded(
            scheme.body_facts,
            &store.scheme_body_facts,
            "scheme body fact range is invalid",
        )? {
            roots.push(fact.ty);
        }
        for root in roots {
            visit_scheme_type(
                store,
                scheme_index,
                root,
                scope,
                &mut type_owners,
                &mut type_states,
                &mut field_coverage,
                &mut child_coverage,
                &mut validation_coverage,
                &mut validation_references,
                &mut type_binder_uses,
                &mut row_binder_uses,
                effects,
                0,
            )?;
        }
        if type_binder_uses[scheme_index].iter().any(|used| !used)
            || row_binder_uses[scheme_index].iter().any(|used| !used)
        {
            return Err(IrVerifyError::new("generic scheme contains an unused binder"));
        }
        let mut effects = Vec::new();
        for effect in bounded(scheme.effects.direct, &store.scheme_effects, "scheme effect range is invalid")? {
            if effects.contains(effect) {
                return Err(IrVerifyError::new("generic scheme effects are duplicated"));
            }
            effects.push(effect.clone());
        }
        for param in params {
            super::scheme_type_physical_kind(store, param.ty)?;
            if let Some(schema) = param.schema {
                verify_schema_expectation(
                    store,
                    schema,
                    &mut schema_coverage,
                    &mut BTreeSet::new(),
                    0,
                )?;
            }
        }
        super::scheme_return_kind(store, scheme.return_type)?;
        if let Some(schema) = scheme.return_schema {
            verify_schema_expectation(
                store,
                schema,
                &mut schema_coverage,
                &mut BTreeSet::new(),
                0,
            )?;
        }
    }
    let empty_scope = SchemeScope {
        type_parameters: Vec::new(),
        row_parameters: Vec::new(),
        type_binders: BTreeMap::new(),
        row_binders: BTreeMap::new(),
        row_lacks: Vec::new(),
        effects: effects::EffectScope::default(),
    };
    for instantiation in &store.call_instantiations {
        let caller = caller_scheme_for_instruction(
            store,
            instantiation.instruction as usize,
            scopes,
        )?;
        let (owner, scope) = caller.map_or((scopes.len(), &empty_scope), |(id, scope)| {
            (id.index(), scope)
        });
        effects.arguments(store, owner, scope, instantiation.effect_arguments)?;
        application_budget.charge(instantiation.application_bindings.len as usize)?;
        for binding in mark_range(instantiation.application_bindings, &store.call_application_bindings, &mut binding_coverage,
            "generic call application binding range is invalid or overlaps")?
        {
            match *binding {
                super::schemes::FullApplicationBinding::Selected { argument_slots, omitted_slots } => {
                    for range in [argument_slots, omitted_slots] {
                        application_budget.charge(range.len as usize)?;
                        if mark_range(range, &store.call_application_slots, &mut slot_coverage,
                            "generic call application slot range is invalid or overlaps")?.contains(&IR_NONE)
                        { return Err(IrVerifyError::new("generic call application slot is invalid")); }
                    }
                }
                super::schemes::FullApplicationBinding::Forwarded(ordinal) => {
                    let Some((caller, _)) = caller else {
                        return Err(IrVerifyError::new("ground generic call forwards an application requirement"));
                    };
                    if ordinal >= store.scheme(caller)?.applications.len {
                        return Err(IrVerifyError::new("generic call application ordinal is out of scope"));
                    }
                }
            }
        }
        let type_rows = bounded(
            instantiation.type_arguments,
            &store.call_type_arguments,
            "generic call type argument range is invalid",
        )?;
        let row_rows = bounded(
            instantiation.row_arguments,
            &store.call_row_arguments,
            "generic call row argument range is invalid",
        )?;
        for argument in type_rows {
            visit_scheme_type(
                store,
                owner,
                argument.ty,
                scope,
                &mut type_owners,
                &mut type_states,
                &mut field_coverage,
                &mut child_coverage,
                &mut validation_coverage,
                &mut validation_references,
                &mut type_binder_uses,
                &mut row_binder_uses,
                effects,
                0,
            )?;
        }
        for row in row_rows {
            for field in bounded(
                row.fields,
                &store.call_row_fields,
                "generic call row evidence range is invalid",
            )? {
                visit_scheme_type(
                    store,
                    owner,
                    field.ty,
                    scope,
                    &mut type_owners,
                    &mut type_states,
                    &mut field_coverage,
                    &mut child_coverage,
                    &mut validation_coverage,
                    &mut validation_references,
                    &mut type_binder_uses,
                    &mut row_binder_uses,
                    effects,
                    0,
                )?;
            }
        }
    }
    let mut previous_application_call = None;
    for site in &store.scheme_application_calls {
        if previous_application_call.is_some_and(|previous| previous >= site.instruction)
            || store.tags.get(site.instruction as usize) != Some(&FullTag::ExprDynamicCall)
        { return Err(IrVerifyError::new("scheme application calls are unordered or have an invalid instruction")); }
        previous_application_call = Some(site.instruction);
        let Some((owner, _)) = caller_scheme_for_instruction(store, site.instruction as usize, scopes)? else {
            return Err(IrVerifyError::new("scheme application call has no generic owner"));
        };
        if site.application >= store.scheme(owner)?.applications.len {
            return Err(IrVerifyError::new("scheme application call ordinal is out of scope"));
        }
    }
    let mut previous_return = None;
    for site in &store.scheme_application_returns {
        if previous_return.is_some_and(|previous| previous >= site.instruction)
            || store.tags.get(site.instruction as usize) != Some(&FullTag::StmtReturn)
        { return Err(IrVerifyError::new("scheme application returns are unordered or have an invalid instruction")); }
        previous_return = Some(site.instruction);
        let function = store.functions.get(site.function.index()).ok_or_else(|| IrVerifyError::new("scheme application return owner is out of bounds"))?;
        if !store.function_instruction_range(site.function.index())?.contains(&(site.instruction as usize)) {
            return Err(IrVerifyError::new("scheme application return belongs to another owner"));
        }
        let FullCallableContract::Scheme(id) = function.contract else { return Err(IrVerifyError::new("scheme application return has no generic owner")) };
        let scheme = store.scheme(id)?;
        if scheme.callable_kind != crate::sema::types::CallableKind::Proc { return Err(IrVerifyError::new("scheme application return owner is not a proc")); }
        let application = bounded(scheme.applications, &store.scheme_applications, "scheme application range is invalid")?
            .get(site.application as usize).ok_or_else(|| IrVerifyError::new("scheme return application ordinal is out of scope"))?;
        if site.envelope >= application.proc_returns.len { return Err(IrVerifyError::new("scheme return envelope ordinal is out of scope")); }
    }
    for callable in &store.scheme_callable_monotypes {
        for parameter in bounded(callable.params, &store.scheme_callable_parameters, "scheme callable parameter range is invalid")? {
            if let Some(schema) = parameter.schema {
                verify_schema_expectation(store, schema, &mut schema_coverage, &mut BTreeSet::new(), 0)?;
            }
        }
        if let Some(schema) = callable.return_schema {
            verify_schema_expectation(store, schema, &mut schema_coverage, &mut BTreeSet::new(), 0)?;
        }
    }
    if type_owners.iter().any(Option::is_none)
        || type_states.iter().any(|state| *state != 2)
        || field_coverage.iter().any(|covered| !covered)
        || child_coverage.iter().any(|covered| !covered)
        || validation_coverage.iter().any(|covered| !covered)
        || validation_references.iter().any(|referenced| !referenced)
        || schema_coverage.expectations.iter().any(|covered| !covered)
        || schema_coverage.instances.iter().any(|covered| !covered)
        || schema_coverage.arguments.iter().any(|covered| !covered)
        || schema_coverage.children.iter().any(|covered| !covered)
        || application_coverage.iter().any(|covered| !covered)
        || argument_coverage.iter().any(|covered| !covered)
        || return_coverage.iter().any(|covered| !covered)
        || binding_coverage.iter().any(|covered| !covered)
        || slot_coverage.iter().any(|covered| !covered)
    {
        return Err(IrVerifyError::new("scheme type tables contain unowned rows"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn visit_scheme_type(
    store: &FullStore,
    scheme_index: usize,
    id: SchemeTypeId,
    scope: &SchemeScope,
    type_owners: &mut [Option<usize>],
    type_states: &mut [u8],
    field_coverage: &mut [bool],
    child_coverage: &mut [bool],
    validation_coverage: &mut [bool],
    validation_references: &mut [bool],
    type_binder_uses: &mut [Vec<bool>],
    row_binder_uses: &mut [Vec<bool>],
    effects: &mut effects::EffectCoverage,
    depth: usize,
) -> Result<(), IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("scheme type nesting is too deep"));
    }
    let index = id.index();
    let Some(owner) = type_owners.get_mut(index) else {
        return Err(IrVerifyError::new("scheme type id is out of bounds"));
    };
    if owner.is_some_and(|owner| owner != scheme_index) {
        return Err(IrVerifyError::new("scheme type node escaped its binder scope"));
    }
    *owner = Some(scheme_index);
    match type_states[index] {
        1 => return Err(IrVerifyError::new("scheme type graph contains a cycle")),
        2 => return Ok(()),
        _ => {}
    }
    type_states[index] = 1;
    let node = *store.scheme_type(id)?;
    if node.reserved != [0; 3] {
        return Err(IrVerifyError::new("scheme type node has reserved bits set"));
    }
    match node.tag {
        FullSchemeTypeTag::Callable => {
            require_empty_children(node.children)?;
            if node.second != 0 { return Err(IrVerifyError::new("scheme callable type has payload bits set")); }
            let callable = store.scheme_callable_monotype(node.first)?;
            let parameters = bounded(callable.params, &store.scheme_callable_parameters, "scheme callable parameter range is invalid")?;
            effects.contract(store, scheme_index, scope, callable.effects)?;
            for child in parameters.iter().map(|parameter| parameter.ty).chain(std::iter::once(callable.return_type)) {
                visit_scheme_type(store, scheme_index, child, scope, type_owners, type_states, field_coverage, child_coverage,
                    validation_coverage, validation_references, type_binder_uses, row_binder_uses, effects, depth + 1)?;
            }
        }
        FullSchemeTypeTag::Ground => {
            require_empty_children(node.children)?;
            if node.second != 0 {
                return Err(IrVerifyError::new("ground scheme type has payload bits set"));
            }
            let raw = TypeId::from_raw(node.first)
                .ok_or_else(|| IrVerifyError::new("ground scheme type id is invalid"))?;
            ensure_closed_type(&store.semantic.to_type(raw)?)?;
        }
        FullSchemeTypeTag::Parameter => {
            require_empty_children(node.children)?;
            if node.second != 0 {
                return Err(IrVerifyError::new("scheme type parameter has payload bits set"));
            }
            let binder = node.first as usize;
            if binder >= scope.type_parameters.len() {
                return Err(IrVerifyError::new("scheme type parameter is out of scope"));
            }
            type_binder_uses[scheme_index][binder] = true;
        }
        FullSchemeTypeTag::List
        | FullSchemeTypeTag::Stream
        | FullSchemeTypeTag::Set
        | FullSchemeTypeTag::Optional => {
            require_empty_children(node.children)?;
            if node.second != 0 {
                return Err(IrVerifyError::new("unary scheme type has payload bits set"));
            }
            visit_scheme_type(
                store,
                scheme_index,
                SchemeTypeId::from_raw(node.first)?,
                scope,
                type_owners,
                type_states,
                field_coverage,
                child_coverage,
                validation_coverage,
                validation_references,
                type_binder_uses,
                row_binder_uses,
                effects,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Map | FullSchemeTypeTag::Result => {
            require_empty_children(node.children)?;
            visit_scheme_type(
                store,
                scheme_index,
                SchemeTypeId::from_raw(node.first)?,
                scope,
                type_owners,
                type_states,
                field_coverage,
                child_coverage,
                validation_coverage,
                validation_references,
                type_binder_uses,
                row_binder_uses,
                effects,
                depth + 1,
            )?;
            visit_scheme_type(
                store,
                scheme_index,
                SchemeTypeId::from_raw(node.second)?,
                scope,
                type_owners,
                type_states,
                field_coverage,
                child_coverage,
                validation_coverage,
                validation_references,
                type_binder_uses,
                row_binder_uses,
                effects,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Record => {
            if node.second != 0 {
                return Err(IrVerifyError::new("scheme record has payload bits set"));
            }
            let fields = mark_range(
                node.children,
                &store.scheme_fields,
                field_coverage,
                "scheme record field range is invalid or overlaps",
            )?;
            let mut names = BTreeSet::new();
            let mut previous = None;
            for field in fields {
                let text = store.string(field.name)?;
                let name = Name::intern(text);
                if text.is_empty() || !names.insert(name) {
                    return Err(IrVerifyError::new("scheme record fields are duplicated or empty"));
                }
                if previous.is_some_and(|previous: Name| previous >= name) {
                    return Err(IrVerifyError::new("scheme record fields are not sorted"));
                }
                previous = Some(name);
                visit_scheme_type(
                    store,
                    scheme_index,
                    field.ty,
                    scope,
                    type_owners,
                    type_states,
                    field_coverage,
                    child_coverage,
                    validation_coverage,
                    validation_references,
                    type_binder_uses,
                    row_binder_uses,
                    effects,
                    depth + 1,
                )?;
            }
            if node.first != IR_NONE {
                let binder = node.first as usize;
                if binder >= scope.row_parameters.len() {
                    return Err(IrVerifyError::new("scheme row parameter is out of scope"));
                }
                row_binder_uses[scheme_index][binder] = true;
                if names.iter().any(|name| !scope.row_lacks[binder].contains(name)) {
                    return Err(IrVerifyError::new("scheme row lacks omit a required field"));
                }
            }
        }
        FullSchemeTypeTag::Union => {
            if node.first != 0 || node.second != 0 {
                return Err(IrVerifyError::new("scheme union has payload bits set"));
            }
            let children = mark_range(
                node.children,
                &store.scheme_children,
                child_coverage,
                "scheme union child range is invalid or overlaps",
            )?;
            if children.len() < 2 {
                return Err(IrVerifyError::new("scheme union has too few members"));
            }
            for child in children {
                visit_scheme_type(
                    store,
                    scheme_index,
                    *child,
                    scope,
                    type_owners,
                    type_states,
                    field_coverage,
                    child_coverage,
                    validation_coverage,
                    validation_references,
                    type_binder_uses,
                    row_binder_uses,
                    effects,
                    depth + 1,
                )?;
            }
        }
        FullSchemeTypeTag::Validated => {
            require_empty_children(node.children)?;
            let validation_index = node.second as usize;
            let validation = store
                .scheme_validations
                .get(validation_index)
                .ok_or_else(|| IrVerifyError::new("scheme validation id is out of bounds"))?;
            verify_validation_row(store, *validation)?;
            validation_coverage[validation_index] = true;
            validation_references[validation_index] = true;
            visit_scheme_type(
                store,
                scheme_index,
                SchemeTypeId::from_raw(node.first)?,
                scope,
                type_owners,
                type_states,
                field_coverage,
                child_coverage,
                validation_coverage,
                validation_references,
                type_binder_uses,
                row_binder_uses,
                effects,
                depth + 1,
            )?;
        }
    }
    type_states[index] = 2;
    Ok(())
}

fn verify_validation_row(
    store: &FullStore,
    row: super::schemes::FullSchemeValidation,
) -> Result<Validation, IrVerifyError> {
    if row.reserved != [0; 7] {
        return Err(IrVerifyError::new("scheme validation has reserved bits set"));
    }
    match row.tag {
        FullSchemeValidationTag::NonEmpty if row.name == IR_NONE && row.low == 0 && row.high == 0 => {
            Ok(Validation::NonEmpty)
        }
        FullSchemeValidationTag::RelPath if row.name == IR_NONE && row.low == 0 && row.high == 0 => {
            Ok(Validation::RelPath)
        }
        FullSchemeValidationTag::Nominal if row.low == 0 && row.high == 0 => {
            let name = store.string(row.name)?;
            if name.is_empty() {
                return Err(IrVerifyError::new("scheme nominal validation has an empty name"));
            }
            Ok(Validation::Nominal(Name::intern(name)))
        }
        FullSchemeValidationTag::Range if row.name == IR_NONE => Ok(Validation::Range(
            IntRange::new(row.low, row.high)
                .ok_or_else(|| IrVerifyError::new("scheme validation range is empty"))?,
        )),
        _ => Err(IrVerifyError::new("scheme validation payload is invalid")),
    }
}

fn verify_schema_expectation(
    store: &FullStore,
    id: super::schemes::FullSchemeSchemaExpectationId,
    coverage: &mut SchemaCoverage,
    active: &mut BTreeSet<usize>,
    depth: usize,
) -> Result<(), IrVerifyError> {
    if depth > 128 || !active.insert(id.index()) {
        return Err(IrVerifyError::new("scheme schema expectation graph is cyclic"));
    }
    let Some(expectation_covered) = coverage.expectations.get_mut(id.index()) else {
        return Err(IrVerifyError::new("scheme schema expectation id is out of bounds"));
    };
    if *expectation_covered {
        return Err(IrVerifyError::new("scheme schema expectation is shared by multiple owners"));
    }
    *expectation_covered = true;
    let expectation = store
        .scheme_schema_expectations
        .get(id.index())
        .ok_or_else(|| IrVerifyError::new("scheme schema expectation id is out of bounds"))?;
    let instances = mark_range(
        expectation.instances,
        &store.scheme_schema_instances,
        &mut coverage.instances,
        "scheme schema instance range is invalid",
    )?;
    for type_id in instances {
        if type_id.definition == IR_NONE {
            return Err(IrVerifyError::new("scheme schema definition id is invalid"));
        }
        let arguments = mark_range(
            type_id.arguments,
            &store.scheme_schema_arguments,
            &mut coverage.arguments,
            "scheme schema argument range is invalid",
        )?;
        for &argument in arguments {
            ensure_closed_type(&store.semantic.to_type(argument)?)?;
        }
    }
    let children = mark_range(
        expectation.children,
        &store.scheme_schema_children,
        &mut coverage.children,
        "scheme schema child range is invalid",
    )?;
    let mut components = BTreeSet::new();
    let mut previous = None;
    for child in children {
        if child.reserved != [0; 3] {
            return Err(IrVerifyError::new("scheme schema child has reserved bits set"));
        }
        let name = match child.component {
            super::schema::FullSchemeSchemaComponentTag::Field => {
                let text = store.string(child.name)?;
                if text.is_empty() {
                    return Err(IrVerifyError::new("scheme schema field is empty"));
                }
                Some(Name::intern(text))
            }
            _ if child.name == IR_NONE => None,
            _ => return Err(IrVerifyError::new("scheme schema child name is invalid")),
        };
        let component = (child.component as u8, name);
        if !components.insert(component)
            || previous.is_some_and(|previous| previous >= component)
        {
            return Err(IrVerifyError::new("scheme schema children are duplicated or unsorted"));
        }
        previous = Some(component);
        verify_schema_expectation(store, child.expectation, coverage, active, depth + 1)?;
    }
    active.remove(&id.index());
    Ok(())
}

fn verify_scheme_body_facts(
    store: &FullStore,
    scopes: &[Option<SchemeScope>],
    producer_index: &GroundProducerIndex,
    function_signatures: &Arc<super::FunctionSignatureIndex>,
) -> Result<producers::SchemeProducerRequests, IrVerifyError> {
    let mut requests = producers::SchemeProducerRequests::default();
    let mut covered = vec![false; store.scheme_body_facts.len()];
    let mut registered_coverage = BTreeSet::new();
    let mut owners = vec![None; scopes.len()];
    for (index, function) in store.functions.iter().enumerate() {
        if let FullCallableContract::Scheme(scheme) = function.contract {
            let owner = owners.get_mut(scheme.index()).ok_or_else(|| IrVerifyError::new("generic scheme owner is out of bounds"))?;
            if owner.replace(index).is_some() { return Err(IrVerifyError::new("generic scheme has multiple function owners")); }
        }
    }
    for (scheme_index, scope) in scopes.iter().enumerate() {
        let Some(scope) = scope else { continue };
        let scheme_id = FullSchemeId::from_raw(u32::try_from(scheme_index).map_err(|_| IrVerifyError::new("full scheme id is out of bounds"))?)?;
        let scheme = store.scheme(scheme_id)?;
        let function_index = owners[scheme_index].ok_or_else(|| IrVerifyError::new("generic scheme has no function owner"))?;
        let function_id = IrFunctionId::new(function_index).map_err(|_| IrVerifyError::new("generic function id is invalid"))?;
        let function = store.functions[function_index];
        let instruction_range = store.function_instruction_range(function_index)?;
        let facts_range = scheme.body_facts.bounds(store.scheme_body_facts.len()).ok_or_else(|| IrVerifyError::new("scheme body fact range is invalid"))?;
        let mut budget = SchemeInstantiationBudget::new(store);
        let mut types = BTreeMap::new();
        let mut tails = BTreeMap::new();
        let mut declarations = BTreeMap::new();
        for (offset, fact) in store.scheme_body_facts[facts_range.clone()].iter().enumerate() {
            covered[facts_range.start + offset] = true;
            let instruction = fact.instruction as usize;
            if fact.function != function_id || !instruction_range.contains(&instruction) || fact.reserved != [0; 3] { return Err(IrVerifyError::new("generic body fact has invalid ownership")); }
            let ty = decode_scheme_type(store, scope, fact.ty, &mut budget, 0)?;
            match fact.kind {
                FullSchemeBodyFactKind::Expression if store.tags[instruction].kind() == schema::InstructionKind::Expr => {
                    if types.insert(instruction, ty).is_some() { return Err(IrVerifyError::new("generic body expression fact is duplicated")); }
                }
                FullSchemeBodyFactKind::Tail if store.tags[instruction] == FullTag::StmtReturn => {
                    if tails.insert(instruction, ty).is_some() { return Err(IrVerifyError::new("generic body tail fact is duplicated")); }
                }
                FullSchemeBodyFactKind::LocalImmutable | FullSchemeBodyFactKind::LocalMutable if matches!(store.tags[instruction], FullTag::StmtLet | FullTag::StmtLetRecord) => {
                    if declarations.insert(instruction, (ty, fact.kind == FullSchemeBodyFactKind::LocalMutable)).is_some() { return Err(IrVerifyError::new("generic local declaration fact is duplicated")); }
                }
                _ => return Err(IrVerifyError::new("generic body fact has an invalid instruction kind")),
            }
        }
        let mut decoder = decoder_for_function(store, function_index)?;
        decoder.function_signature_index = Some(Arc::clone(function_signatures));
        let mut slots = BTreeMap::new();
        for (slot, parameter) in bounded(scheme.params, &store.scheme_params, "scheme parameter range is invalid")?.iter().enumerate() {
            slots.insert(slot, decode_scheme_type(store, scope, parameter.ty, &mut budget, 0)?);
        }
        for capture in bounded(function.captures, &store.captures, "generic capture range is invalid")? {
            let slot = (capture.slot_and_flags & !(1 << 31)) as usize;
            if capture.slot_and_flags & (1 << 31) != 0 { return Err(IrVerifyError::new("generic body captures mutable state")); }
            let ty = store.semantic.to_type_with_budget(capture.type_id, &mut budget.semantic_types)?;
            ensure_closed_type(&ty)?;
            if ty.may_hold_resource() { return Err(IrVerifyError::new("generic body captures resource state")); }
            let actual = decoder.checked_slot_type(slot)?.ok_or_else(|| IrVerifyError::new("generic capture has no authenticated lexical writer type"))?;
            if store.semantic.to_type_with_budget(actual, &mut budget.semantic_types)? != ty { return Err(IrVerifyError::new("generic capture type disagrees with its lexical writer")); }
            if slots.insert(slot, SchemeType::Ground(ty)).is_some() { return Err(IrVerifyError::new("generic capture shares a parameter slot")); }
        }
        let body_id = IrBlockId::from_raw(function.body).ok_or_else(|| IrVerifyError::new("generic body block is invalid"))?;
        let block = store.blocks.get(body_id.index()).ok_or_else(|| IrVerifyError::new("generic body block is out of bounds"))?;
        let statements = store.payload(block.instructions)?;
        let expected_return = decode_scheme_type(store, scope, scheme.return_type, &mut budget, 0)?;
        let analysis = producer_index.scheme_return_analysis(function_id)?;
        let mut constant_budget = SchemeInstantiationBudget::for_prepared_constants(store, producer_index.scheme_prepared_constant_work(function_id)?);
        let body::BodyProof { expressions: visited, writers, statements: visited_statements, streams: stream_sources, captures, applications } = body::verify_body(store, &decoder, &types, &mut tails, &mut declarations, slots, scope, producer_index.scheme_scopes(store)?, &expected_return, statements.iter().skip(1).map(|raw| *raw as usize).collect(), &analysis, &mut registered_coverage, &mut budget, &mut constant_budget)?;
        let slot_facts = bounded(function.slot_type_facts, &store.slot_type_facts, "generic slot type fact range is invalid")?;
        for (slot, &raw) in slot_facts.iter().enumerate().filter(|(_, raw)| **raw != IR_NONE) {
            let ty = TypeId::from_raw(raw).ok_or_else(|| IrVerifyError::new("generic slot type fact is invalid"))?;
            let ground = store.semantic.to_type_with_budget(ty, &mut budget.semantic_types)?;
            let actual = writers.get(&slot).ok_or_else(|| IrVerifyError::new("generic slot type fact has no initialized writer"))?;
            let equal = match actual {
                SchemeType::Ground(actual) => actual == &ground,
                actual => actual.closed_ground_type().is_some_and(|actual| actual == ground),
            };
            if !equal { return Err(IrVerifyError::new("generic slot type fact disagrees with its initialized value")); }
        }
        if !tails.is_empty() || !declarations.is_empty() || visited.len() != types.len() { return Err(IrVerifyError::new("generic body contains an unchecked or escaped expression")); }
        let mut encoded_expressions = BTreeSet::new();
        let mut encoded_statements = BTreeSet::new();
        for instruction in instruction_range {
            budget.charge(1)?;
            match store.tags[instruction].kind() {
                schema::InstructionKind::Expr => { encoded_expressions.insert(instruction); }
                schema::InstructionKind::Stmt => { encoded_statements.insert(instruction); }
                _ => return Err(IrVerifyError::new("generic body contains an unchecked scalar instruction")),
            }
        }
        if visited != encoded_expressions || visited_statements != encoded_statements { return Err(IrVerifyError::new("generic body contains an unchecked instruction")); }
        budget.charge(stream_sources.len() + captures.len() + applications.len())?;
        requests.streams.extend(stream_sources.into_iter().map(|(source, expected_return)| producers::StreamIteration { owner: decoder.owner, source, expected_return }));
        requests.captures.extend(captures.into_iter().map(|(instruction, expected_error)| producers::CaptureErrorRequest { owner: decoder.owner, instruction, expected_error }));
        requests.applications.extend(applications.into_iter().map(|instruction| (decoder.owner, instruction)));
    }
    if covered.iter().any(|covered| !covered) || store.registered_calls.keys().any(|instruction| !registered_coverage.contains(instruction)) { return Err(IrVerifyError::new("generic body metadata contains unowned rows")); }
    Ok(requests)
}

fn body_types_equal(actual: &SchemeType, expected: &SchemeType) -> bool {
    actual == expected || matches!((actual.closed_ground_type(), expected.closed_ground_type()), (Some(actual), Some(expected)) if actual == expected)
}

fn body_type_matches(actual: &SchemeType, expected: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    if let SchemeType::Optional(inner) = expected {
        budget.charge(1)?;
        if actual == &SchemeType::Ground(Type::Null) { return Ok(true); }
        if !matches!(actual, SchemeType::Optional(_)) { return body_type_matches(actual, inner, budget); }
    }
    if actual == expected { return Ok(true); }
    match (actual.closed_ground_type(), expected.closed_ground_type()) {
        (Some(actual), Some(expected)) => initializer_type_matches_annotation(&actual, &expected, budget, 0),
        _ => Ok(false),
    }
}

fn body_propagated_error_matches_owner(error: &SchemeType, expected_return: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let compatible = match expected_return {
        SchemeType::Result(_, expected_error) => body_type_matches(error, expected_error, budget)?,
        SchemeType::Ground(Type::Result(_, expected_error)) => {
            budget.charge_clone(expected_error)?;
            body_type_matches(error, &SchemeType::Ground((**expected_error).clone()), budget)?
        }
        _ => return Err(IrVerifyError::new("generic propagation has no owner result boundary")),
    };
    if !compatible { return Err(IrVerifyError::new("generic propagated error disagrees with its owner boundary")); }
    Ok(())
}

fn body_ordering_matches(left: &SchemeType, right: &SchemeType) -> Result<(), IrVerifyError> {
    let left = left.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic ordering operand has no closed domain"))?;
    let right = right.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic ordering operand has no closed domain"))?;
    if !left.can_be_ordering_operand() || !right.can_be_ordering_operand() || !right.unvalidated().matches_expected(left.unvalidated()) { return Err(IrVerifyError::new("generic ordering operands have incompatible domains")); }
    Ok(())
}

fn body_equality_matches(left: &SchemeType, right: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    budget.charge(1)?;
    if body_types_equal(left, right) { return Ok(true); }
    budget.charge_scheme_clone(left)?;
    budget.charge_scheme_clone(right)?;
    if let (Some(left), Some(right)) = (left.closed_ground_type(), right.closed_ground_type()) {
        budget.charge_clone(&left)?;
        budget.charge_clone(&right)?;
        let (left, right) = (left.equality_operand_type(), right.equality_operand_type());
        return Ok(left.matches_expected(&right) || right.matches_expected(&left));
    }
    match (left, right) {
        (SchemeType::Ground(Type::Any), _) | (_, SchemeType::Ground(Type::Any)) => Ok(true),
        (SchemeType::Validated(_, inner), right) => body_equality_matches(inner, right, budget),
        (left, SchemeType::Validated(_, inner)) => body_equality_matches(left, inner, budget),
        (SchemeType::Optional(_), SchemeType::Ground(Type::Null)) | (SchemeType::Ground(Type::Null), SchemeType::Optional(_)) => Ok(true),
        (SchemeType::Optional(left), SchemeType::Optional(right)) => body_equality_matches(left, right, budget),
        (SchemeType::Optional(left), right) => body_equality_matches(left, right, budget),
        (left, SchemeType::Optional(right)) => body_equality_matches(left, right, budget),
        _ => Ok(false),
    }
}

fn body_type_matches_ground(actual: &SchemeType, expected: &Type) -> bool {
    if let Some(actual) = actual.closed_ground_type() { return actual.matches_expected(expected); }
    match (actual, expected) {
        (SchemeType::Record { fields, .. }, Type::Record(expected)) => expected.iter().all(|(name, expected)| fields.get(name).is_some_and(|actual| body_type_matches_ground(actual, expected))),
        _ => false,
    }
}

fn scheme_stored_type(actual: SchemeType, annotation: Option<Type>) -> Result<SchemeType, IrVerifyError> {
    let Some(annotation) = annotation else { return Ok(actual); };
    if !body_type_matches_ground(&actual, &annotation) || annotation.has_unsigned_constraint() && actual.closed_ground_type() != Some(annotation.clone()) {
        return Err(IrVerifyError::new("generic local storage disagrees with its checked type"));
    }
    Ok(SchemeType::Ground(annotation))
}

fn verify_body_expression(
    store: &FullStore,
    decoder: &FullDecoder<'_>,
    instruction: usize,
    types: &BTreeMap<usize, SchemeType>,
    slots: &body::LexicalScope,
    scopes: &body::LexicalScopes,
    pending: &mut Vec<usize>,
    visited: &mut BTreeSet<usize>,
    registered_coverage: &mut BTreeSet<u32>,
    expected_return: &SchemeType,
    budget: &mut SchemeInstantiationBudget,
    constant_budget: &mut SchemeInstantiationBudget,
) -> Result<SchemeType, IrVerifyError> {
    budget.charge(1)?;
    if !visited.insert(instruction) { return Err(IrVerifyError::new("generic body expression is shared or cyclic")); }
    let ty = types.get(&instruction).ok_or_else(|| IrVerifyError::new("generic expression has no checked body fact"))?;
    let mut child = |value: super::decode::ExprInstructionId<'_>, budget: &mut SchemeInstantiationBudget| {
        let instruction = value.raw() as usize;
        let ty = types.get(&instruction).ok_or_else(|| IrVerifyError::new("generic child has no checked body fact"))?;
        budget.charge_scheme_clone(ty)?;
        pending.push(instruction);
        Ok::<SchemeType, IrVerifyError>(ty.clone())
    };
    let expected = match decoder.expression_view(instruction)? {
        schema::ExprView::Param(slot) => {
            let actual = slots.slot_type(scopes, slot, budget)?.ok_or_else(|| IrVerifyError::new("generic body reads an uninitialized slot"))?;
            if matches!(ty, SchemeType::Optional(_) | SchemeType::Ground(Type::Optional(_)))
                && !matches!(actual, SchemeType::Optional(_) | SchemeType::Ground(Type::Optional(_)))
                && body_type_matches(actual, ty, budget)?
            {
                budget.charge_scheme_clone(ty)?;
                ty.clone()
            } else {
                budget.charge_scheme_clone(actual)?;
                actual.clone()
            }
        },
        schema::ExprView::Null => SchemeType::Ground(Type::Null),
        schema::ExprView::Unit => SchemeType::Ground(Type::Unit),
        schema::ExprView::Bool(_) => SchemeType::Ground(Type::Bool),
        schema::ExprView::Int(_) => SchemeType::Ground(Type::Int),
        schema::ExprView::UInt(_) => SchemeType::Ground(Type::UInt),
        schema::ExprView::Float(_) => SchemeType::Ground(Type::Float),
        schema::ExprView::Str(_) => SchemeType::Ground(Type::Str),
        schema::ExprView::Bytes(_) => SchemeType::Ground(Type::Bytes),
        schema::ExprView::Duration(_) => SchemeType::Ground(Type::Duration),
        schema::ExprView::PreparedRegex(_) => SchemeType::Ground(Type::Regex),
        schema::ExprView::PreparedConstant(value) => {
            budget.charge_scheme_clone(ty)?;
            let expected = ty.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic prepared constant has no closed contract"))?;
            ensure_substitution_type(&expected)?;
            decoder.verify_prepared_constant_type(value, &expected, &mut |amount| constant_budget.charge(amount))?;
            ty.clone()
        }
        schema::ExprView::Path(_) => SchemeType::Ground(Type::Path),
        schema::ExprView::List(mut values) => {
            let SchemeType::List(item) = ty else { return Err(IrVerifyError::new("generic list producer has no item contract")); };
            for value in &mut values {
                if !body_type_matches(&child(value?, budget)?, item, budget)? { return Err(IrVerifyError::new("generic list item disagrees with its checked contract")); }
            }
            ty.clone()
        }
        schema::ExprView::ListBuild(mut values) => {
            let SchemeType::List(item) = ty else { return Err(IrVerifyError::new("generic list builder has no item contract")); };
            for entry in &mut values {
                let (splice, value, _) = entry?;
                let mut actual = child(value, budget)?;
                let actual = if splice {
                    while let SchemeType::Validated(_, inner) = actual { budget.charge(1)?; actual = *inner; }
                    match actual {
                        SchemeType::List(item) => *item,
                        SchemeType::Ground(source) => {
                            let Type::List(item) = source.unvalidated() else { return Err(IrVerifyError::new("generic list splice has no list producer")) };
                            budget.charge_clone(item)?;
                            SchemeType::Ground((**item).clone())
                        }
                        _ => return Err(IrVerifyError::new("generic list splice has no list producer")),
                    }
                } else { actual };
                if !body_type_matches(&actual, item, budget)? { return Err(IrVerifyError::new("generic list splice disagrees with its item contract")); }
            }
            ty.clone()
        }
        schema::ExprView::FmtString(mut parts) | schema::ExprView::PathFmtString { mut parts, .. } => {
            while parts.len() != 0 {
                budget.charge(1)?;
                let part = parts.next().ok_or_else(|| IrVerifyError::new("generic interpolation part is absent"))??;
                if let super::decode::FmtPartView::Expr(value, _, _) = part {
                    let actual = child(value, budget)?;
                    let actual = actual.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic interpolation has no checked display domain"))?;
                    if !actual.can_display() { return Err(IrVerifyError::new("generic interpolation producer cannot be displayed")); }
                }
            }
            SchemeType::Ground(if store.tags[instruction] == FullTag::ExprPathFmtString { Type::Path } else { Type::Str })
        }
        schema::ExprView::MapLiteral(mut entries) => {
            let SchemeType::Map(key, item) = ty else { return Err(IrVerifyError::new("generic map producer has no key and value contract")); };
            let domain = key.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic map key domain is unresolved"))?;
            if !domain.is_map_key() { return Err(IrVerifyError::new("generic map key domain is invalid")); }
            for entry in &mut entries {
                let (entry_key, value, _) = entry?;
                if let Some(entry_key) = entry_key {
                    if !body_type_matches(&child(entry_key, budget)?, key, budget)? || !body_type_matches(&child(value, budget)?, item, budget)? {
                        return Err(IrVerifyError::new("generic map entry disagrees with its checked domain"));
                    }
                } else if !body_type_matches(&child(value, budget)?, ty, budget)? {
                    return Err(IrVerifyError::new("generic map spread disagrees with its checked domain"));
                }
            }
            ty.clone()
        }
        schema::ExprView::Record(mut entries) => {
            let mut fields = BTreeMap::new();
            let mut has_spread = false;
            let mut tail = SchemeRowTail::Closed;
            for entry in &mut entries {
                match entry? {
                    super::decode::RecordEntryView::Field(name, value) => {
                        if fields.contains_key(&name) && !has_spread { return Err(IrVerifyError::new("generic record producer repeats a field")); }
                        fields.insert(name, child(value, budget)?);
                    }
                    super::decode::RecordEntryView::Spread(value) => {
                        has_spread = true;
                        let mut source = child(value, budget)?;
                        while let SchemeType::Validated(_, inner) = source { budget.charge(1)?; source = *inner; }
                        let (spread, spread_tail) = match source {
                            SchemeType::Record { fields, tail } => (fields, tail),
                            SchemeType::Ground(source) => {
                                let Type::Record(fields) = source.unvalidated() else { return Err(IrVerifyError::new("generic record spread has no record producer")) };
                                let mut spread = BTreeMap::new();
                                for (name, ty) in fields.iter() { budget.charge_clone(ty)?; spread.insert(*name, SchemeType::Ground(ty.clone())); }
                                (spread, SchemeRowTail::Closed)
                            }
                            _ => return Err(IrVerifyError::new("generic record spread has no record producer")),
                        };
                        if spread_tail != SchemeRowTail::Closed {
                            if tail != SchemeRowTail::Closed && tail != spread_tail { return Err(IrVerifyError::new("generic record spreads disagree on their row domain")); }
                            tail = spread_tail;
                        }
                        fields.extend(spread);
                    }
                }
            }
            SchemeType::Record { fields, tail }
        }
        schema::ExprView::RecordUpdate { base, mut updates, .. } => {
            let source = child(base, budget)?;
            if !matches!(source, SchemeType::Record { .. } | SchemeType::Ground(Type::Record(_))) { return Err(IrVerifyError::new("generic record update has no record base")); }
            for update in &mut updates {
                let (path, value, _) = update?;
                budget.charge(path.len())?;
                let path = path.collect::<Result<Vec<_>, _>>()?;
                let selected = body::record_path(&source, &path, budget)?.ok_or_else(|| IrVerifyError::new("generic record update target has no checked field"))?;
                if !body_type_matches(&child(value, budget)?, &selected, budget)? { return Err(IrVerifyError::new("generic record update replacement disagrees with its field")); }
            }
            source
        }
        schema::ExprView::Ok(value) => {
            let SchemeType::Result(ok, _) = ty else { return Err(IrVerifyError::new("generic success producer has no result contract")); };
            if !body_type_matches(&child(value, budget)?, ok, budget)? { return Err(IrVerifyError::new("generic success payload disagrees with its checked contract")); }
            ty.clone()
        }
        schema::ExprView::Err { value, cause } => {
            let SchemeType::Result(_, error) = ty else { return Err(IrVerifyError::new("generic failure producer has no result contract")); };
            if !body_type_matches(&child(value, budget)?, error, budget)? { return Err(IrVerifyError::new("generic failure payload disagrees with its checked contract")); }
            if let Some(cause) = cause && !body_type_matches_ground(&child(cause, budget)?, &Type::Error) { return Err(IrVerifyError::new("generic failure cause is not an error")); }
            ty.clone()
        }
        schema::ExprView::IfExpr { mut branches, else_value, .. } => {
            for branch in &mut branches {
                let (condition, value) = branch?;
                if !matches!(child(condition, budget)?.closed_ground_type(), Some(Type::Bool | Type::Status)) { return Err(IrVerifyError::new("generic branch condition has no checked boolean contract")); }
                if !body_type_matches(&child(value, budget)?, ty, budget)? { return Err(IrVerifyError::new("generic branch value disagrees with its checked contract")); }
            }
            if !body_type_matches(&child(else_value, budget)?, ty, budget)? { return Err(IrVerifyError::new("generic fallback value disagrees with its checked contract")); }
            ty.clone()
        }
        schema::ExprView::Index { base, index, .. } => {
            let mut source = child(base, budget)?;
            while let SchemeType::Validated(_, base) = source { budget.charge(1)?; source = *base; }
            let key = child(index, budget)?;
            match source {
                SchemeType::Map(expected_key, item) if body_type_matches(&key, &expected_key, budget)? => *item,
                SchemeType::Ground(Type::Map(expected_key, item)) => {
                    if !body_type_matches(&key, &SchemeType::Ground(*expected_key), budget)? { return Err(IrVerifyError::new("generic map index has an incompatible key producer")); }
                    SchemeType::Ground(*item)
                }
                SchemeType::List(item) if body_type_matches_ground(&key, &Type::Int) => *item,
                SchemeType::Ground(Type::List(item)) if body_type_matches_ground(&key, &Type::Int) => SchemeType::Ground(*item),
                SchemeType::Record { fields, .. } if body_type_matches_ground(&key, &Type::Str) => {
                    let key = slots.literal_string(scopes, decoder, index.raw() as usize, budget)?.ok_or_else(|| IrVerifyError::new("generic record index has no constant key producer"))?;
                    fields.get(&Name::intern(&key)).cloned().ok_or_else(|| IrVerifyError::new("generic record index has no selected field"))?
                }
                _ => return Err(IrVerifyError::new("generic index operands have no collection contract")),
            }
        }
        schema::ExprView::IndexFromEnd { base, .. } => {
            let mut source = child(base, budget)?;
            while let SchemeType::Validated(_, base) = source { budget.charge(1)?; source = *base; }
            let SchemeType::List(item) = source else { return Err(IrVerifyError::new("generic from-end index has no list contract")) };
            *item
        }
        schema::ExprView::Slice { base, start, end, .. } => {
            let mut source = child(base, budget)?;
            while let SchemeType::Validated(_, base) = source { budget.charge(1)?; source = *base; }
            for bound in [start, end].into_iter().flatten() {
                if !body_type_matches_ground(&child(bound, budget)?, &Type::Int) { return Err(IrVerifyError::new("generic slice bound is not an integer")); }
            }
            match source {
                SchemeType::List(_) => source,
                SchemeType::Ground(ty) => SchemeType::Ground(FullDecoder::checked_slice_type(ty)?),
                _ => return Err(IrVerifyError::new("generic slice has no collection contract")),
            }
        }
        schema::ExprView::ComparisonChain { first, mut pairs } => {
            let mut left = child(first, budget)?;
            for pair in &mut pairs {
                budget.charge(1)?;
                let (op, right, _) = pair?;
                if !matches!(op, crate::syntax::node::BinaryOp::Lt | crate::syntax::node::BinaryOp::Le | crate::syntax::node::BinaryOp::Gt | crate::syntax::node::BinaryOp::Ge) { return Err(IrVerifyError::new("generic comparison chain requires ordering operators")); }
                let right = child(right, budget)?;
                body_ordering_matches(&left, &right)?;
                left = right;
            }
            SchemeType::Ground(Type::Bool)
        }
        schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::Eq | crate::syntax::node::BinaryOp::Ne, left, right, .. } => {
            if !body_equality_matches(&child(left, budget)?, &child(right, budget)?, budget)? { return Err(IrVerifyError::new("generic equality operands have incompatible domains")); }
            SchemeType::Ground(Type::Bool)
        }
        schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::Lt | crate::syntax::node::BinaryOp::Le | crate::syntax::node::BinaryOp::Gt | crate::syntax::node::BinaryOp::Ge, left, right, .. } => {
            body_ordering_matches(&child(left, budget)?, &child(right, budget)?)?;
            SchemeType::Ground(Type::Bool)
        }
        schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::In | crate::syntax::node::BinaryOp::NotIn, left, right, .. } => {
            let needle = child(left, budget)?;
            let mut container = child(right, budget)?;
            while let SchemeType::Validated(_, inner) = container { budget.charge(1)?; container = *inner; }
            let compatible = match &container {
                SchemeType::List(item) => body_equality_matches(&needle, item, budget)?,
                SchemeType::Map(key, _) => match (key.closed_ground_type(), needle.closed_ground_type()) {
                    (Some(key), Some(needle)) if key.is_map_key() && !key.contains_recovery() => crate::sema::types::MembershipCheck::Expected(&key).accepts_producer(&needle),
                    _ => false,
                },
                SchemeType::Record { .. } => needle.closed_ground_type() == Some(Type::Str),
                _ => match (container.closed_ground_type(), needle.closed_ground_type()) {
                    (Some(container), Some(needle)) => container.membership_needle_type(&needle).accepts_producer(&needle),
                    _ => false,
                },
            };
            if !compatible { return Err(IrVerifyError::new("generic membership operands have incompatible domains")); }
            SchemeType::Ground(Type::Bool)
        }
        schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::And | crate::syntax::node::BinaryOp::Or, left, right, .. } => {
            for value in [left, right] {
                let actual = child(value, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic logical operand has no Boolean contract"))?;
                if !crate::sema::types::ConditionRole::Boolean.accepts(&actual) { return Err(IrVerifyError::new("generic logical operand has no Boolean contract")); }
            }
            SchemeType::Ground(Type::Bool)
        }
        schema::ExprView::Binary { op, left, right, .. } => {
            let left = child(left, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic binary operand has no closed operation contract"))?;
            let right = child(right, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic binary operand has no closed operation contract"))?;
            if matches!(op, crate::syntax::node::BinaryOp::Union | crate::syntax::node::BinaryOp::Intersect | crate::syntax::node::BinaryOp::Sub) && matches!(left.unvalidated(), Type::Set(_)) {
                if !initializer_type_matches_annotation(right.unvalidated(), left.unvalidated(), budget, 0)? { return Err(IrVerifyError::new("generic set operands have incompatible domains")); }
                SchemeType::Ground(left.into_unvalidated())
            } else if op == crate::syntax::node::BinaryOp::Add && matches!(left.unvalidated(), Type::List(_)) {
                let expected = ty.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic concatenation has no closed item contract"))?;
                budget.charge_clone(&expected)?;
                let expected = expected.into_unvalidated();
                if !matches!(expected, Type::List(_))
                    || !initializer_type_matches_annotation(left.unvalidated(), &expected, budget, 0)?
                    || !initializer_type_matches_annotation(right.unvalidated(), &expected, budget, 0)?
                { return Err(IrVerifyError::new("generic binary operands disagree with the operation")); }
                SchemeType::Ground(Type::concatenation_type([left.validated().map(|ty| ty.validation()), right.validated().map(|ty| ty.validation())], expected))
            } else {
                SchemeType::Ground(FullDecoder::checked_binary_result_type(op, left.unvalidated(), right.unvalidated()).ok_or_else(|| IrVerifyError::new("generic binary operands disagree with the operation"))?)
            }
        }
        schema::ExprView::Field { base, name, .. } => {
            let mut base = child(base, budget)?;
            while let SchemeType::Validated(_, inner) = base { budget.charge(1)?; base = *inner; }
            match base {
                SchemeType::Record { fields, .. } => fields.get(&Name::intern(name)).cloned(),
                _ => match base.closed_ground_type() {
                    Some(ty) => match ty.unvalidated() { Type::Record(fields) => fields.get(&Name::intern(name)).cloned().map(SchemeType::Ground), _ => None },
                    None => None,
                },
            }.ok_or_else(|| IrVerifyError::new("generic field projection has no required field contract"))?
        }
        schema::ExprView::Call { args, .. } | schema::ExprView::DirectPureCall { args, .. } | schema::ExprView::SelfCall { args, .. } => {
            let function = match decoder.expression_view(instruction)? {
                schema::ExprView::Call { function, .. } | schema::ExprView::DirectPureCall { function, .. } => function.id,
                schema::ExprView::SelfCall { .. } => IrFunctionId::from_raw(decoder.owner)
                    .ok_or_else(|| IrVerifyError::new("generic self call has no function owner"))?,
                _ => unreachable!("call argument view has a call target"),
            };
            let mut arguments = Vec::new();
            for argument in args {
                arguments.push(match argument? {
                    super::decode::CallArgView::Single(value) => Some((child(value, budget)?, false)),
                    super::decode::CallArgView::Splice(value) => Some((child(value, budget)?, true)),
                    super::decode::CallArgView::Default(slot) => { budget.charge(1)?; if slot as usize != arguments.len() { return Err(IrVerifyError::new("generic default argument has an invalid slot")); } None },
                });
            }
            let target = store.functions.get(function.index()).ok_or_else(|| IrVerifyError::new("generic body call target is invalid"))?;
            match target.contract {
                FullCallableContract::Scheme(_) => ty.clone(),
                FullCallableContract::Ground(signature) => {
                    let count = store.semantic.signature_param_count(signature)?;
                    budget.charge(count)?;
                    let parameters = (0..count).map(|slot| store.semantic.signature_param(signature, slot)).collect::<Result<Vec<_>, _>>()?;
                    let rest = parameters.iter().position(|(_, _, flags)| flags & 2 != 0);
                    if rest.is_some_and(|slot| slot + 1 != count) { return Err(IrVerifyError::new("generic rest parameter is not final")); }
                    let fixed = rest.unwrap_or(count);
                    let required = parameters[..fixed].iter().rposition(|(_, _, flags)| flags & 1 == 0).map_or(0, |slot| slot + 1);
                    if arguments.len() < required || rest.is_none() && arguments.len() > count { return Err(IrVerifyError::new("generic body call signature has incompatible arguments or effects")); }
                    for (index, actual) in arguments.iter().enumerate() {
                        let (_, expected, flags) = parameters[index.min(fixed)];
                        let Some((actual, splice)) = actual else {
                            if index >= fixed || flags & 1 == 0 { return Err(IrVerifyError::new("generic default argument has no matching parameter")); }
                            continue;
                        };
                        let expected = store.semantic.to_type_with_budget(expected, &mut budget.semantic_types)?;
                        let expected = if index >= fixed && !splice {
                            let Type::List(item) = expected else { return Err(IrVerifyError::new("generic rest parameter has no list domain")) };
                            *item
                        } else { expected };
                        if *splice && index < fixed || !body_type_matches_ground(actual, &expected) { return Err(IrVerifyError::new("generic body call argument disagrees with its checked parameter")); }
                    }
                    let result = if target.checked_return_type != IR_NONE { TypeId::from_raw(target.checked_return_type).ok_or_else(|| IrVerifyError::new("generic body call result is invalid"))? } else { store.semantic.signature_return_type(signature)? };
                    SchemeType::Ground(store.semantic.to_type_with_budget(result, &mut budget.semantic_types)?)
                }
            }
        }
        schema::ExprView::CheckedValue { value, check, .. } => {
            let actual = child(value, budget)?;
            let expected = store.semantic.to_type_with_budget(check.ty.id, &mut budget.semantic_types)?;
            if check.schema.is_some() || !body_type_matches_ground(&actual, &expected) || !runtime_complete_value_check_type(&expected, budget, 0)? { return Err(IrVerifyError::new("generic checked value is not a complete storage contract")); }
            SchemeType::Ground(expected)
        }
        schema::ExprView::Require { value, check, .. } => {
            let actual = child(value, budget)?;
            let expected = check.ty.materialize()?;
            if check.schema.is_some() || expected.contains_any() || expected.contains_inference() || expected.contains_recovery() || expected.may_hold_resource() || !body_type_matches_ground(&actual, &expected) { return Err(IrVerifyError::new("generic storage check has an invalid ground contract")); }
            SchemeType::Result(Box::new(SchemeType::Ground(expected)), Box::new(SchemeType::Ground(Type::Error)))
        }
        schema::ExprView::Try { value, .. } => {
            let actual = child(value, budget)?;
            let (ok, error) = match actual {
                SchemeType::Result(ok, error) => (*ok, *error),
                SchemeType::Ground(Type::Result(ok, error)) => (SchemeType::Ground(*ok), SchemeType::Ground(*error)),
                _ => return Err(IrVerifyError::new("generic propagation source is not a result")),
            };
            let owner_required = if let schema::ExprView::Require { value: input, check, .. } = decoder.expression_view(value.raw() as usize)? {
                let required = check.ty.materialize()?;
                if check.schema.is_some() || required.may_hold_resource() || error != SchemeType::Ground(Type::Error) { return Err(IrVerifyError::new("generic propagation has an invalid ground storage contract")); }
                let actual_input = types.get(&(input.raw() as usize)).ok_or_else(|| IrVerifyError::new("generic requirement has no checked input contract"))?;
                !body_type_matches(actual_input, &SchemeType::Ground(required), budget)?
                    && matches!(expected_return, SchemeType::Result(_, _) | SchemeType::Ground(Type::Result(_, _)))
            } else { true };
            if owner_required { body_propagated_error_matches_owner(&error, expected_return, budget)?; }
            ok
        }
        schema::ExprView::Method { receiver, name, args, .. } => {
            let receiver = child(receiver, budget)?;
            let mut arguments = Vec::new();
            for argument in args { arguments.push(Some(child(argument?, budget)?)); }
            let plan = store.registered_calls.get(&(instruction as u32)).ok_or_else(|| IrVerifyError::new("generic host method has no selected registry plan"))?;
            if plan.name.as_ref() != name || plan.receiver.is_none() { return Err(IrVerifyError::new("generic host method identity disagrees with its plan")); }
            let references = arguments.iter().map(Option::as_ref).collect::<Vec<_>>();
            if !crate::sema::builtin_templates::verify_registered_call(plan, Some(&receiver), &references, ty) { return Err(IrVerifyError::new("generic host method disagrees with its selected registry signature")); }
            registered_coverage.insert(instruction as u32);
            ty.clone()
        }
        schema::ExprView::RunCapture(run) => {
            commands::operands(run.operands, &mut child, budget)?;
            SchemeType::Ground(Type::run_capture_output(run.kind, run.assert_success).ok_or_else(|| IrVerifyError::new("generic process capture has no output contract"))?)
        }
        schema::ExprView::RunPipeline { segments, propagate, .. } => {
            let mut head = None;
            for segment in segments {
                budget.charge(1)?;
                let segment = segment?;
                head.get_or_insert(segment.kind);
                commands::operands(segment.operands, &mut child, budget)?;
            }
            let head = head.ok_or_else(|| IrVerifyError::new("generic process pipeline has no head"))?;
            SchemeType::Ground(Type::run_pipeline_output(head, propagate).ok_or_else(|| IrVerifyError::new("generic process pipeline has no output contract"))?)
        }
        schema::ExprView::ModuleCall { op, call_plan, args, .. } => {
            let mut arguments = Vec::new();
            for argument in args { arguments.push(argument?.map(|argument| child(argument, budget)).transpose()?); }
            if let Some(crate::runtime::eval::ModuleCallPlan::CommandArguments(plan)) = call_plan.map(Arc::as_ref) {
                let supplied = arguments.iter().map(Option::is_some).collect::<Vec<_>>();
                call_plan.expect("the command plan was matched").verify_arguments(op, &supplied).map_err(IrVerifyError::new)?;
                budget.charge(arguments.len() + plan.word_conversions.len() + plan.flag_slots.len())?;
                for (slot, actual) in arguments.iter().enumerate() {
                    let Some(actual) = actual else { continue; };
                    let expected = &plan.signature.params[slot].ty;
                    let conversion = plan.word_conversions.iter().any(|conversion| conversion.slot == slot);
                    let actual = actual.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic module command argument has no closed conversion contract"))?;
                    if !(actual.matches_expected(expected) || conversion && actual.unvalidated() == &Type::Str) { return Err(IrVerifyError::new("generic module command argument disagrees with its selected conversion")); }
                    if plan.flag_slots.contains(&slot) && actual != Type::Bool { return Err(IrVerifyError::new("generic module command flag has no Boolean producer")); }
                }
                SchemeType::Ground(plan.signature.return_ty.clone())
            } else if let Some(plan) = store.registered_calls.get(&(instruction as u32)) {
                let references = arguments.iter().map(Option::as_ref).collect::<Vec<_>>();
                if call_plan.is_some() || plan.receiver.is_some() || plan.signature.op != op || !crate::sema::builtin_templates::verify_registered_call(plan, None, &references, ty) { return Err(IrVerifyError::new("generic module call disagrees with its selected registry signature")); }
                registered_coverage.insert(instruction as u32);
                ty.clone()
            } else if call_plan.is_none() {
                SchemeType::Ground(commands::native_call(op, &arguments, budget)?)
            } else {
                return Err(IrVerifyError::new("generic module call has no selected registry plan"));
            }
        }
        _ => return Err(IrVerifyError::new("generic body contains an unsupported executable expression")),
    };
    if !body_types_equal(ty, &expected) { return Err(IrVerifyError::new("generic body expression disagrees with its checked type")); }
    Ok(ty.clone())
}

fn verify_call_instantiations(
    store: &FullStore,
    scopes: &[Option<SchemeScope>],
    requests: &mut producers::SchemeProducerRequests,
    function_signatures: &Arc<super::FunctionSignatureIndex>,
) -> Result<(), IrVerifyError> {
    let mut target_by_instruction = BTreeMap::new();
    for fact in &store.checked_call_targets {
        if target_by_instruction.insert(fact.instruction, fact.function).is_some() {
            return Err(IrVerifyError::new("checked call targets are duplicated"));
        }
    }
    let mut instantiation_coverage = vec![false; store.call_instantiations.len()];
    let mut type_argument_coverage = vec![false; store.call_type_arguments.len()];
    let mut row_argument_coverage = vec![false; store.call_row_arguments.len()];
    let mut row_field_coverage = vec![false; store.call_row_fields.len()];
    let mut instantiations = BTreeMap::new();
    let mut previous_instantiation = None;

    for (index, instantiation) in store.call_instantiations.iter().copied().enumerate() {
        if previous_instantiation.is_some_and(|previous| previous >= instantiation.instruction) {
            return Err(IrVerifyError::new("generic call instantiations are not strictly ordered"));
        }
        previous_instantiation = Some(instantiation.instruction);
        instantiation_coverage[index] = true;
        if instantiations.insert(instantiation.instruction, index).is_some() {
            return Err(IrVerifyError::new("generic call instantiations are duplicated"));
        }
        let instruction = instantiation.instruction as usize;
        if instruction >= store.tags.len()
            || !matches!(
                store.tags[instruction],
                FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall | FullTag::ExprFunctionRef | FullTag::ExprParam
            )
        {
            return Err(IrVerifyError::new("generic call instantiation has no direct call"));
        }
        let target = *target_by_instruction
            .get(&instantiation.instruction)
            .ok_or_else(|| IrVerifyError::new("generic call has no checked target"))?;
        let target_function = store
            .functions
            .get(target.index())
            .ok_or_else(|| IrVerifyError::new("generic call target is out of bounds"))?;
        let FullCallableContract::Scheme(scheme_id) = target_function.contract else {
            return Err(IrVerifyError::new("ground call carries generic substitution evidence"));
        };
        let scope = scopes
            .get(scheme_id.index())
            .and_then(Option::as_ref)
            .ok_or_else(|| IrVerifyError::new("generic call target has no binder scope"))?;
        let scheme = store.scheme(scheme_id)?;
        let reference = matches!(store.tags[instruction], FullTag::ExprFunctionRef | FullTag::ExprParam);
        if instantiation.application_bindings.len != scheme.applications.len {
            return Err(IrVerifyError::new("generic call application binding is incomplete"));
        }
        let type_rows = mark_range(
            instantiation.type_arguments,
            &store.call_type_arguments,
            &mut type_argument_coverage,
            "generic call type argument range is invalid or overlaps",
        )?;
        let row_rows = mark_range(
            instantiation.row_arguments,
            &store.call_row_arguments,
            &mut row_argument_coverage,
            "generic call row argument range is invalid or overlaps",
        )?;
        let effect_rows = bounded(instantiation.effect_arguments, &store.call_effect_arguments,
            "generic call effect argument range is invalid")?;
        if let Some((caller_scheme, caller_scope)) =
            caller_scheme_for_instruction(store, instruction, scopes)?
        {
            let substitution = build_symbolic_substitution(
                store,
                scope,
                caller_scope,
                type_rows,
                row_rows,
                effect_rows,
                &mut row_field_coverage,
            )?;
            let mut budget = SchemeInstantiationBudget::new(store);
            effects::verify_predicates(store, scheme, scope, &substitution.effects,
                Some((store.scheme(caller_scheme)?, caller_scope)), &mut budget)?;
            applications::verify_bindings(store, scheme, scope,
                applications::ApplicationSubstitution::Symbolic(&substitution),
                Some((store.scheme(caller_scheme)?, caller_scope)),
                bounded(instantiation.application_bindings, &store.call_application_bindings, "generic call application binding range is invalid")?, &mut budget)?;
            if reference {
                let fact = scheme_body_fact_type(store, caller_scheme, instantiation.instruction, FullSchemeBodyFactKind::Expression)?
                    .ok_or_else(|| IrVerifyError::new("callable reference has no checked scheme type"))?;
                let claimed = decode_scheme_type(store, caller_scope, fact, &mut budget, 0)?;
                references::verify(store, target_function, scheme, scope,
                    applications::ApplicationSubstitution::Symbolic(&substitution), &claimed, &mut budget)?;
                continue;
            }
            verify_symbolic_call_arguments(
                store,
                instruction,
                target_function,
                scheme,
                scope,
                caller_scheme,
                caller_scope,
                &substitution,
                &mut budget,
                function_signatures,
            )?;
            let result = scheme_body_fact_type(
                store,
                caller_scheme,
                instantiation.instruction,
                FullSchemeBodyFactKind::Expression,
            )?
            .ok_or_else(|| IrVerifyError::new("generic call has no checked scheme result"))?;
            let actual = decode_scheme_type(store, caller_scope, result, &mut budget, 0)?;
            let expected = substitute_scheme_type(
                &decode_scheme_type(store, scope, scheme.return_type, &mut budget, 0)?,
                scope,
                &substitution,
                &mut budget,
                0,
            )?;
            if actual != expected && !matches!((actual.closed_ground_type(), expected.closed_ground_type()), (Some(actual), Some(expected)) if actual == expected) {
                return Err(IrVerifyError::new(
                    "generic call result does not match its checked scheme fact",
                ));
            }
        } else {
            let substitution = build_substitution(
                store,
                scope,
                type_rows,
                row_rows,
                effect_rows,
                &mut row_field_coverage,
            )?;
            let mut materialization = SchemeInstantiationBudget::new(store);
            effects::verify_predicates(store, scheme, scope, &substitution.effects, None, &mut materialization)?;
            applications::verify_bindings(store, scheme, scope,
                applications::ApplicationSubstitution::Ground(&substitution), None,
                bounded(instantiation.application_bindings, &store.call_application_bindings, "generic call application binding range is invalid")?, &mut materialization)?;
            if reference {
                let fact = checked_expression_type_id(store, instantiation.instruction)
                    .ok_or_else(|| IrVerifyError::new("callable reference has no checked type"))?;
                let claimed = store.semantic.to_type_with_budget(fact, &mut materialization.semantic_types)?;
                ensure_closed_type(&claimed)?;
                references::verify(store, target_function, scheme, scope,
                    applications::ApplicationSubstitution::Ground(&substitution), &SchemeType::Ground(claimed), &mut materialization)?;
                continue;
            }
            requests.ground_arguments.extend(verify_instantiated_call_arguments(
                store,
                instruction,
                target_function,
                scheme,
                scope,
                &substitution,
                &mut materialization,
                function_signatures,
            )?);
            let result = checked_expression_type_id(store, instantiation.instruction)
                .ok_or_else(|| IrVerifyError::new("generic call has no checked result type"))?;
            let checked_result = store
                .semantic
                .to_type_with_budget(result, &mut materialization.semantic_types)?;
            ensure_closed_type(&checked_result)?;
            let expected_result = instantiate_scheme_type(
                store,
                scope,
                scheme.return_type,
                &substitution,
                &mut materialization,
                0,
            )?;
            if checked_result != expected_result {
                return Err(IrVerifyError::new(
                    "generic call result does not match its checked instantiation",
                ));
            }
        }
    }

    for (&instruction, &target) in &target_by_instruction {
        let target_function = store
            .functions
            .get(target.index())
            .ok_or_else(|| IrVerifyError::new("checked call target is out of bounds"))?;
        let generic = matches!(target_function.contract, FullCallableContract::Scheme(_));
        if generic != instantiations.contains_key(&instruction) {
            return Err(IrVerifyError::new(if generic {
                "generic direct call has no substitution evidence"
            } else {
                "ground direct call has generic substitution evidence"
            }));
        }
        if generic
            && checked_expression_type_id(store, instruction).is_none()
            && caller_scheme_for_instruction(store, instruction as usize, scopes)?.is_none()
        {
            return Err(IrVerifyError::new("generic direct call has no checked result fact"));
        }
    }
    for instruction in 0..store.tags.len() {
        if !matches!(
            store.tags[instruction],
            FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall
        ) {
            continue;
        }
        let decoder = decoder_for_instruction(store, instruction, function_signatures)?;
        let target = match decoder.expression_view(instruction)? {
            schema::ExprView::Call { function, .. }
            | schema::ExprView::DirectPureCall { function, .. } => function.id,
            schema::ExprView::SelfCall { .. } => IrFunctionId::from_raw(decoder.owner)
                .ok_or_else(|| IrVerifyError::new("generic self call has no function owner"))?,
            _ => return Err(IrVerifyError::new("direct call tag has an invalid view")),
        };
        let target_function = store
            .functions
            .get(target.index())
            .ok_or_else(|| IrVerifyError::new("generic call target is out of bounds"))?;
        if matches!(target_function.contract, FullCallableContract::Scheme(_)) {
            let instruction = u32::try_from(instruction)
                .map_err(|_| IrVerifyError::new("generic call instruction is out of bounds"))?;
            if !target_by_instruction.contains_key(&instruction)
                || !instantiations.contains_key(&instruction)
                || (checked_expression_type_id(store, instruction).is_none()
                    && caller_scheme_for_instruction(store, instruction as usize, scopes)?.is_none())
            {
                return Err(IrVerifyError::new(
                    "generic direct call has incomplete checked evidence",
                ));
            }
        }
    }
    if instantiation_coverage.iter().any(|covered| !covered)
        || type_argument_coverage.iter().any(|covered| !covered)
        || row_argument_coverage.iter().any(|covered| !covered)
        || row_field_coverage.iter().any(|covered| !covered)
    {
        return Err(IrVerifyError::new("generic call evidence contains unowned rows"));
    }
    Ok(())
}

fn build_substitution(
    store: &FullStore,
    scope: &SchemeScope,
    type_rows: &[FullTypeArgument],
    row_rows: &[super::schemes::FullRowArgument],
    effect_rows: &[super::schemes::FullEffectArgument],
    row_field_coverage: &mut [bool],
) -> Result<CallSubstitution, IrVerifyError> {
    let mut types = vec![None; scope.type_parameters.len()];
    let mut materialization = TypeMaterializationBudget::default();
    let mut type_ids = BTreeSet::new();
    for row in type_rows {
        if !type_ids.insert(row.binder) {
            return Err(IrVerifyError::new("generic call type binder is duplicated"));
        }
        let Some(&binder) = scope.type_binders.get(&row.binder) else {
            return Err(IrVerifyError::new("generic call type binder is out of scope"));
        };
        let ty = ground_call_substitution_type(store, row.ty, &mut materialization)?;
        ensure_substitution_type(&ty)?;
        if types[binder].replace(ty).is_some() {
            return Err(IrVerifyError::new("generic call type binder is duplicated"));
        }
    }
    if types.iter().any(Option::is_none) {
        return Err(IrVerifyError::new("generic call type substitution is incomplete"));
    }

    let mut rows = vec![None; scope.row_parameters.len()];
    let mut row_ids = BTreeSet::new();
    for row in row_rows {
        if !row_ids.insert(row.binder) {
            return Err(IrVerifyError::new("generic call row binder is duplicated"));
        }
        let Some(&binder) = scope.row_binders.get(&row.binder) else {
            return Err(IrVerifyError::new("generic call row binder is out of scope"));
        };
        if row.tail != IR_NONE {
            return Err(IrVerifyError::new(
                "ground generic call carries a symbolic row tail",
            ));
        }
        let mut fields = BTreeMap::new();
        let mut previous = None;
        for field in mark_range(
            row.fields,
            &store.call_row_fields,
            row_field_coverage,
            "generic call row evidence range is invalid or overlaps",
        )? {
            let text = store.string(field.name)?;
            if text.is_empty() || previous.is_some_and(|previous: &str| previous >= text) {
                return Err(IrVerifyError::new("generic call row fields are not sorted unique"));
            }
            previous = Some(text);
            let name = Name::intern(text);
            if scope.row_lacks[binder].contains(&name) {
                return Err(IrVerifyError::new("generic call row evidence violates a lacks constraint"));
            }
            let ty = ground_call_substitution_type(store, field.ty, &mut materialization)?;
            ensure_substitution_type(&ty)?;
            if fields.insert(name, ty).is_some() {
                return Err(IrVerifyError::new("generic call row field is duplicated"));
            }
        }
        if rows[binder].replace(fields).is_some() {
            return Err(IrVerifyError::new("generic call row binder is duplicated"));
        }
    }
    if rows.iter().any(Option::is_none) {
        return Err(IrVerifyError::new("generic call row substitution is incomplete"));
    }
    let effects = effects::build_substitution(store, scope, None, effect_rows,
        &mut SchemeInstantiationBudget::new(store))?;
    Ok(CallSubstitution {
        effects,
        types: types.into_iter().map(Option::unwrap).collect(),
        rows: rows.into_iter().map(Option::unwrap).collect(),
    })
}

fn ground_call_substitution_type(
    store: &FullStore,
    id: SchemeTypeId,
    budget: &mut TypeMaterializationBudget,
) -> Result<Type, IrVerifyError> {
    let node = store.scheme_type(id)?;
    if node.tag != FullSchemeTypeTag::Ground {
        return Err(IrVerifyError::new(
            "ground generic call carries a symbolic type substitution",
        ));
    }
    let type_id = TypeId::from_raw(node.first)
        .ok_or_else(|| IrVerifyError::new("ground scheme type id is invalid"))?;
    let ty = store.semantic.to_type_with_budget(type_id, budget)?;
    ensure_substitution_type(&ty)?;
    Ok(ty)
}

fn caller_scheme_for_instruction<'a>(
    store: &FullStore,
    instruction: usize,
    scopes: &'a [Option<SchemeScope>],
) -> Result<Option<(FullSchemeId, &'a SchemeScope)>, IrVerifyError> {
    for (index, function) in store.functions.iter().enumerate() {
        if !store.function_instruction_range(index)?.contains(&instruction) {
            continue;
        }
        let FullCallableContract::Scheme(id) = function.contract else {
            return Ok(None);
        };
        let scope = scopes
            .get(id.index())
            .and_then(Option::as_ref)
            .ok_or_else(|| IrVerifyError::new("generic caller has no binder scope"))?;
        return Ok(Some((id, scope)));
    }
    Ok(None)
}

fn scheme_body_fact_type(
    store: &FullStore,
    scheme_id: FullSchemeId,
    instruction: u32,
    kind: FullSchemeBodyFactKind,
) -> Result<Option<SchemeTypeId>, IrVerifyError> {
    let scheme = store.scheme(scheme_id)?;
    let mut found = None;
    for fact in bounded(
        scheme.body_facts,
        &store.scheme_body_facts,
        "scheme body fact range is invalid",
    )? {
        if fact.instruction == instruction
            && fact.kind == kind
            && found.replace(fact.ty).is_some()
        {
            return Err(IrVerifyError::new("generic body fact is duplicated"));
        }
    }
    Ok(found)
}

fn build_symbolic_substitution(
    store: &FullStore,
    callee_scope: &SchemeScope,
    caller_scope: &SchemeScope,
    type_rows: &[FullTypeArgument],
    row_rows: &[super::schemes::FullRowArgument],
    effect_rows: &[super::schemes::FullEffectArgument],
    row_field_coverage: &mut [bool],
) -> Result<SymbolicCallSubstitution, IrVerifyError> {
    let mut types = vec![None; callee_scope.type_parameters.len()];
    let mut type_ids = BTreeSet::new();
    let mut budget = SchemeInstantiationBudget::new(store);
    for row in type_rows {
        if !type_ids.insert(row.binder) {
            return Err(IrVerifyError::new("generic call type binder is duplicated"));
        }
        let Some(&binder) = callee_scope.type_binders.get(&row.binder) else {
            return Err(IrVerifyError::new("generic call type binder is out of scope"));
        };
        let ty = decode_scheme_type(store, caller_scope, row.ty, &mut budget, 0)?;
        validate_symbolic_substitution_type(&ty, caller_scope, 0)?;
        if types[binder].replace(ty).is_some() {
            return Err(IrVerifyError::new("generic call type binder is duplicated"));
        }
    }
    if types.iter().any(Option::is_none) {
        return Err(IrVerifyError::new("generic call type substitution is incomplete"));
    }

    let mut rows = vec![None; callee_scope.row_parameters.len()];
    let mut row_ids = BTreeSet::new();
    for row in row_rows {
        if !row_ids.insert(row.binder) {
            return Err(IrVerifyError::new("generic call row binder is duplicated"));
        }
        let Some(&callee_binder) = callee_scope.row_binders.get(&row.binder) else {
            return Err(IrVerifyError::new("generic call row binder is out of scope"));
        };
        let mut fields = BTreeMap::new();
        let mut previous = None;
        for field in mark_range(
            row.fields,
            &store.call_row_fields,
            row_field_coverage,
            "generic call row evidence range is invalid or overlaps",
        )? {
            let text = store.string(field.name)?;
            if text.is_empty() || previous.is_some_and(|previous: &str| previous >= text) {
                return Err(IrVerifyError::new("generic call row fields are not sorted unique"));
            }
            previous = Some(text);
            let name = Name::intern(text);
            if callee_scope.row_lacks[callee_binder].contains(&name) {
                return Err(IrVerifyError::new(
                    "generic call row evidence violates a lacks constraint",
                ));
            }
            let ty = decode_scheme_type(store, caller_scope, field.ty, &mut budget, 0)?;
            validate_symbolic_substitution_type(&ty, caller_scope, 0)?;
            if fields.insert(name, ty).is_some() {
                return Err(IrVerifyError::new("generic call row field is duplicated"));
            }
        }
        let tail = if row.tail == IR_NONE {
            SchemeRowTail::Closed
        } else {
            let caller_binder = row.tail as usize;
            let caller = caller_scope
                .row_parameters
                .get(caller_binder)
                .ok_or_else(|| IrVerifyError::new("generic call row tail is out of scope"))?;
            if !callee_scope.row_lacks[callee_binder]
                .iter()
                .chain(fields.keys())
                .all(|name| caller_scope.row_lacks[caller_binder].contains(name))
            {
                return Err(IrVerifyError::new(
                    "generic call row tail lacks do not cover the callee contract",
                ));
            }
            SchemeRowTail::Parameter(SchemeRowParameterId(caller.id))
        };
        if rows[callee_binder]
            .replace(SchemeRowSubstitution { fields, tail })
            .is_some()
        {
            return Err(IrVerifyError::new("generic call row binder is duplicated"));
        }
    }
    if rows.iter().any(Option::is_none) {
        return Err(IrVerifyError::new("generic call row substitution is incomplete"));
    }
    let effects = effects::build_substitution(store, callee_scope, Some(caller_scope), effect_rows, &mut budget)?;
    Ok(SymbolicCallSubstitution {
        effects,
        types: types.into_iter().map(Option::unwrap).collect(),
        rows: rows.into_iter().map(Option::unwrap).collect(),
    })
}

fn decode_scheme_type(
    store: &FullStore,
    scope: &SchemeScope,
    id: SchemeTypeId,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<SchemeType, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("scheme substitution type nesting is too deep"));
    }
    budget.charge(1)?;
    let node = *store.scheme_type(id)?;
    if node.reserved != [0; 3] {
        return Err(IrVerifyError::new("scheme substitution type has reserved bits set"));
    }
    let nested = |id, budget: &mut SchemeInstantiationBudget| {
        decode_scheme_type(store, scope, id, budget, depth + 1)
    };
    let ty = match node.tag {
        FullSchemeTypeTag::Callable => {
            let callable = store.scheme_callable_monotype(node.first)?;
            let params = bounded(callable.params, &store.scheme_callable_parameters, "scheme callable parameter range is invalid")?
                .iter().map(|parameter| {
                    budget.charge(1)?;
                    Ok(SchemeParameter {
                        name: Name::intern(store.string(parameter.name)?),
                        ty: nested(parameter.ty, budget)?,
                        schema_expectation: parameter.schema.map(|schema| decode_scheme_schema(store, schema, budget, depth + 1)).transpose()?,
                        defaulted: parameter.flags & 2 != 0,
                        rest: parameter.flags & 1 != 0,
                    })
                }).collect::<Result<Vec<_>, IrVerifyError>>()?;
            let return_type = nested(callable.return_type, budget)?;
            let return_schema = callable.return_schema.map(|schema| decode_scheme_schema(store, schema, budget, depth + 1)).transpose()?;
            let effects = effects::decode_contract(store, scope, callable.effects, budget)?;
            SchemeType::Callable {
                kind: callable.kind,
                monotype: Arc::new(CallableMonotype::new(params, return_type, return_schema)),
                effects,
            }
        }
        FullSchemeTypeTag::Ground => {
            let id = TypeId::from_raw(node.first)
                .ok_or_else(|| IrVerifyError::new("ground scheme type id is invalid"))?;
            let ty = store.semantic.to_type_with_budget(id, &mut budget.semantic_types)?;
            ensure_closed_type(&ty)?;
            SchemeType::Ground(ty)
        }
        FullSchemeTypeTag::Parameter => {
            let parameter = scope
                .type_parameters
                .get(node.first as usize)
                .ok_or_else(|| IrVerifyError::new("scheme type parameter is out of scope"))?;
            SchemeType::Parameter(SchemeTypeParameterId(parameter.id))
        }
        FullSchemeTypeTag::List => SchemeType::List(Box::new(nested(
            SchemeTypeId::from_raw(node.first)?,
            budget,
        )?)),
        FullSchemeTypeTag::Map => SchemeType::Map(
            Box::new(nested(SchemeTypeId::from_raw(node.first)?, budget)?),
            Box::new(nested(SchemeTypeId::from_raw(node.second)?, budget)?),
        ),
        FullSchemeTypeTag::Stream => SchemeType::Stream(Box::new(nested(
            SchemeTypeId::from_raw(node.first)?,
            budget,
        )?)),
        FullSchemeTypeTag::Set => SchemeType::Set(Box::new(nested(
            SchemeTypeId::from_raw(node.first)?,
            budget,
        )?)),
        FullSchemeTypeTag::Result => SchemeType::Result(
            Box::new(nested(SchemeTypeId::from_raw(node.first)?, budget)?),
            Box::new(nested(SchemeTypeId::from_raw(node.second)?, budget)?),
        ),
        FullSchemeTypeTag::Optional => SchemeType::Optional(Box::new(nested(
            SchemeTypeId::from_raw(node.first)?,
            budget,
        )?)),
        FullSchemeTypeTag::Record => {
            let mut fields = BTreeMap::new();
            let mut previous = None;
            for field in bounded(
                node.children,
                &store.scheme_fields,
                "scheme record field range is invalid",
            )? {
                let text = store.string(field.name)?;
                if text.is_empty() || previous.is_some_and(|previous: &str| previous >= text) {
                    return Err(IrVerifyError::new("scheme record fields are not sorted unique"));
                }
                previous = Some(text);
                let name = Name::intern(text);
                let field_type = nested(field.ty, budget)?;
                if fields.insert(name, field_type).is_some() {
                    return Err(IrVerifyError::new("scheme record fields are duplicated"));
                }
            }
            let tail = if node.first == IR_NONE {
                SchemeRowTail::Closed
            } else {
                let parameter = scope
                    .row_parameters
                    .get(node.first as usize)
                    .ok_or_else(|| IrVerifyError::new("scheme row parameter is out of scope"))?;
                SchemeRowTail::Parameter(SchemeRowParameterId(parameter.id))
            };
            SchemeType::Record { fields, tail }
        }
        FullSchemeTypeTag::Union => SchemeType::Union(
            bounded(
                node.children,
                &store.scheme_children,
                "scheme union child range is invalid",
            )?
            .iter()
            .map(|child| nested(*child, budget))
            .collect::<Result<Vec<_>, _>>()?,
        ),
        FullSchemeTypeTag::Validated => {
            let base = nested(SchemeTypeId::from_raw(node.first)?, budget)?;
            let validation = verify_validation_row(
                store,
                *store
                    .scheme_validations
                    .get(node.second as usize)
                    .ok_or_else(|| IrVerifyError::new("scheme validation id is out of bounds"))?,
            )?;
            SchemeType::Validated(validation, Box::new(base))
        }
    };
    Ok(ty)
}

fn decode_scheme_schema(
    store: &FullStore,
    id: super::schemes::FullSchemeSchemaExpectationId,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<crate::sema::constants::SchemaExpectation, IrVerifyError> {
    use crate::sema::constants::{SchemaComponent, SchemaExpectation, SchemaInstance};
    use super::schema::FullSchemeSchemaComponentTag;
    if depth > 128 { return Err(IrVerifyError::new("scheme callable schema nesting is too deep")); }
    budget.charge(1)?;
    let schema = store.scheme_schema_expectations.get(id.index())
        .ok_or_else(|| IrVerifyError::new("scheme callable schema id is out of bounds"))?;
    let instances = bounded(schema.instances, &store.scheme_schema_instances, "scheme callable schema instance range is invalid")?
        .iter().map(|instance| {
            budget.charge(1)?;
            let arguments = bounded(instance.arguments, &store.scheme_schema_arguments, "scheme callable schema argument range is invalid")?
                .iter().map(|argument| store.semantic.to_type_with_budget(*argument, &mut budget.semantic_types))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SchemaInstance { definition: crate::syntax::arena::TypeDefId::from_index(instance.definition as usize), arguments })
        }).collect::<Result<Vec<_>, IrVerifyError>>()?;
    let mut children = BTreeMap::new();
    for child in bounded(schema.children, &store.scheme_schema_children, "scheme callable schema child range is invalid")? {
        let component = match child.component {
            FullSchemeSchemaComponentTag::Field => SchemaComponent::Field(Name::intern(store.string(child.name)?)),
            FullSchemeSchemaComponentTag::Item => SchemaComponent::Item,
            FullSchemeSchemaComponentTag::Key => SchemaComponent::Key,
            FullSchemeSchemaComponentTag::Value => SchemaComponent::Value,
            FullSchemeSchemaComponentTag::Optional => SchemaComponent::Optional,
            FullSchemeSchemaComponentTag::Success => SchemaComponent::Success,
            FullSchemeSchemaComponentTag::Error => SchemaComponent::Error,
        };
        let value = decode_scheme_schema(store, child.expectation, budget, depth + 1)?;
        if children.insert(component, value).is_some() { return Err(IrVerifyError::new("scheme callable schema repeats a child")); }
    }
    Ok(SchemaExpectation { instances, children: Arc::new(children) })
}

fn validate_symbolic_substitution_type(
    ty: &SchemeType,
    caller_scope: &SchemeScope,
    depth: usize,
) -> Result<(), IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("scheme substitution type nesting is too deep"));
    }
    match ty {
        SchemeType::Callable { monotype, effects, .. } => {
            effects::validate_contract(effects, &caller_scope.effects)?;
            for parameter in monotype.params() {
                validate_symbolic_substitution_type(&parameter.ty, caller_scope, depth + 1)?;
            }
            validate_symbolic_substitution_type(monotype.return_type(), caller_scope, depth + 1)
        }
        SchemeType::Ground(ty) => ensure_substitution_type(ty),
        SchemeType::Parameter(id) => caller_scope
            .type_binders
            .contains_key(&id.0)
            .then_some(())
            .ok_or_else(|| IrVerifyError::new("scheme substitution type binder is out of scope")),
        SchemeType::List(inner)
        | SchemeType::Stream(inner)
        | SchemeType::Set(inner)
        | SchemeType::Optional(inner)
        | SchemeType::Validated(_, inner) => {
            validate_symbolic_substitution_type(inner, caller_scope, depth + 1)
        }
        SchemeType::Map(key, value) | SchemeType::Result(key, value) => {
            validate_symbolic_substitution_type(key, caller_scope, depth + 1)?;
            validate_symbolic_substitution_type(value, caller_scope, depth + 1)
        }
        SchemeType::Record { fields, tail } => {
            for ty in fields.values() {
                validate_symbolic_substitution_type(ty, caller_scope, depth + 1)?;
            }
            if let SchemeRowTail::Parameter(id) = tail {
                let Some(&binder) = caller_scope.row_binders.get(&id.0) else {
                    return Err(IrVerifyError::new("scheme substitution row binder is out of scope"));
                };
                if fields
                    .keys()
                    .any(|name| !caller_scope.row_lacks[binder].contains(name))
                {
                    return Err(IrVerifyError::new(
                        "scheme substitution row lacks do not cover its explicit fields",
                    ));
                }
            }
            Ok(())
        }
        SchemeType::Union(members) => {
            for member in members {
                validate_symbolic_substitution_type(member, caller_scope, depth + 1)?;
            }
            Ok(())
        }
    }
}

fn substitute_scheme_type(
    ty: &SchemeType,
    callee_scope: &SchemeScope,
    substitution: &SymbolicCallSubstitution,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<SchemeType, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("generic call scheme substitution is too deep"));
    }
    budget.charge(1)?;
    let nested = |ty, budget: &mut SchemeInstantiationBudget| {
        substitute_scheme_type(ty, callee_scope, substitution, budget, depth + 1)
    };
    Ok(match ty {
        SchemeType::Callable { kind, monotype, effects } => {
            let params = monotype.params().iter().map(|parameter| {
                budget.charge(1)?;
                Ok(SchemeParameter {
                    name: parameter.name,
                    ty: nested(&parameter.ty, budget)?,
                    schema_expectation: parameter.schema_expectation.clone(),
                    defaulted: parameter.defaulted,
                    rest: parameter.rest,
                })
            }).collect::<Result<Vec<_>, IrVerifyError>>()?;
            SchemeType::Callable {
                kind: *kind,
                monotype: Arc::new(CallableMonotype::new(params, nested(monotype.return_type(), budget)?, monotype.return_schema().cloned())),
                effects: effects::substitute_contract(effects, callee_scope, &substitution.effects, budget)?,
            }
        }
        SchemeType::Ground(ty) => {
            budget.charge_clone(ty)?;
            SchemeType::Ground(ty.clone())
        }
        SchemeType::Parameter(id) => {
            let Some(&binder) = callee_scope.type_binders.get(&id.0) else {
                return Err(IrVerifyError::new("scheme type parameter is out of scope"));
            };
            let ty = &substitution.types[binder];
            budget.charge_scheme_clone(ty)?;
            ty.clone()
        }
        SchemeType::List(inner) => SchemeType::List(Box::new(nested(inner, budget)?)),
        SchemeType::Map(key, value) => SchemeType::Map(
            Box::new(nested(key, budget)?),
            Box::new(nested(value, budget)?),
        ),
        SchemeType::Stream(inner) => SchemeType::Stream(Box::new(nested(inner, budget)?)),
        SchemeType::Set(inner) => SchemeType::Set(Box::new(nested(inner, budget)?)),
        SchemeType::Result(ok, error) => SchemeType::Result(
            Box::new(nested(ok, budget)?),
            Box::new(nested(error, budget)?),
        ),
        SchemeType::Optional(inner) => SchemeType::Optional(Box::new(nested(inner, budget)?)),
        SchemeType::Record { fields, tail } => {
            let mut substituted = fields
                .iter()
                .map(|(&name, ty)| Ok((name, nested(ty, budget)?)))
                .collect::<Result<BTreeMap<_, _>, IrVerifyError>>()?;
            let tail = match tail {
                SchemeRowTail::Closed => SchemeRowTail::Closed,
                SchemeRowTail::Parameter(id) => {
                    let Some(&binder) = callee_scope.row_binders.get(&id.0) else {
                        return Err(IrVerifyError::new("scheme row parameter is out of scope"));
                    };
                    let evidence = &substitution.rows[binder];
                    for (&name, ty) in &evidence.fields {
                        budget.charge(1)?;
                        budget.charge_scheme_clone(ty)?;
                        if substituted.insert(name, ty.clone()).is_some() {
                            return Err(IrVerifyError::new(
                                "generic row evidence repeats a required field",
                            ));
                        }
                    }
                    evidence.tail
                }
            };
            SchemeType::Record {
                fields: substituted,
                tail,
            }
        }
        SchemeType::Union(members) => SchemeType::Union(
            members
                .iter()
                .map(|member| nested(member, budget))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        SchemeType::Validated(validation, base) => {
            SchemeType::Validated(*validation, Box::new(nested(base, budget)?))
        }
    })
}

fn verify_symbolic_call_arguments(
    store: &FullStore,
    instruction: usize,
    target: &FullFunction,
    scheme: &FullScheme,
    callee_scope: &SchemeScope,
    caller_scheme: FullSchemeId,
    caller_scope: &SchemeScope,
    substitution: &SymbolicCallSubstitution,
    budget: &mut SchemeInstantiationBudget,
    function_signatures: &Arc<super::FunctionSignatureIndex>,
) -> Result<(), IrVerifyError> {
    let decoder = decoder_for_instruction(store, instruction, function_signatures)?;
    let args = match decoder.expression_view(instruction)? {
        schema::ExprView::Call { args, .. }
        | schema::ExprView::DirectPureCall { args, .. }
        | schema::ExprView::SelfCall { args, .. } => args,
        _ => return Err(IrVerifyError::new("generic call instruction has an invalid view")),
    };
    let arguments = args.collect::<Result<Vec<_>, _>>()?;
    let params = bounded(target.params, &store.params, "generic target parameter range is invalid")?;
    let scheme_params = bounded(scheme.params, &store.scheme_params, "scheme parameter range is invalid")?;
    if params.len() != scheme_params.len() {
        return Err(IrVerifyError::new("generic call parameters do not match checked parameter slots"));
    }
    let rest = params.iter().position(|parameter| parameter.flags & 1 != 0);
    if rest.is_some_and(|slot| slot + 1 != params.len()) { return Err(IrVerifyError::new("generic rest parameter is not final")); }
    let fixed = rest.unwrap_or(params.len());
    let required = params[..fixed].iter().rposition(|parameter| parameter.flags & 2 == 0).map_or(0, |slot| slot + 1);
    if arguments.len() < required || rest.is_none() && arguments.len() > fixed {
        return Err(IrVerifyError::new("generic call arguments do not match checked parameter slots"));
    }
    for (slot, argument) in arguments.iter().enumerate() {
        budget.charge(1)?;
        let parameter = &scheme_params[slot.min(fixed)];
        let expected = decode_scheme_type(store, callee_scope, parameter.ty, budget, 0)?;
        let mut expected = substitute_scheme_type(&expected, callee_scope, substitution, budget, 0)?;
        let value = match argument {
            super::decode::CallArgView::Default(actual) if slot < fixed && *actual as usize == slot && params[slot].flags & 2 != 0 => continue,
            super::decode::CallArgView::Default(_) => return Err(IrVerifyError::new("generic default argument has no matching parameter")),
            super::decode::CallArgView::Single(value) if slot < fixed => value,
            super::decode::CallArgView::Single(value) => {
                expected = match expected {
                    SchemeType::List(item) => *item,
                    SchemeType::Ground(Type::List(item)) => SchemeType::Ground(*item),
                    _ => return Err(IrVerifyError::new("generic rest parameter has no list domain")),
                };
                value
            }
            super::decode::CallArgView::Splice(value) if slot >= fixed && rest.is_some() => value,
            super::decode::CallArgView::Splice(_) => return Err(IrVerifyError::new("generic fixed parameter cannot be supplied by a splice")),
        };
        let actual = scheme_body_fact_type(store, caller_scheme, value.raw(), FullSchemeBodyFactKind::Expression)?.ok_or_else(|| IrVerifyError::new("generic call argument has no checked scheme fact"))?;
        let actual = decode_scheme_type(store, caller_scope, actual, budget, 0)?;
        let matches = actual == expected || matches!((actual.closed_ground_type(), expected.closed_ground_type()), (Some(actual), Some(expected)) if actual.matches_expected(&expected));
        if !matches { return Err(IrVerifyError::new("generic call argument does not match its checked scheme fact")); }
    }
    Ok(())
}

fn verify_instantiated_call_arguments(
    store: &FullStore,
    instruction: usize,
    target: &FullFunction,
    scheme: &FullScheme,
    scope: &SchemeScope,
    substitution: &CallSubstitution,
    materialization: &mut SchemeInstantiationBudget,
    function_signatures: &Arc<super::FunctionSignatureIndex>,
) -> Result<Vec<producers::GroundEmptyArgumentRequest>, IrVerifyError> {
    let decoder = decoder_for_instruction(store, instruction, function_signatures)?;
    let view = decoder.expression_view(instruction)?;
    let args = match view {
        schema::ExprView::Call { args, .. }
        | schema::ExprView::DirectPureCall { args, .. }
        | schema::ExprView::SelfCall { args, .. } => args,
        _ => return Err(IrVerifyError::new("generic call instruction has an invalid view")),
    };
    let mut arguments = Vec::new();
    for arg in args {
        arguments.push(arg?);
    }
    let params = bounded(target.params, &store.params, "generic target parameter range is invalid")?;
    let scheme_params = bounded(
        scheme.params,
        &store.scheme_params,
        "scheme parameter range is invalid",
    )?;
    if params.len() != scheme_params.len() {
        return Err(IrVerifyError::new(
            "generic call parameters do not match checked parameter slots",
        ));
    }
    let rest_index = params.iter().position(|param| param.flags & 1 != 0);
    if rest_index.is_some_and(|index| index + 1 != params.len()) {
        return Err(IrVerifyError::new("generic rest parameter is not final"));
    }
    let fixed_count = rest_index.unwrap_or(params.len());
    let required = params[..fixed_count].iter().rposition(|parameter| parameter.flags & 2 == 0).map_or(0, |slot| slot + 1);
    if arguments.len() < required || (rest_index.is_none() && arguments.len() > params.len()) {
        return Err(IrVerifyError::new("generic call arguments do not match checked parameter slots"));
    }
    let rest_item = rest_index
        .map(|index| {
            scheme_list_item(
                store,
                scheme_params[index].ty,
                &mut materialization.semantic_types,
            )
        })
        .transpose()?;
    let mut checked_arguments = Vec::with_capacity(arguments.len());
    let mut ground_arguments = Vec::new();
    for (index, argument) in arguments.iter().enumerate() {
        let (value, expected_scheme_type, expected_ground_type, is_splice) = match argument {
            super::decode::CallArgView::Default(slot) if index < fixed_count && *slot as usize == index && params[index].flags & 2 != 0 => continue,
            super::decode::CallArgView::Default(_) => return Err(IrVerifyError::new("generic default argument has no matching parameter")),
            super::decode::CallArgView::Single(value) if index < fixed_count => {
                (value, Some(scheme_params[index].ty), None, false)
            }
            super::decode::CallArgView::Single(value) => {
                let item = rest_item.as_ref().ok_or_else(|| {
                    IrVerifyError::new("generic extra argument has no rest parameter")
                })?;
                match item {
                    SchemeTypeExpectation::Encoded(id) => (value, Some(*id), None, false),
                    SchemeTypeExpectation::Ground(ty) => (value, None, Some(ty), false),
                }
            }
            super::decode::CallArgView::Splice(_) if index < fixed_count => {
                return Err(IrVerifyError::new(
                    "generic fixed parameter cannot be supplied by a splice",
                ));
            }
            super::decode::CallArgView::Splice(value) => {
                let rest = rest_index
                    .ok_or_else(|| IrVerifyError::new("generic splice has no rest parameter"))?;
                (value, Some(scheme_params[rest].ty), None, true)
            }
        };
        let expected = expected_scheme_type
            .map(|id| {
                instantiate_scheme_type(
                    store,
                    scope,
                    id,
                    substitution,
                    materialization,
                    0,
                )
            })
            .transpose()?;
        let expected = expected
            .or_else(|| expected_ground_type.cloned())
            .ok_or_else(|| IrVerifyError::new("generic call argument type is invalid"))?;
        let actual = checked_argument_type(
            store,
            &decoder,
            instruction,
            value.raw() as usize,
            materialization,
            0,
        )?
        .ok_or_else(|| {
            IrVerifyError::new("generic call argument has no closed checked type")
        })?;
        if matches!(actual, CheckedArgumentType::EmptyList(None) | CheckedArgumentType::EmptyMap) {
            materialization.charge_clone(&expected)?;
            materialization.charge(1)?;
            ground_arguments.push(producers::GroundEmptyArgumentRequest {
                owner: decoder.owner, call: instruction, argument: value.raw() as usize, expected: expected.clone(),
            });
        }
        checked_arguments.push((expected, expected_scheme_type, is_splice, actual));
    }
    let mut binder_evidence = SchemeBinderEvidence::default();
    for (expected, expected_scheme_type, is_splice, actual) in &checked_arguments {
        match actual {
            CheckedArgumentType::Known(actual) => {
                ensure_closed_type(actual)?;
                if !actual.matches_expected(expected) {
                    return Err(IrVerifyError::new(
                        "generic call argument does not match its checked parameter slot",
                    ));
                }
                if let Some(id) = expected_scheme_type {
                    verify_scheme_argument_evidence(
                        store,
                        scope,
                        *id,
                        actual,
                        substitution,
                        &mut binder_evidence,
                        materialization,
                        0,
                    )?;
                }
            }
            CheckedArgumentType::ListItems(items) if *is_splice => {
                let expected_item = expected_list_item(expected, !items.is_empty())
                    .ok_or_else(|| {
                        IrVerifyError::new(
                            "generic list literal does not match a list parameter",
                        )
                    })?;
                for item in items {
                    ensure_closed_type(item)?;
                    if !item.matches_expected(expected_item) {
                        return Err(IrVerifyError::new(
                            "generic list item does not match its checked parameter slot",
                        ));
                    }
                }
                if let Some(id) = expected_scheme_type
                    && let SchemeTypeExpectation::Encoded(item_id) = scheme_list_item(
                        store,
                        *id,
                        &mut materialization.semantic_types,
                    )?
                {
                    for item in items {
                        verify_scheme_argument_evidence(
                            store,
                            scope,
                            item_id,
                            item,
                            substitution,
                            &mut binder_evidence,
                            materialization,
                            0,
                        )?;
                    }
                }
            }
            CheckedArgumentType::ListItems(items) => {
                let expected_item = expected_list_item(expected, !items.is_empty())
                    .ok_or_else(|| {
                        IrVerifyError::new(
                            "generic list literal does not match a list parameter",
                        )
                    })?;
                for item in items {
                    ensure_closed_type(item)?;
                    if !item.matches_expected(expected_item) {
                        return Err(IrVerifyError::new(
                            "generic list item does not match its checked parameter slot",
                        ));
                    }
                }
                if let Some(id) = expected_scheme_type {
                    if let Some(item_scheme) = scheme_list_item_for_argument(
                        store,
                        *id,
                        &mut materialization.semantic_types,
                    )? {
                        if let SchemeTypeExpectation::Encoded(item_id) = item_scheme {
                            for item in items {
                                verify_scheme_argument_evidence(
                                    store,
                                    scope,
                                    item_id,
                                    item,
                                    substitution,
                                    &mut binder_evidence,
                                    materialization,
                                    0,
                                )?;
                            }
                        }
                    } else {
                        let actual = merge_list_item_types(items, materialization)?.ok_or_else(|| {
                            IrVerifyError::new("generic list literal has no closed checked type")
                        })?;
                        verify_scheme_argument_evidence(
                            store,
                            scope,
                            *id,
                            &actual,
                            substitution,
                            &mut binder_evidence,
                            materialization,
                            0,
                        )?;
                    }
                }
            }
            CheckedArgumentType::EmptyList(Some(actual)) => {
                ensure_closed_type(actual)?;
                if !actual.matches_expected(expected) {
                    return Err(IrVerifyError::new(
                        "generic empty list argument does not match its checked parameter slot",
                    ));
                }
                if let Some(id) = expected_scheme_type {
                    verify_scheme_argument_evidence(
                        store,
                        scope,
                        *id,
                        actual,
                        substitution,
                        &mut binder_evidence,
                        materialization,
                        0,
                    )?;
                }
            }
            CheckedArgumentType::EmptyList(None) | CheckedArgumentType::EmptyMap => {}
        }
    }
    applications::extend_ground_binder_evidence(store, scheme, &mut binder_evidence, materialization)?;
    for (expected, expected_scheme_type, _, actual) in &checked_arguments {
        let declared = match actual {
            CheckedArgumentType::EmptyList(declared) => {
                if expected_list_item(expected, false).is_none() {
                    return Err(IrVerifyError::new("empty generic list argument has no concrete list contract"));
                }
                declared.as_ref()
            }
            CheckedArgumentType::EmptyMap => {
                if !matches!(expected, Type::Map(_, _)) {
                    return Err(IrVerifyError::new("empty generic map argument has no concrete map contract"));
                }
                None
            }
            _ => continue,
        };
        if let Some(id) = expected_scheme_type {
            let (required_types, required_rows) = scheme_type_binders(
                store,
                *id,
                materialization,
            )?;
            if !required_types.is_subset(&binder_evidence.types)
                || !required_rows.is_subset(&binder_evidence.rows)
            {
                return Err(IrVerifyError::new(
                    "empty generic collection argument has no independently checked binder evidence",
                ));
            }
        }
        if let Some(declared) = declared {
            ensure_closed_type(declared)?;
            if !matches!(declared, Type::List(_)) || !declared.matches_expected(expected) {
                return Err(IrVerifyError::new(
                    "empty list initializer disagrees with its checked slot type",
                ));
            }
        }
    }
    let required_types = (0..scope.type_parameters.len()).collect::<BTreeSet<_>>();
    let required_rows = (0..scope.row_parameters.len()).collect::<BTreeSet<_>>();
    if !required_types.is_subset(&binder_evidence.types)
        || !required_rows.is_subset(&binder_evidence.rows)
    {
        return Err(IrVerifyError::new(
            "generic call substitution has no independently checked argument evidence",
        ));
    }
    Ok(ground_arguments)
}

fn scheme_list_item(
    store: &FullStore,
    id: SchemeTypeId,
    materialization: &mut TypeMaterializationBudget,
) -> Result<SchemeTypeExpectation, IrVerifyError> {
    let mut current = id;
    for _ in 0..=128 {
        let node = store.scheme_type(current)?;
        match node.tag {
            FullSchemeTypeTag::List => {
                return Ok(SchemeTypeExpectation::Encoded(SchemeTypeId::from_raw(
                    node.first,
                )?));
            }
            FullSchemeTypeTag::Validated => current = SchemeTypeId::from_raw(node.first)?,
            FullSchemeTypeTag::Ground => {
                let type_id = TypeId::from_raw(node.first)
                    .ok_or_else(|| IrVerifyError::new("scheme ground type id is invalid"))?;
                let ty = store
                    .semantic
                    .to_type_with_budget(type_id, materialization)?
                    .into_unvalidated();
                return match ty {
                    Type::List(item) => Ok(SchemeTypeExpectation::Ground(*item)),
                    _ => Err(IrVerifyError::new(
                        "generic rest parameter does not have a List type",
                    )),
                };
            }
            _ => {
                return Err(IrVerifyError::new(
                    "generic rest parameter does not have a List type",
                ));
            }
        }
    }
    Err(IrVerifyError::new("generic rest parameter type is too deep"))
}

fn scheme_list_item_for_argument(
    store: &FullStore,
    id: SchemeTypeId,
    materialization: &mut TypeMaterializationBudget,
) -> Result<Option<SchemeTypeExpectation>, IrVerifyError> {
    let mut current = id;
    for _ in 0..=128 {
        let node = store.scheme_type(current)?;
        match node.tag {
            FullSchemeTypeTag::List => {
                return scheme_list_item(store, id, materialization).map(Some);
            }
            FullSchemeTypeTag::Ground => {
                let type_id = TypeId::from_raw(node.first)
                    .ok_or_else(|| IrVerifyError::new("scheme ground type id is invalid"))?;
                let actual = store
                    .semantic
                    .to_type_with_budget(type_id, materialization)?
                    .into_unvalidated();
                return Ok(match actual {
                    Type::List(item) => Some(SchemeTypeExpectation::Ground(*item)),
                    _ => None,
                });
            }
            FullSchemeTypeTag::Validated => {
                current = SchemeTypeId::from_raw(node.first)?;
            }
            _ => return Ok(None),
        }
    }
    Err(IrVerifyError::new("generic list argument type is too deep"))
}

fn verify_scheme_list_shape(store: &FullStore, id: SchemeTypeId) -> Result<(), IrVerifyError> {
    let mut current = id;
    for _ in 0..=128 {
        let node = store.scheme_type(current)?;
        match node.tag {
            FullSchemeTypeTag::List => return Ok(()),
            FullSchemeTypeTag::Validated => current = SchemeTypeId::from_raw(node.first)?,
            FullSchemeTypeTag::Ground => {
                let type_id = TypeId::from_raw(node.first)
                    .ok_or_else(|| IrVerifyError::new("scheme ground type id is invalid"))?;
                return store
                    .semantic
                    .list_item_type_id(type_id)?
                    .map(|_| ())
                    .ok_or_else(|| {
                        IrVerifyError::new("generic rest parameter does not have a List type")
                    });
            }
            _ => {
                return Err(IrVerifyError::new(
                    "generic rest parameter does not have a List type",
                ));
            }
        }
    }
    Err(IrVerifyError::new("generic rest parameter type is too deep"))
}

enum ArgumentAliasCheck {
    Annotation(TypeId),
    Declared(TypeId),
}

fn checked_argument_type(
    store: &FullStore,
    decoder: &FullDecoder<'_>,
    mut use_instruction: usize,
    mut instruction: usize,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<Option<CheckedArgumentType>, IrVerifyError> {
    let mut owned_decoder = None;
    let mut aliases = BTreeSet::new();
    let mut checks = Vec::new();
    let actual = loop {
        let current = owned_decoder.as_ref().unwrap_or(decoder);
        materialization.charge(1)?;
        if !aliases.insert((current.owner, instruction)) {
            return Err(IrVerifyError::new("generic argument alias chain contains a cycle"));
        }
        if store.tags.get(instruction) == Some(&FullTag::ExprParam)
            && store.call_instantiations.binary_search_by_key(&(instruction as u32), |instance| instance.instruction).is_ok()
        {
            let type_id = checked_expression_type_id(store, instruction as u32)
                .ok_or_else(|| IrVerifyError::new("materialized callable argument has no checked type"))?;
            let actual = store.semantic.to_type_with_budget(type_id, &mut materialization.semantic_types)?;
            ensure_closed_type(&actual)?;
            break Some(CheckedArgumentType::Known(actual));
        }
        let schema::ExprView::Param(slot) = current.expression_view(instruction)? else {
            break checked_argument_type_inner(
                store, current, use_instruction, instruction, materialization, depth,
            )?;
        };
        if super::driver_owner_index(current.owner).is_some()
            && let Some(source) = current.driver_local_let_source(
                slot,
                |amount| materialization.charge(amount),
            )?
        {
            let mut next = decoder_for_driver_step(store, source.step)?;
            next.function_signature_index.clone_from(&current.function_signature_index);
            if !next.instruction_range.contains(&source.initializer) {
                return Err(IrVerifyError::new(
                    "driver local initializer belongs to another step",
                ));
            }
            if let Some(annotation) = source.annotation {
                let annotated = store.semantic.to_type_with_budget(
                    annotation, &mut materialization.semantic_types,
                )?;
                let checked = store.semantic.to_type_with_budget(
                    source.checked_type, &mut materialization.semantic_types,
                )?;
                if checked != annotated {
                    return Err(IrVerifyError::new(
                        "driver slot type fact disagrees with its declaration annotation",
                    ));
                }
                verify_driver_annotation_runtime_type(
                    store, source.runtime_type, source.validation_type, &annotated, materialization,
                )?;
                checks.push(ArgumentAliasCheck::Annotation(annotation));
            } else {
                checks.push(ArgumentAliasCheck::Declared(source.checked_type));
            }
            instruction = source.initializer;
            use_instruction = source.initializer;
            owned_decoder = Some(next);
            continue;
        }
        let (has_local_write, initializer) = current.local_immutable_let_initializer(slot)?;
        let Some(initializer) = initializer.filter(|_| has_local_write) else {
            break checked_argument_type_inner(
                store, current, use_instruction, instruction, materialization, depth,
            )?;
        };
        let annotation = current.local_immutable_let_annotation(slot)?;
        let slot_type = current.slot_type_fact(slot)?;
        if let Some(annotation) = annotation {
            let annotated = store.semantic.to_type_with_budget(
                annotation, &mut materialization.semantic_types,
            )?;
            if let Some(slot_type) = slot_type {
                let checked = store.semantic.to_type_with_budget(
                    slot_type, &mut materialization.semantic_types,
                )?;
                if checked != annotated {
                    return Err(IrVerifyError::new(
                        "local slot type fact disagrees with its declaration annotation",
                    ));
                }
            } else if annotated != Type::Any {
                return Err(IrVerifyError::new("annotated local has no checked slot type"));
            }
            checks.push(ArgumentAliasCheck::Annotation(annotation));
        } else if let Some(slot_type) = slot_type {
            checks.push(ArgumentAliasCheck::Declared(slot_type));
        }
        instruction = initializer;
    };
    let Some(mut actual) = actual else {
        return Ok(None);
    };
    for check in checks.into_iter().rev() {
        let type_id = match check {
            ArgumentAliasCheck::Annotation(type_id) | ArgumentAliasCheck::Declared(type_id) => type_id,
        };
        let declared = store.semantic.to_type_with_budget(
            type_id, &mut materialization.semantic_types,
        )?;
        actual = match check {
            ArgumentAliasCheck::Annotation(_) => {
                validate_annotated_initializer_type(actual, &declared, materialization)?
            }
            ArgumentAliasCheck::Declared(_) => match actual {
                CheckedArgumentType::Known(actual) if actual.matches_expected(&declared) => {
                    CheckedArgumentType::Known(actual)
                }
                CheckedArgumentType::ListItems(items) => {
                    let Some(expected_item) = expected_list_item(&declared, !items.is_empty()) else {
                        return Ok(None);
                    };
                    if !items.iter().all(|item| item.matches_expected(expected_item)) {
                        return Ok(None);
                    }
                    CheckedArgumentType::ListItems(items)
                }
                CheckedArgumentType::EmptyList(_) => CheckedArgumentType::EmptyList(None),
                CheckedArgumentType::EmptyMap => CheckedArgumentType::EmptyMap,
                _ => return Ok(None),
            },
        };
    }
    Ok(Some(actual))
}

fn closed_literal_argument_type(actual: CheckedArgumentType, materialization: &mut SchemeInstantiationBudget) -> Result<Option<Type>, IrVerifyError> {
    match actual {
        CheckedArgumentType::Known(actual) | CheckedArgumentType::EmptyList(Some(actual)) => {
            ensure_closed_type(&actual)?;
            Ok(Some(actual))
        }
        CheckedArgumentType::ListItems(items) => merge_list_item_types(&items, materialization),
        CheckedArgumentType::EmptyList(None) | CheckedArgumentType::EmptyMap => Ok(None),
    }
}

fn checked_argument_type_inner(
    store: &FullStore,
    decoder: &FullDecoder<'_>,
    use_instruction: usize,
    instruction: usize,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<Option<CheckedArgumentType>, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new(
            "generic call argument nesting is too deep",
        ));
    }
    materialization.charge(1)?;
    if let schema::ExprView::CheckedValue { check, .. } =
        decoder.expression_view(instruction)?
    {
        if check.schema.is_some() {
            return Ok(None);
        }
        let checked_type = store
            .semantic
            .to_type_with_budget(check.ty.id, &mut materialization.semantic_types)?;
        ensure_closed_type(&checked_type)?;
        if !runtime_complete_value_check_type(&checked_type, materialization, 0)? {
            return Ok(None);
        }
        return Ok(Some(CheckedArgumentType::Known(checked_type)));
    }
    if decoder.is_empty_list_literal_expression(instruction)? {
        return Ok(Some(CheckedArgumentType::EmptyList(None)));
    }
    match decoder.expression_view(instruction)? {
        schema::ExprView::EmptyMap => return Ok(Some(CheckedArgumentType::EmptyMap)),
        schema::ExprView::MapLiteral(mut entries) => {
            let mut key_type = None;
            let mut values = Vec::new();
            while entries.len() != 0 {
                materialization.charge(1)?;
                let (key, value, _) = entries.next().transpose()?.ok_or_else(|| IrVerifyError::new("map literal entry is missing"))?;
                let Some(value) = checked_argument_type(store, decoder, use_instruction, value.raw() as usize, materialization, depth + 1)? else { return Ok(None) };
                let (key, value) = if let Some(key) = key {
                    let Some(key) = checked_argument_type(store, decoder, use_instruction, key.raw() as usize, materialization, depth + 1)? else { return Ok(None) };
                    let Some(key) = closed_literal_argument_type(key, materialization)? else { return Ok(None) };
                    let Some(value) = closed_literal_argument_type(value, materialization)? else { return Ok(None) };
                    (key, value)
                } else {
                    if matches!(value, CheckedArgumentType::EmptyMap) { continue; }
                    let Some(value) = closed_literal_argument_type(value, materialization)? else { return Ok(None) };
                    let Type::Map(key, value) = value.into_unvalidated() else { return Ok(None) };
                    (*key, *value)
                };
                if !key.is_map_key() { return Ok(None); }
                if let Some(expected) = &key_type {
                    if !initializer_type_matches_annotation(&key, expected, materialization, 0)? { return Ok(None); }
                } else { key_type = Some(key); }
                values.push(value);
            }
            let Some(key) = key_type else { return Ok(Some(CheckedArgumentType::EmptyMap)) };
            let Some(Type::List(value)) = merge_list_item_types(&values, materialization)? else { return Ok(None) };
            for actual in &values {
                if !initializer_type_matches_annotation(actual, &value, materialization, 0)? { return Ok(None); }
            }
            ensure_closed_type(&value)?;
            materialization.charge(3)?;
            return Ok(Some(CheckedArgumentType::Known(Type::Map(Box::new(key), value))));
        }
        schema::ExprView::List(mut values) => {
            let mut item_types = Vec::new();
            for value in &mut values {
                let Some(item) = checked_argument_type(
                    store,
                    decoder,
                    use_instruction,
                    value?.raw() as usize,
                    materialization,
                    depth + 1,
                )?
                else {
                    return Ok(None);
                };
                match item {
                    CheckedArgumentType::Known(ty) => item_types.push(ty),
                    CheckedArgumentType::ListItems(items) => {
                        let Some(ty) = merge_list_item_types(&items, materialization)? else {
                            return Ok(None);
                        };
                        item_types.push(ty);
                    }
                    CheckedArgumentType::EmptyList(_) | CheckedArgumentType::EmptyMap => return Ok(None),
                }
            }
            return Ok(Some(CheckedArgumentType::ListItems(item_types)));
        }
        schema::ExprView::ListBuild(mut values) => {
            let mut item_types = Vec::new();
            for entry in &mut values {
                let (splice, value, _) = entry?;
                let Some(actual) = checked_argument_type(
                    store,
                    decoder,
                    use_instruction,
                    value.raw() as usize,
                    materialization,
                    depth + 1,
                )?
                else {
                    return Ok(None);
                };
                match (splice, actual) {
                    (true, CheckedArgumentType::EmptyList(None)) => {}
                    (true, CheckedArgumentType::EmptyList(Some(actual))) => {
                        let Type::List(item) = actual.unvalidated() else {
                            return Ok(None);
                        };
                        materialization.charge_clone(item)?;
                        item_types.push(item.as_ref().clone());
                    }
                    (true, CheckedArgumentType::Known(actual)) => {
                        let Type::List(item) = actual.unvalidated() else {
                            return Ok(None);
                        };
                        materialization.charge_clone(item)?;
                        item_types.push(item.as_ref().clone());
                    }
                    (true, CheckedArgumentType::ListItems(items)) => {
                        let Some(Type::List(item)) =
                            merge_list_item_types(&items, materialization)?
                        else {
                            if items.is_empty() {
                                continue;
                            }
                            return Ok(None);
                        };
                        item_types.push(*item);
                    }
                    (false, CheckedArgumentType::Known(actual)) => item_types.push(actual),
                    (false, CheckedArgumentType::ListItems(items)) => {
                        let Some(actual) = merge_list_item_types(&items, materialization)? else {
                            return Ok(None);
                        };
                        item_types.push(actual);
                    }
                    (false, CheckedArgumentType::EmptyList(Some(actual))) => {
                        item_types.push(actual);
                    }
                    (false, CheckedArgumentType::EmptyList(None)) | (_, CheckedArgumentType::EmptyMap) => return Ok(None),
                }
            }
            if item_types.is_empty() {
                return Ok(Some(CheckedArgumentType::EmptyList(None)));
            }
            return Ok(Some(CheckedArgumentType::ListItems(item_types)));
        }
        schema::ExprView::Param(slot) => {
            if super::driver_owner_index(decoder.owner).is_none() {
                let (has_local_write, initializer) = decoder.local_immutable_let_initializer(slot)?;
                if has_local_write {
                    if initializer.is_some() {
                        return Err(IrVerifyError::new("generic argument alias was not resolved"));
                    }
                    let Some(actual) = decoder.local_immutable_let_scalar_type(slot)? else {
                        return Ok(None);
                    };
                    materialization.charge_clone(&actual)?;
                    if let Some(slot_type) = decoder.checked_slot_type(slot)?
                        && store.semantic.to_type_with_budget(
                            slot_type,
                            &mut materialization.semantic_types,
                        )? != actual
                    {
                        return Err(IrVerifyError::new(
                            "local scalar writer disagrees with its checked slot type",
                        ));
                    }
                    return Ok(Some(CheckedArgumentType::Known(actual)));
                }
            }
            let initializer = {
                let mut charge = |amount| materialization.charge(amount);
                decoder.match_bind_initializer_for_use(use_instruction, slot, &mut charge)?
            };
            if let Some(initializer) = initializer {
                return checked_argument_type(
                    store,
                    decoder,
                    use_instruction,
                    initializer,
                    materialization,
                    depth + 1,
                );
            }
            if super::driver_owner_index(decoder.owner).is_none()
                && let Some(type_id) = decoder.checked_slot_type(slot)?
            {
                return store
                    .semantic
                    .to_type_with_budget(type_id, &mut materialization.semantic_types)
                    .map(|ty| Some(CheckedArgumentType::Known(ty)));
            }
        }
        schema::ExprView::Field { base, name, .. } => {
            let base_instruction = base.raw() as usize;
            if decoder.instruction_range.contains(&instruction)
                && decoder.instruction_range.contains(&base_instruction)
                && decoder.store.tags.get(base_instruction) == Some(&FullTag::ExprParam)
            {
                let Some(CheckedArgumentType::Known(base_type)) = checked_argument_type(
                    store,
                    decoder,
                    use_instruction,
                    base_instruction,
                    materialization,
                    depth + 1,
                )?
                else {
                    return Ok(None);
                };
                ensure_closed_type(&base_type)?;
                let Type::Record(fields) = base_type.into_unvalidated() else {
                    return Ok(None);
                };
                let field = Name::intern(name);
                let Some(field_type) = fields.get(&field) else {
                    return Ok(None);
                };
                materialization.charge_clone(field_type)?;
                return Ok(Some(CheckedArgumentType::Known(field_type.clone())));
            }
        }
        _ => {}
    }
    let instruction = u32::try_from(instruction)
        .map_err(|_| IrVerifyError::new("checked argument instruction is out of bounds"))?;
    if let Some(type_id) = checked_expression_type_id(store, instruction) {
        return store
            .semantic
            .to_type_with_budget(type_id, &mut materialization.semantic_types)
            .map(|ty| Some(CheckedArgumentType::Known(ty)));
    }
    decoder
        .derive_checked_expression_type(instruction as usize)
        .map(|ty| ty.map(CheckedArgumentType::Known))
}

fn verify_driver_annotation_runtime_type(
    store: &FullStore,
    runtime_type: Option<TypeId>,
    validation_type: Option<TypeId>,
    annotation: &Type,
    materialization: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let runtime_type = runtime_type
        .ok_or_else(|| IrVerifyError::new("annotated driver local has no runtime type"))?;
    let runtime_type = store
        .semantic
        .to_type_with_budget(runtime_type, &mut materialization.semantic_types)?;
    let annotation_kind = super::lowered_type_from_type(annotation)?;
    if super::lowered_type_from_type(&runtime_type)? != annotation_kind {
        return Err(IrVerifyError::new(
            "driver annotation disagrees with its runtime type",
        ));
    }
    if let Some(validation_type) = validation_type {
        let validation = store
            .semantic
            .to_type_with_budget(validation_type, &mut materialization.semantic_types)?;
        if validation != *annotation {
            return Err(IrVerifyError::new(
                "driver annotation disagrees with its runtime validation",
            ));
        }
    }
    Ok(())
}

fn validate_annotated_initializer_type(
    actual: CheckedArgumentType,
    annotation: &Type,
    materialization: &mut SchemeInstantiationBudget,
) -> Result<CheckedArgumentType, IrVerifyError> {
    let matches = match actual {
        CheckedArgumentType::Known(actual) => {
            ensure_closed_type(&actual)?;
            initializer_type_matches_annotation(&actual, annotation, materialization, 0)?
        }
        CheckedArgumentType::ListItems(items) => {
            let Some(expected_item) = expected_list_item(annotation, !items.is_empty()) else {
                return Err(IrVerifyError::new("local initializer disagrees with its declaration annotation"));
            };
            let mut matches = true;
            for item in &items {
                ensure_closed_type(item)?;
                if !initializer_type_matches_annotation(
                    item,
                    expected_item,
                    materialization,
                    0,
                )? {
                    matches = false;
                    break;
                }
            }
            matches
        }
        CheckedArgumentType::EmptyList(Some(actual)) => {
            ensure_closed_type(&actual)?;
            matches!(&actual, Type::List(_))
                && initializer_type_matches_annotation(
                    &actual,
                    annotation,
                    materialization,
                    0,
                )?
        }
        CheckedArgumentType::EmptyList(None) => matches!(annotation, Type::List(_)),
        CheckedArgumentType::EmptyMap => matches!(annotation, Type::Map(_, _)),
    };
    if !matches {
        return Err(IrVerifyError::new(
            "local initializer disagrees with its declaration annotation",
        ));
    }
    materialization.charge_clone(annotation)?;
    Ok(CheckedArgumentType::Known(annotation.clone()))
}

fn initializer_type_matches_annotation(
    actual: &Type,
    annotation: &Type,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<bool, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new(
            "annotated initializer type nesting is too deep",
        ));
    }
    materialization.charge(1)?;
    match (actual, annotation) {
        (Type::Int, Type::UInt) => Ok(false),
        (Type::Validated(actual), Type::Validated(annotation)) => {
            Ok(actual.validation().implies(annotation.validation())
                && initializer_type_matches_annotation(
                    actual.base(),
                    annotation.base(),
                    materialization,
                    depth + 1,
                )?)
        }
        (Type::Validated(actual), annotation) => initializer_type_matches_annotation(
            actual.base(),
            annotation,
            materialization,
            depth + 1,
        ),
        (Type::Union(actual), Type::Union(annotation)) => {
            for actual_member in actual {
                let mut matched = false;
                for annotation_member in annotation {
                    if initializer_type_matches_annotation(
                        actual_member,
                        annotation_member,
                        materialization,
                        depth + 1,
                    )? {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (actual, Type::Union(members)) => {
            for member in members {
                if initializer_type_matches_annotation(
                    actual,
                    member,
                    materialization,
                    depth + 1,
                )? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        (Type::Union(members), annotation) => {
            for member in members {
                if !initializer_type_matches_annotation(
                    member,
                    annotation,
                    materialization,
                    depth + 1,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Type::List(actual), Type::List(annotation))
        | (Type::Stream(actual), Type::Stream(annotation))
        | (Type::Set(actual), Type::Set(annotation))
        | (Type::Optional(actual), Type::Optional(annotation)) => {
            initializer_type_matches_annotation(
                actual,
                annotation,
                materialization,
                depth + 1,
            )
        }
        (Type::Map(actual_key, actual_value), Type::Map(annotation_key, annotation_value)) => {
            Ok(initializer_types_match_invariant(
                actual_key,
                annotation_key,
                materialization,
                depth + 1,
            )? && initializer_type_matches_annotation(
                actual_value,
                annotation_value,
                materialization,
                depth + 1,
            )?)
        }
        (Type::Null, Type::Optional(_)) => Ok(true),
        (actual, Type::Optional(annotation)) => initializer_type_matches_annotation(
            actual,
            annotation,
            materialization,
            depth + 1,
        ),
        (Type::Result(actual_ok, actual_error), Type::Result(annotation_ok, annotation_error)) => {
            Ok(initializer_type_matches_annotation(
                actual_ok,
                annotation_ok,
                materialization,
                depth + 1,
            )? && initializer_type_matches_annotation(
                actual_error,
                annotation_error,
                materialization,
                depth + 1,
            )?)
        }
        (Type::Record(actual_fields), Type::Record(annotation_fields)) => {
            for (name, annotation_field) in annotation_fields.iter() {
                let Some(actual_field) = actual_fields.get(name) else {
                    return Ok(false);
                };
                if !initializer_type_matches_annotation(
                    actual_field,
                    annotation_field,
                    materialization,
                    depth + 1,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(actual.matches_expected(annotation)),
    }
}

fn initializer_types_match_invariant(
    actual: &Type,
    annotation: &Type,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<bool, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new(
            "annotated initializer type nesting is too deep",
        ));
    }
    materialization.charge(1)?;
    if actual == annotation {
        return Ok(true);
    }
    match (actual, annotation) {
        (Type::Validated(actual), Type::Validated(annotation)) => {
            Ok(actual.validation() == annotation.validation()
                && initializer_types_match_invariant(
                    actual.base(),
                    annotation.base(),
                    materialization,
                    depth + 1,
                )?)
        }
        (Type::List(actual), Type::List(annotation))
        | (Type::Stream(actual), Type::Stream(annotation))
        | (Type::Set(actual), Type::Set(annotation))
        | (Type::Optional(actual), Type::Optional(annotation)) => {
            initializer_types_match_invariant(
                actual,
                annotation,
                materialization,
                depth + 1,
            )
        }
        (Type::Map(actual_key, actual_value), Type::Map(annotation_key, annotation_value))
        | (
            Type::Result(actual_key, actual_value),
            Type::Result(annotation_key, annotation_value),
        ) => Ok(initializer_types_match_invariant(
            actual_key,
            annotation_key,
            materialization,
            depth + 1,
        )? && initializer_types_match_invariant(
            actual_value,
            annotation_value,
            materialization,
            depth + 1,
        )?),
        (Type::Record(actual_fields), Type::Record(annotation_fields)) => {
            if actual_fields.len() != annotation_fields.len() {
                return Ok(false);
            }
            for (name, actual_field) in actual_fields.iter() {
                let Some(annotation_field) = annotation_fields.get(name) else {
                    return Ok(false);
                };
                if !initializer_types_match_invariant(
                    actual_field,
                    annotation_field,
                    materialization,
                    depth + 1,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Type::Union(actual_members), Type::Union(annotation_members)) => {
            if actual_members.len() != annotation_members.len() {
                return Ok(false);
            }
            for actual_member in actual_members {
                let mut matched = false;
                for annotation_member in annotation_members {
                    if initializer_types_match_invariant(
                        actual_member,
                        annotation_member,
                        materialization,
                        depth + 1,
                    )? {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn runtime_complete_value_check_type(ty: &Type, materialization: &mut SchemeInstantiationBudget, depth: usize) -> Result<bool, IrVerifyError> {
    runtime_complete_check_type(ty, materialization, depth, false)
}

fn runtime_complete_storage_check_type(ty: &Type, materialization: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    runtime_complete_check_type(ty, materialization, 0, true)
}

fn runtime_complete_check_type(
    ty: &Type,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
    allow_dynamic: bool,
) -> Result<bool, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new(
            "generic checked-value type nesting is too deep",
        ));
    }
    materialization.charge(1)?;
    Ok(match ty {
        Type::Null
        | Type::Bool
        | Type::Int
        | Type::UInt
        | Type::Float
        | Type::Duration
        | Type::Str
        | Type::Bytes
        | Type::Digest
        | Type::Regex
        | Type::Path
        | Type::ErasedRecord
        | Type::Status
        | Type::EnvPathList
        | Type::Error
        | Type::ProcessError
        | Type::Pure
        | Type::Proc
        | Type::Command
        | Type::ProcessHandle
        | Type::NetJob
        | Type::FsRoot
        | Type::FsLock
        | Type::Unit
        => true,
        Type::List(item) | Type::Optional(item) => {
            runtime_complete_check_type(item, materialization, depth + 1, allow_dynamic)?
        }
        Type::Map(key, value) => {
            key.is_map_key()
                && runtime_complete_check_type(value, materialization, depth + 1, allow_dynamic)?
        }
        Type::Set(item) => item.is_map_key(),
        Type::Record(fields) => {
            let mut complete = true;
            for field_type in fields.values() {
                complete &=
                    runtime_complete_check_type(field_type, materialization, depth + 1, allow_dynamic)?;
            }
            complete
        }
        Type::Result(success, error) => {
            runtime_complete_check_type(success, materialization, depth + 1, allow_dynamic)?
                && runtime_complete_check_type(error, materialization, depth + 1, allow_dynamic)?
        }
        Type::Union(members) => {
            if members.is_empty() {
                false
            } else {
                let mut complete = true;
                for member in members {
                    complete &=
                        runtime_complete_check_type(member, materialization, depth + 1, allow_dynamic)?;
                }
                complete
            }
        }
        Type::Validated(validated) => match validated.validation() {
            Validation::Nominal(_) => false,
            _ => runtime_complete_check_type(
                validated.base(),
                materialization,
                depth + 1,
                allow_dynamic,
            )?,
        },
        Type::Any => allow_dynamic,
        Type::BuiltinParameter(_)
        | Type::Inference(_)
        | Type::Unknown
        | Type::Invalid
        | Type::Stream(_)
        | Type::Module(_)
        | Type::DynamicModule
        | Type::Callable(_)
        | Type::Tag(_)
        | Type::ErrorFamily(_)
        | Type::ErrorVariant { .. }
        | Type::ErrorFacet(_) => false,
    })
}

fn merge_list_item_types(
    items: &[Type],
    materialization: &mut SchemeInstantiationBudget,
) -> Result<Option<Type>, IrVerifyError> {
    let mut items = items.iter();
    let Some(first) = items.next() else {
        return Ok(None);
    };
    materialization.charge_clone(first)?;
    let mut item = first.clone();
    for next in items {
        materialization.charge_clone(&item)?;
        materialization.charge_clone(next)?;
        let Some(merged) = merge_list_literal_item_ty(&item, next) else {
            return Ok(None);
        };
        item = merged;
    }
    materialization.charge(1)?;
    Ok(Some(Type::List(Box::new(item))))
}

fn expected_list_item(ty: &Type, nonempty: bool) -> Option<&Type> {
    match ty {
        Type::List(item) => Some(item),
        Type::Validated(validated)
            if nonempty && validated.validation() == Validation::NonEmpty =>
        {
            let Type::List(item) = validated.base() else {
                return None;
            };
            Some(item)
        }
        _ => None,
    }
}

fn scheme_type_binders(
    store: &FullStore,
    root: SchemeTypeId,
    materialization: &mut SchemeInstantiationBudget,
) -> Result<(BTreeSet<usize>, BTreeSet<usize>), IrVerifyError> {
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();
    let mut types = BTreeSet::new();
    let mut rows = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        materialization.charge(1)?;
        let node = store.scheme_type(id)?;
        match node.tag {
            FullSchemeTypeTag::Callable => {
                let callable = store.scheme_callable_monotype(node.first)?;
                pending.push(callable.return_type);
                for parameter in bounded(callable.params, &store.scheme_callable_parameters, "scheme callable parameter range is invalid")? {
                    pending.push(parameter.ty);
                }
            }
            FullSchemeTypeTag::Parameter => {
                types.insert(node.first as usize);
            }
            FullSchemeTypeTag::List
            | FullSchemeTypeTag::Stream
            | FullSchemeTypeTag::Set
            | FullSchemeTypeTag::Optional => pending.push(SchemeTypeId::from_raw(node.first)?),
            FullSchemeTypeTag::Map | FullSchemeTypeTag::Result => {
                pending.push(SchemeTypeId::from_raw(node.first)?);
                pending.push(SchemeTypeId::from_raw(node.second)?);
            }
            FullSchemeTypeTag::Record => {
                if node.first != IR_NONE {
                    rows.insert(node.first as usize);
                }
                for field in bounded(
                    node.children,
                    &store.scheme_fields,
                    "scheme record field range is invalid",
                )? {
                    pending.push(field.ty);
                }
            }
            FullSchemeTypeTag::Union => {
                for child in bounded(
                    node.children,
                    &store.scheme_children,
                    "scheme child range is invalid",
                )? {
                    pending.push(*child);
                }
            }
            FullSchemeTypeTag::Validated => pending.push(SchemeTypeId::from_raw(node.first)?),
            FullSchemeTypeTag::Ground => {}
        }
    }
    Ok((types, rows))
}

fn checked_expression_type_id(store: &FullStore, instruction: u32) -> Option<TypeId> {
    store
        .checked_expr_types
        .binary_search_by_key(&instruction, |fact| fact.instruction)
        .ok()
        .and_then(|index| { let fact = store.checked_expr_types[index]; fact.is_contract().then_some(fact.type_id) })
}

fn verify_scheme_argument_evidence(
    store: &FullStore,
    scope: &SchemeScope,
    scheme_type: SchemeTypeId,
    actual: &Type,
    substitution: &CallSubstitution,
    evidence: &mut SchemeBinderEvidence,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<(), IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("generic call argument type nesting is too deep"));
    }
    materialization.charge(1)?;
    let node = store.scheme_type(scheme_type)?;
    let actual_shape = actual.unvalidated();
    match node.tag {
        FullSchemeTypeTag::Callable => {
            let Type::Callable(actual) = actual_shape else {
                return Err(IrVerifyError::new("generic callable argument has no checked callable contract"));
            };
            let callable = store.scheme_callable_monotype(node.first)?;
            let parameters = bounded(callable.params, &store.scheme_callable_parameters, "scheme callable parameter range is invalid")?;
            let pure = match callable.kind {
                crate::sema::types::CallableKind::Pure => true,
                crate::sema::types::CallableKind::Proc => false,
                crate::sema::types::CallableKind::Stream => return Err(IrVerifyError::new("stream callable argument has no supported checked contract")),
            };
            if actual.pure != pure || actual.sig.params.len() != parameters.len() {
                return Err(IrVerifyError::new("generic callable argument kind or arity disagrees with its contract"));
            }
            for (parameter, actual) in parameters.iter().zip(&actual.sig.params) {
                if store.string(parameter.name)? != actual.name.as_str()
                    || (parameter.flags & 1 != 0) != actual.rest || (parameter.flags & 2 != 0) != actual.defaulted
                {
                    return Err(IrVerifyError::new("generic callable argument layout disagrees with its contract"));
                }
                verify_scheme_argument_evidence(store, scope, parameter.ty, &actual.ty, substitution, evidence, materialization, depth + 1)?;
            }
            verify_scheme_argument_evidence(store, scope, callable.return_type, &actual.sig.return_ty, substitution, evidence, materialization, depth + 1)?;
        }
        FullSchemeTypeTag::Parameter => {
            let binder = node.first as usize;
            if binder >= scope.type_parameters.len() {
                return Err(IrVerifyError::new("scheme type parameter is out of scope"));
            }
            if substitution.types.get(binder) != Some(actual) {
                return Err(IrVerifyError::new(
                    "generic type binder disagrees with the checked argument",
                ));
            }
            evidence.types.insert(binder);
        }
        FullSchemeTypeTag::Record if node.first != IR_NONE => {
            let Type::Record(actual_fields) = actual_shape else {
                return Err(IrVerifyError::new("generic row argument is not a checked record"));
            };
            let binder = node.first as usize;
            if binder >= scope.row_parameters.len() {
                return Err(IrVerifyError::new("scheme row parameter is out of scope"));
            }
            let row_evidence = &substitution.rows[binder];
            let fields = bounded(node.children, &store.scheme_fields, "scheme record field range is invalid")?;
            let explicit = fields
                .iter()
                .map(|field| Ok((Name::intern(store.string(field.name)?), field.ty)))
                .collect::<Result<BTreeMap<_, _>, IrVerifyError>>()?;
            let actual_extras = actual_fields
                .iter()
                .filter(|(name, _)| !explicit.contains_key(name))
                .map(|(name, ty)| (*name, ty))
                .collect::<BTreeMap<_, _>>();
            if actual_extras.len() != row_evidence.len()
                || actual_extras
                    .iter()
                    .any(|(name, ty)| row_evidence.get(name).is_none_or(|expected| expected != *ty))
            {
                return Err(IrVerifyError::new("generic row substitution disagrees with the checked argument"));
            }
            evidence.rows.insert(binder);
            for (name, field_type) in explicit {
                let actual_field = actual_fields
                    .get(&name)
                    .ok_or_else(|| IrVerifyError::new("generic call is missing a required record field"))?;
                verify_scheme_argument_evidence(
                    store,
                    scope,
                    field_type,
                    actual_field,
                    substitution,
                    evidence,
                    materialization,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        FullSchemeTypeTag::Record => {
            if let Type::Record(actual_fields) = actual_shape {
                for field in bounded(node.children, &store.scheme_fields, "scheme record field range is invalid")? {
                    let name = Name::intern(store.string(field.name)?);
                    if let Some(actual_field) = actual_fields.get(&name) {
                        verify_scheme_argument_evidence(
                            store,
                            scope,
                            field.ty,
                            actual_field,
                            substitution,
                            evidence,
                            materialization,
                            depth + 1,
                        )?;
                    }
                }
            }
        }
        FullSchemeTypeTag::List | FullSchemeTypeTag::Stream | FullSchemeTypeTag::Set => {
            let inner = match (node.tag, actual_shape) {
                (FullSchemeTypeTag::List, Type::List(inner))
                | (FullSchemeTypeTag::Stream, Type::Stream(inner))
                | (FullSchemeTypeTag::Set, Type::Set(inner)) => inner,
                _ => return Ok(()),
            };
            verify_scheme_argument_evidence(
                store,
                scope,
                SchemeTypeId::from_raw(node.first)?,
                inner,
                substitution,
                evidence,
                materialization,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Optional => {
            if matches!(actual_shape, Type::Null) {
                return Ok(());
            }
            let inner = match actual_shape {
                Type::Optional(inner) => inner,
                actual => actual,
            };
            verify_scheme_argument_evidence(
                store,
                scope,
                SchemeTypeId::from_raw(node.first)?,
                inner,
                substitution,
                evidence,
                materialization,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Map => {
            if let Type::Map(key, value) = actual_shape {
                verify_scheme_argument_evidence(store, scope, SchemeTypeId::from_raw(node.first)?, key, substitution, evidence, materialization, depth + 1)?;
                verify_scheme_argument_evidence(store, scope, SchemeTypeId::from_raw(node.second)?, value, substitution, evidence, materialization, depth + 1)?;
            }
        }
        FullSchemeTypeTag::Result => {
            if let Type::Result(ok, error) = actual_shape {
                verify_scheme_argument_evidence(store, scope, SchemeTypeId::from_raw(node.first)?, ok, substitution, evidence, materialization, depth + 1)?;
                verify_scheme_argument_evidence(store, scope, SchemeTypeId::from_raw(node.second)?, error, substitution, evidence, materialization, depth + 1)?;
            }
        }
        FullSchemeTypeTag::Validated => {
            verify_scheme_argument_evidence(
                store,
                scope,
                SchemeTypeId::from_raw(node.first)?,
                actual_shape,
                substitution,
                evidence,
                materialization,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Union => {
            let members = match actual_shape {
                Type::Union(members) => members.iter().collect(),
                actual => vec![actual],
            };
            let children = bounded(
                node.children,
                &store.scheme_children,
                "scheme union child range is invalid",
            )?;
            for actual_member in members {
                let mut best = None;
                let mut best_evidence = evidence.types.len() + evidence.rows.len();
                for child in children {
                    let child_expected = instantiate_scheme_type(
                        store,
                        scope,
                        *child,
                        substitution,
                        materialization,
                        depth + 1,
                    )?;
                    if !actual_member.matches_expected(&child_expected) {
                        continue;
                    }
                    let mut candidate = evidence.clone();
                    if verify_scheme_argument_evidence(
                        store,
                        scope,
                        *child,
                        actual_member,
                        substitution,
                        &mut candidate,
                        materialization,
                        depth + 1,
                    )
                    .is_ok()
                    {
                        let candidate_count = candidate.types.len() + candidate.rows.len();
                        if best.is_none() || candidate_count > best_evidence {
                            best = Some(candidate);
                            best_evidence = candidate_count;
                        }
                    }
                }
                let Some(best) = best else {
                    return Err(IrVerifyError::new(
                        "generic union argument disagrees with its checked substitution",
                    ));
                };
                *evidence = best;
            }
        }
        _ => {}
    }
    Ok(())
}

fn scheme_type_work_limit(store: &FullStore) -> usize {
    store
        .scheme_types
        .len()
        .saturating_add(store.scheme_effect_parameters.len())
        .saturating_add(store.scheme_effect_terms.len())
        .saturating_add(store.scheme_effect_predicates.len())
        .saturating_add(store.scheme_applications.len())
        .saturating_add(store.scheme_application_calls.len())
        .saturating_add(store.scheme_application_returns.len())
        .saturating_add(store.scheme_application_arguments.len())
        .saturating_add(store.scheme_proc_returns.len())
        .saturating_add(store.call_application_bindings.len())
        .saturating_add(store.call_application_slots.len())
        .saturating_add(store.call_effect_arguments.len())
        .saturating_add(store.scheme_effects.len())
        .saturating_mul(SCHEME_TYPE_WORK_PER_NODE)
        .saturating_add(SCHEME_TYPE_WORK_BASE)
}

fn instantiate_scheme_type(
    store: &FullStore,
    scope: &SchemeScope,
    id: SchemeTypeId,
    substitution: &CallSubstitution,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<Type, IrVerifyError> {
    if scope.type_parameters.len() != substitution.types.len()
        || scope.row_parameters.len() != substitution.rows.len()
        || scope.effects.parameters.len() != substitution.effects.len()
    {
        return Err(IrVerifyError::new("generic call substitution does not match its binder scope"));
    }
    instantiate_bound_type(store, id, substitution, budget, depth)
}

pub(super) fn invocation_parameter_types(
    store: &FullStore, function: usize, substitution: &CallSubstitution,
) -> Result<Vec<Type>, IrVerifyError> {
    let target = store.functions.get(function).ok_or_else(|| IrVerifyError::new("invocation parameter owner is out of bounds"))?;
    let mut budget = SchemeInstantiationBudget::new(store);
    match target.contract {
        FullCallableContract::Ground(signature) => {
            let count = store.semantic.signature_param_count(signature)?;
            budget.charge(count)?;
            (0..count).map(|slot| {
                let (_, ty, _) = store.semantic.signature_param(signature, slot)?;
                store.semantic.to_type_with_budget(ty, &mut budget.semantic_types)
            }).collect()
        }
        FullCallableContract::Scheme(id) => {
            let scheme = store.scheme(id)?;
            let parameters = scheme.params.bounds(store.scheme_params.len())
                .ok_or_else(|| IrVerifyError::new("invocation parameter range is invalid"))?;
            budget.charge(parameters.len())?;
            store.scheme_params[parameters].iter().map(|parameter| {
                instantiate_bound_type(store, parameter.ty, substitution, &mut budget, 0)
            }).collect()
        }
    }
}

pub(super) fn invocation_callable_type(
    store: &FullStore, target: usize, substitution: &CallSubstitution,
) -> Result<crate::sema::types::TypedCallable, IrVerifyError> {
    let function = store.functions.get(target).ok_or_else(|| IrVerifyError::new("prepared callable target is out of bounds"))?;
    let metadata = store.function_metadata.get(target).ok_or_else(|| IrVerifyError::new("prepared callable metadata is absent"))?;
    let pure = match metadata.callable_kind {
        crate::sema::types::CallableKind::Pure => true,
        crate::sema::types::CallableKind::Proc => false,
        crate::sema::types::CallableKind::Stream => return Err(IrVerifyError::new("stream declaration has no callable value representation")),
    };
    let mut budget = SchemeInstantiationBudget::new(store);
    let mut sig = match function.contract {
        super::schemes::FullCallableContract::Ground(signature) => {
            store.semantic.to_signature_with_budget(signature, 0, &mut budget.semantic_types)?
        }
        super::schemes::FullCallableContract::Scheme(id) => {
            let scheme = store.scheme(id)?;
            let parameters = bounded(scheme.params, &store.scheme_params, "prepared callable parameter range is invalid")?;
            let physical = bounded(function.params, &store.params, "prepared callable physical parameter range is invalid")?;
            if parameters.len() != physical.len() || scheme.return_schema.is_some() || parameters.iter().any(|parameter| parameter.schema.is_some()) {
                return Err(IrVerifyError::new("prepared callable has no complete runtime signature representation"));
            }
            budget.charge(parameters.len())?;
            let params = parameters.iter().zip(physical).map(|(parameter, physical)| {
                Ok(crate::sema::types::CallableParamType {
                    name: Name::intern(store.string(physical.name)?),
                    ty: instantiate_bound_type(store, parameter.ty, substitution, &mut budget, 0)?,
                    defaulted: physical.flags & 2 != 0, rest: physical.flags & 1 != 0,
                })
            }).collect::<Result<Vec<_>, IrVerifyError>>()?;
            let return_ty = Box::new(instantiate_bound_type(store, scheme.return_type, substitution, &mut budget, 0)?);
            let effects = effects::instantiate_contract(store, scheme.effects, &substitution.effects, &mut budget)?
                .for_invocation(scheme.callable_kind).concrete().map_err(|_| IrVerifyError::new("prepared callable effects remain unresolved"))?;
            crate::sema::types::CallableType { params, return_ty, effects: crate::sema::types::CallableEffects::from_clause(effects) }
        }
    };
    if pure { sig.effects = crate::sema::types::CallableEffects::finite([]); }
    Ok(crate::sema::types::TypedCallable { pure, sig })
}

pub(super) fn instantiate_invocation(
    store: &FullStore, scheme: &FullScheme, instance: &super::schemes::FullCallInstantiation,
    caller: &CallSubstitution, budget: &mut SchemeInstantiationBudget,
) -> Result<CallSubstitution, IrVerifyError> {
    let type_parameters = bounded(scheme.type_parameters, &store.scheme_type_parameters, "invocation type parameter range is invalid")?;
    let type_arguments = bounded(instance.type_arguments, &store.call_type_arguments, "invocation type argument range is invalid")?;
    let row_parameters = bounded(scheme.row_parameters, &store.scheme_row_parameters, "invocation row parameter range is invalid")?;
    let row_arguments = bounded(instance.row_arguments, &store.call_row_arguments, "invocation row argument range is invalid")?;
    let effect_parameters = bounded(scheme.effect_parameters, &store.scheme_effect_parameters, "invocation effect parameter range is invalid")?;
    let effect_arguments = bounded(instance.effect_arguments, &store.call_effect_arguments, "invocation effect argument range is invalid")?;
    if type_parameters.len() != type_arguments.len() || row_parameters.len() != row_arguments.len()
        || effect_parameters.len() != effect_arguments.len()
    {
        return Err(IrVerifyError::new("invocation substitution is incomplete"));
    }
    budget.charge(type_parameters.len().saturating_add(row_parameters.len()).saturating_add(effect_parameters.len()))?;
    let type_arguments = type_arguments.iter().map(|argument| (argument.binder, argument)).collect::<BTreeMap<_, _>>();
    let row_arguments = row_arguments.iter().map(|argument| (argument.binder, argument)).collect::<BTreeMap<_, _>>();
    let effect_arguments = effect_arguments.iter().map(|argument| (argument.binder, argument)).collect::<BTreeMap<_, _>>();
    if type_parameters.len() != type_arguments.len() || row_parameters.len() != row_arguments.len()
        || effect_parameters.len() != effect_arguments.len()
    { return Err(IrVerifyError::new("invocation substitution binder is duplicated")); }
    let mut result = CallSubstitution::default();
    for parameter in type_parameters {
        budget.charge(1)?;
        let argument = type_arguments.get(&parameter.id).ok_or_else(|| IrVerifyError::new("invocation type argument is absent"))?;
        let ty = instantiate_bound_type(store, argument.ty, caller, budget, 0)?;
        ensure_substitution_type(&ty)?;
        result.types.push(ty);
    }
    for parameter in row_parameters {
        budget.charge(1)?;
        let argument = row_arguments.get(&parameter.id).ok_or_else(|| IrVerifyError::new("invocation row argument is absent"))?;
        let mut fields = BTreeMap::new();
        if argument.tail != IR_NONE {
            let tail = caller.rows.get(argument.tail as usize).ok_or_else(|| IrVerifyError::new("invocation row tail is out of scope"))?;
            for (name, ty) in tail {
                budget.charge_clone(ty)?;
                fields.insert(*name, ty.clone());
            }
        }
        for field in bounded(argument.fields, &store.call_row_fields, "invocation row field range is invalid")? {
            budget.charge(1)?;
            let name = Name::intern(store.string(field.name)?);
            let ty = instantiate_bound_type(store, field.ty, caller, budget, 0)?;
            ensure_substitution_type(&ty)?;
            if fields.insert(name, ty).is_some() { return Err(IrVerifyError::new("invocation row field is duplicated")); }
        }
        for lack in bounded(parameter.lacks, &store.scheme_row_lacks, "invocation row lacks range is invalid")? {
            budget.charge(1)?;
            if fields.contains_key(&Name::intern(store.string(*lack)?)) {
                return Err(IrVerifyError::new("invocation row violates a lacks constraint"));
            }
        }
        result.rows.push(fields);
    }
    for parameter in effect_parameters {
        budget.charge(1)?;
        let argument = effect_arguments.get(&parameter.id).ok_or_else(|| IrVerifyError::new("invocation effect argument is absent"))?;
        let expression = effects::instantiate_contract(store, argument.expression, &caller.effects, budget)?;
        expression.concrete().map_err(|_| IrVerifyError::new("invocation effect argument remains unresolved"))?;
        result.effects.push(expression);
    }
    Ok(result)
}

pub(super) fn instantiate_bound_type(
    store: &FullStore, id: SchemeTypeId, substitution: &CallSubstitution,
    budget: &mut SchemeInstantiationBudget, depth: usize,
) -> Result<Type, IrVerifyError> {
    let ty = instantiate_scheme_type_inner(store, id, substitution, budget, depth)?;
    ensure_closed_type(&ty)?;
    Ok(ty)
}

fn instantiate_scheme_type_inner(
    store: &FullStore,
    id: SchemeTypeId,
    substitution: &CallSubstitution,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<Type, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("generic call result type nesting is too deep"));
    }
    budget.charge(1)?;
    let node = store.scheme_type(id)?;
    let ty = match node.tag {
        FullSchemeTypeTag::Callable => {
            let callable = store.scheme_callable_monotype(node.first)?;
            let pure = match callable.kind {
                crate::sema::types::CallableKind::Pure => true,
                crate::sema::types::CallableKind::Proc => false,
                crate::sema::types::CallableKind::Stream => return Err(IrVerifyError::new("stream callable monotype has no supported checked type")),
            };
            let parameters = bounded(callable.params, &store.scheme_callable_parameters, "scheme callable parameter range is invalid")?;
            if callable.return_schema.is_some() || parameters.iter().any(|parameter| parameter.schema.is_some()) {
                return Err(IrVerifyError::new("scheme callable schema contract has no ground representation"));
            }
            let params = parameters
                .iter().map(|parameter| {
                    budget.charge(1)?;
                    Ok(crate::sema::types::CallableParamType {
                        name: Name::intern(store.string(parameter.name)?),
                        ty: instantiate_scheme_type_inner(store, parameter.ty, substitution, budget, depth + 1)?,
                        defaulted: parameter.flags & 2 != 0,
                        rest: parameter.flags & 1 != 0,
                    })
                }).collect::<Result<Vec<_>, IrVerifyError>>()?;
            let return_ty = Box::new(instantiate_scheme_type_inner(store, callable.return_type, substitution, budget, depth + 1)?);
            let effects = effects::instantiate_contract(store, callable.effects, &substitution.effects, budget)?
                .concrete().map_err(|_| IrVerifyError::new("ground callable effects remain unresolved"))?;
            Type::Callable(Arc::new(crate::sema::types::TypedCallable {
                pure,
                sig: crate::sema::types::CallableType { params, return_ty, effects: crate::sema::types::CallableEffects::from_clause(effects) },
            }))
        }
        FullSchemeTypeTag::Ground => {
            let id = TypeId::from_raw(node.first)
                .ok_or_else(|| IrVerifyError::new("ground scheme type id is invalid"))?;
            store
                .semantic
                .to_type_with_budget(id, &mut budget.semantic_types)?
        }
        FullSchemeTypeTag::Parameter => {
            let binder = node.first as usize;
            let actual = substitution.types.get(binder)
                .ok_or_else(|| IrVerifyError::new("scheme type parameter is out of scope"))?;
            budget.charge_clone(actual)?;
            actual.clone()
        }
        FullSchemeTypeTag::List => Type::List(Box::new(instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?)),
        FullSchemeTypeTag::Stream => Type::Stream(Box::new(instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?)),
        FullSchemeTypeTag::Set => Type::Set(Box::new(instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?)),
        FullSchemeTypeTag::Optional => Type::Optional(Box::new(instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?)),
        FullSchemeTypeTag::Map => Type::Map(
            Box::new(instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?),
            Box::new(instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.second)?, substitution, budget, depth + 1)?),
        ),
        FullSchemeTypeTag::Result => Type::Result(
            Box::new(instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?),
            Box::new(instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.second)?, substitution, budget, depth + 1)?),
        ),
        FullSchemeTypeTag::Record => {
            let mut fields = BTreeMap::new();
            for field in bounded(node.children, &store.scheme_fields, "scheme record field range is invalid")? {
                let name = Name::intern(store.string(field.name)?);
                let ty = instantiate_scheme_type_inner(store, field.ty, substitution, budget, depth + 1)?;
                if fields.insert(name, ty).is_some() {
                    return Err(IrVerifyError::new("scheme record fields are duplicated"));
                }
            }
            if node.first != IR_NONE {
                let binder = node.first as usize;
                let row = substitution.rows.get(binder)
                    .ok_or_else(|| IrVerifyError::new("scheme row parameter is out of scope"))?;
                for (name, ty) in row {
                    budget.charge(1)?;
                    budget.charge_clone(ty)?;
                    if fields.insert(*name, ty.clone()).is_some() {
                        return Err(IrVerifyError::new("generic row evidence repeats a required field"));
                    }
                }
            }
            Type::Record(Arc::new(fields))
        }
        FullSchemeTypeTag::Union => {
            let members = bounded(node.children, &store.scheme_children, "scheme union child range is invalid")?
                .iter()
                .map(|child| instantiate_scheme_type_inner(store, *child, substitution, budget, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            if crate::sema::types::union_member_error(&members).is_some() {
                return Err(IrVerifyError::new("instantiated generic union is invalid"));
            }
            Type::Union(members)
        }
        FullSchemeTypeTag::Validated => {
            let base = instantiate_scheme_type_inner(store, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?;
            let row = *store
                .scheme_validations
                .get(node.second as usize)
                .ok_or_else(|| IrVerifyError::new("scheme validation id is out of bounds"))?;
            let validation = verify_validation_row(store, row)?;
            Type::Validated(Box::new(
                ValidatedType::new(validation, base)
                    .map_err(|_| IrVerifyError::new("scheme validation does not fit its base type"))?,
            ))
        }
    };
    Ok(ty)
}

fn decoder_for_instruction<'store>(
    store: &'store FullStore,
    instruction: usize,
    function_signatures: &Arc<super::FunctionSignatureIndex>,
) -> Result<FullDecoder<'store>, IrVerifyError> {
    for (index, function) in store.functions.iter().enumerate() {
        let range = store.function_instruction_range(index)?;
        if range.contains(&instruction) {
            let mut decoder = decoder_for_function(store, index)?;
            decoder.function_signature_index = Some(Arc::clone(function_signatures));
            return Ok(decoder);
        }
        let _ = function;
    }
    for index in 0..store.driver_steps.len() {
        let range = store.driver_instruction_range(index)?;
        if range.contains(&instruction) {
            let mut decoder = decoder_for_driver_step(store, index)?;
            decoder.function_signature_index = Some(Arc::clone(function_signatures));
            return Ok(decoder);
        }
    }
    Err(IrVerifyError::new("generic call instruction has no executable owner"))
}

pub(super) fn decoder_for_driver_step(
    store: &FullStore,
    index: usize,
) -> Result<FullDecoder<'_>, IrVerifyError> {
    let step = store
        .driver_steps
        .get(index)
        .ok_or_else(|| IrVerifyError::new("generic driver step is out of bounds"))?;
    Ok(FullDecoder {
        store,
        owner: driver_owner(index).map_err(|_| IrVerifyError::new("driver owner is invalid"))?,
        instruction_range: store.driver_instruction_range(index)?,
        instruction_states: None,
        borrowed_instruction_parents: None,
            producer_prefix: None,
        instruction_proof: None,
        block_states: None,
        slot_count: step.slot_count,
        pattern_ceiling: Cell::new(usize::MAX),
        local_immutable_let_initializers: RefCell::default(),
        binding_identity_stack: OnceCell::new(),
        proven_callable_writers: RefCell::default(),
        function_signature_index: None,
        return_analysis: RefCell::default(),
        producer_demand: crate::runtime::eval::ProducerDemand::Absent,
        prepared_constant_work: Cell::new(0),
        verified: false,
    })
}

fn decoder_for_function(store: &FullStore, function_index: usize) -> Result<FullDecoder<'_>, IrVerifyError> {
    let function = store
        .functions
        .get(function_index)
        .ok_or_else(|| IrVerifyError::new("generic function is out of bounds"))?;
    Ok(FullDecoder {
        store,
        owner: IrFunctionId::new(function_index)
            .map_err(|_| IrVerifyError::new("generic function id is invalid"))?
            .raw(),
        instruction_range: store.function_instruction_range(function_index)?,
        instruction_states: None,
        borrowed_instruction_parents: None,
            producer_prefix: None,
        instruction_proof: None,
        block_states: None,
        slot_count: function.slot_count,
        pattern_ceiling: Cell::new(usize::MAX),
        local_immutable_let_initializers: RefCell::default(),
        binding_identity_stack: OnceCell::new(),
        proven_callable_writers: RefCell::default(),
        function_signature_index: None,
        return_analysis: RefCell::default(),
        producer_demand: crate::runtime::eval::ProducerDemand::Absent,
        prepared_constant_work: Cell::new(0),
        verified: false,
    })
}

fn ensure_closed_type(ty: &Type) -> Result<(), IrVerifyError> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::BuiltinParameter(_) | Type::Inference(_) | Type::Unknown | Type::Invalid => return Err(IrVerifyError::new("scheme type contains an inference, recovery, or builtin parameter type")),
            Type::List(inner) | Type::Stream(inner) | Type::Set(inner) | Type::Optional(inner) => pending.push(inner),
            Type::Map(key, value) | Type::Result(key, value) => { pending.push(key); pending.push(value); }
            Type::Record(fields) => pending.extend(fields.values()),
            Type::Union(members) => pending.extend(members),
            Type::Validated(validated) => pending.push(validated.base()),
            Type::Callable(callable) => { pending.push(&callable.sig.return_ty); pending.extend(callable.sig.params.iter().map(|parameter| &parameter.ty)); }
            Type::Module(module) => {
                for export in module.values() {
                    match export {
                        crate::sema::types::ModuleExportType::Value { ty, .. } => pending.push(ty),
                        crate::sema::types::ModuleExportType::Pure { sig, .. } | crate::sema::types::ModuleExportType::Proc { sig, .. } => { pending.push(&sig.return_ty); pending.extend(sig.params.iter().map(|parameter| &parameter.ty)); }
                        crate::sema::types::ModuleExportType::GenericCallable { .. } => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn ensure_substitution_type(ty: &Type) -> Result<(), IrVerifyError> {
    ensure_closed_type(ty)?;
    if ty.contains_any() {
        return Err(IrVerifyError::new(
            format!("generic call substitution contains dynamic type {ty}"),
        ));
    }
    Ok(())
}

fn bounded<'a, T>(range: IrRange, values: &'a [T], message: &'static str) -> Result<&'a [T], IrVerifyError> {
    let bounds = range.bounds(values.len()).ok_or_else(|| IrVerifyError::new(message))?;
    Ok(&values[bounds])
}

fn mark_range<'a, T>(
    range: IrRange,
    values: &'a [T],
    coverage: &mut [bool],
    message: &'static str,
) -> Result<&'a [T], IrVerifyError> {
    let _ = range.bounds(values.len()).ok_or_else(|| IrVerifyError::new(message))?;
    let bounds = range.start as usize..range.start as usize + range.len as usize;
    if bounds.end > coverage.len() {
        return Err(IrVerifyError::new(message));
    }
    if coverage[bounds.clone()].iter().any(|covered| *covered) {
        return Err(IrVerifyError::new(message));
    }
    coverage[bounds.clone()].fill(true);
    Ok(&values[bounds])
}

fn require_empty_children(range: IrRange) -> Result<(), IrVerifyError> {
    if range.len == 0 {
        Ok(())
    } else {
        Err(IrVerifyError::new("scheme type node has unexpected child rows"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::semantic::SemanticPoolBuilder;
    use super::super::schemes::FullSchemeTypeNode;

    fn symbolic_substitution_test_scope() -> SchemeScope {
        SchemeScope {
            type_parameters: vec![FullSchemeTypeParameter { id: 10, binder: 0 }],
            row_parameters: vec![FullSchemeRowParameter {
                id: 20,
                binder: 0,
                lacks: IrRange::default(),
            }],
            type_binders: BTreeMap::from([(10, 0)]),
            row_binders: BTreeMap::from([(20, 0)]),
            row_lacks: vec![BTreeSet::new()],
            effects: effects::EffectScope::default(),
        }
    }

    #[test]
    fn symbolic_substitution_charges_cloned_type_and_row_evidence() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let scope = symbolic_substitution_test_scope();
            let type_argument = SchemeType::List(Box::new(SchemeType::Ground(Type::Str)));
            let substitution = SymbolicCallSubstitution {
                effects: Vec::new(),
                types: vec![type_argument],
                rows: vec![SchemeRowSubstitution {
                    fields: BTreeMap::from([(
                        Name::intern("field"),
                        SchemeType::List(Box::new(SchemeType::Ground(Type::Str))),
                    )]),
                    tail: SchemeRowTail::Closed,
                }],
            };
            let ground_error = substitute_scheme_type(
                &SchemeType::Ground(Type::List(Box::new(Type::Str))),
                &scope,
                &substitution,
                &mut SchemeInstantiationBudget {
                    remaining: 1,
                    semantic_types: TypeMaterializationBudget::default(),
                },
                0,
            )
            .unwrap_err();
            assert!(ground_error.message.contains("exceeds its work bound"));

            let type_error = substitute_scheme_type(
                &SchemeType::Parameter(SchemeTypeParameterId(10)),
                &scope,
                &substitution,
                &mut SchemeInstantiationBudget {
                    remaining: 3,
                    semantic_types: TypeMaterializationBudget::default(),
                },
                0,
            )
            .unwrap_err();
            assert!(type_error.message.contains("exceeds its work bound"));

            let row_error = substitute_scheme_type(
                &SchemeType::Record {
                    fields: BTreeMap::new(),
                    tail: SchemeRowTail::Parameter(SchemeRowParameterId(20)),
                },
                &scope,
                &substitution,
                &mut SchemeInstantiationBudget {
                    remaining: 4,
                    semantic_types: TypeMaterializationBudget::default(),
                },
                0,
            )
            .unwrap_err();
            assert!(row_error.message.contains("exceeds its work bound"));
        });
    }

    #[test]
    fn substitutions_share_one_type_materialization_budget() {
        let mut store = FullStore::default();
        let mut semantic = SemanticPoolBuilder::default();
        let mut ty = Type::Int;
        for _ in 0..15 {
            ty = Type::Result(Box::new(ty.clone()), Box::new(ty));
        }
        let type_id = semantic.intern_type(&mut store.semantic, &ty).unwrap();
        assert!(store.semantic.to_type(type_id).is_ok());
        let scheme_type_id = SchemeTypeId::from_raw(0).unwrap();
        store.scheme_types.push(FullSchemeTypeNode {
            first: type_id.raw(),
            second: 0,
            children: IrRange::default(),
            tag: FullSchemeTypeTag::Ground,
            reserved: [0; 3],
        });

        let scope = SchemeScope {
            type_parameters: vec![
                FullSchemeTypeParameter { id: 10, binder: 0 },
                FullSchemeTypeParameter { id: 11, binder: 1 },
            ],
            row_parameters: Vec::new(),
            type_binders: BTreeMap::from([(10, 0), (11, 1)]),
            row_binders: BTreeMap::new(),
            row_lacks: Vec::new(),
            effects: effects::EffectScope::default(),
        };
        let type_rows = [
            FullTypeArgument {
                binder: 10,
                ty: scheme_type_id,
            },
            FullTypeArgument {
                binder: 11,
                ty: scheme_type_id,
            },
        ];
        let error = build_substitution(&store, &scope, &type_rows, &[], &[], &mut [])
            .err()
            .expect("aggregate type materialization should exceed the shared budget");

        assert!(error.message.contains("semantic type materialization exceeds its work limit"));
    }
}

#[cfg(test)]
mod import_contract_tests;
