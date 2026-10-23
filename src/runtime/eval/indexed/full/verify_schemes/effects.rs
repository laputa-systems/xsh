use super::{bounded, mark_range, FullStore, IrVerifyError, SchemeInstantiationBudget, SchemeScope};
use super::super::schemes::{FullEffectContract, FullEffectParameter, FullEffectPredicate};
use crate::sema::effects::{EffectContract, EffectExpr, EffectPredicate, EffectSet};
use crate::sema::schemes::SchemeEffectParameterId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct EffectScope {
    pub(super) parameters: Vec<FullEffectParameter>,
    pub(super) binders: BTreeMap<u32, usize>,
}

impl EffectScope {
    pub(super) fn new(parameters: Vec<FullEffectParameter>) -> Result<Self, IrVerifyError> {
        let mut binders = BTreeMap::new();
        for (index, parameter) in parameters.iter().enumerate() {
            if parameter.id == super::IR_NONE || parameter.binder as usize != index
                || binders.insert(parameter.id, index).is_some()
            {
                return Err(IrVerifyError::new("scheme effect binders are duplicated or out of scope"));
            }
        }
        Ok(Self { parameters, binders })
    }
}

pub(super) struct EffectCoverage {
    pub(super) direct: Vec<bool>,
    pub(super) parameters: Vec<bool>,
    terms: Vec<bool>,
    predicates: Vec<bool>,
    arguments: Vec<bool>,
    used: BTreeMap<usize, BTreeSet<usize>>,
    budget: SchemeInstantiationBudget,
}

impl EffectCoverage {
    pub(super) fn new(store: &FullStore) -> Self {
        Self {
            direct: vec![false; store.scheme_effects.len()],
            parameters: vec![false; store.scheme_effect_parameters.len()],
            terms: vec![false; store.scheme_effect_terms.len()],
            predicates: vec![false; store.scheme_effect_predicates.len()],
            arguments: vec![false; store.call_effect_arguments.len()],
            used: BTreeMap::new(),
            budget: SchemeInstantiationBudget::new(store),
        }
    }

    pub(super) fn scope(
        &mut self, store: &FullStore, range: super::IrRange,
    ) -> Result<EffectScope, IrVerifyError> {
        self.budget.charge(range.len as usize)?;
        let parameters = mark_range(range, &store.scheme_effect_parameters, &mut self.parameters,
            "scheme effect parameter range is invalid or overlaps")?.to_vec();
        EffectScope::new(parameters)
    }

    pub(super) fn contract(
        &mut self, store: &FullStore, owner: usize, scope: &SchemeScope, contract: FullEffectContract,
    ) -> Result<EffectContract<SchemeEffectParameterId>, IrVerifyError> {
        self.budget.charge(1)?;
        mark_range(contract.direct, &store.scheme_effects, &mut self.direct,
            "scheme direct effect range is invalid or overlaps")?;
        let terms = mark_range(contract.terms, &store.scheme_effect_terms, &mut self.terms,
            "scheme effect term range is invalid or overlaps")?;
        let decoded = decode_contract(store, scope, contract, &mut self.budget)?;
        for term in terms {
            self.budget.charge(1)?;
            self.used.entry(owner).or_default().insert(term.binder as usize);
        }
        Ok(decoded)
    }

    pub(super) fn predicates(
        &mut self, store: &FullStore, owner: usize, scope: &SchemeScope, range: super::IrRange,
    ) -> Result<(), IrVerifyError> {
        self.budget.charge(range.len as usize)?;
        let predicates = mark_range(range, &store.scheme_effect_predicates, &mut self.predicates,
            "scheme effect predicate range is invalid or overlaps")?;
        for predicate in predicates {
            match *predicate {
                FullEffectPredicate::Equal(left, right) | FullEffectPredicate::Included(left, right)
                | FullEffectPredicate::Covered(left, right) => {
                    self.contract(store, owner, scope, left)?;
                    self.contract(store, owner, scope, right)?;
                }
                FullEffectPredicate::Excluded(expression, excluded) => {
                    self.contract(store, owner, scope, expression)?;
                    EffectSet::from_bits(excluded).ok_or_else(|| IrVerifyError::new("scheme effect exclusion mask is invalid"))?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn arguments(
        &mut self, store: &FullStore, owner: usize, scope: &SchemeScope, range: super::IrRange,
    ) -> Result<(), IrVerifyError> {
        self.budget.charge(range.len as usize)?;
        let arguments = mark_range(range, &store.call_effect_arguments, &mut self.arguments,
            "generic call effect argument range is invalid or overlaps")?;
        let mut binders = BTreeSet::new();
        for argument in arguments {
            if argument.binder == super::IR_NONE || !binders.insert(argument.binder) {
                return Err(IrVerifyError::new("generic call effect arguments are duplicated or invalid"));
            }
            self.contract(store, owner, scope, argument.expression)?;
        }
        Ok(())
    }

    pub(super) fn finish(self, scopes: &[Option<SchemeScope>]) -> Result<(), IrVerifyError> {
        if self.direct.iter().chain(&self.parameters).chain(&self.terms).chain(&self.predicates)
            .chain(&self.arguments).any(|used| !used)
        {
            return Err(IrVerifyError::new("scheme effect tables contain unowned rows"));
        }
        for (owner, scope) in scopes.iter().enumerate() {
            if let Some(scope) = scope
                && self.used.get(&owner).map_or(0, BTreeSet::len) != scope.effects.parameters.len()
            {
                return Err(IrVerifyError::new("generic scheme contains an unused effect binder"));
            }
        }
        Ok(())
    }
}

pub(super) fn decode_contract(
    store: &FullStore, scope: &SchemeScope, contract: FullEffectContract, budget: &mut SchemeInstantiationBudget,
) -> Result<EffectContract<SchemeEffectParameterId>, IrVerifyError> {
    map_contract(store, contract, budget, |ordinal, _| {
        let parameter = scope.effects.parameters.get(ordinal)
            .ok_or_else(|| IrVerifyError::new("scheme effect parameter is out of scope"))?;
        Ok(EffectContract::variable(SchemeEffectParameterId(parameter.id)))
    })
}

pub(super) fn instantiate_contract(
    store: &FullStore, contract: FullEffectContract,
    substitutions: &[EffectContract<SchemeEffectParameterId>], budget: &mut SchemeInstantiationBudget,
) -> Result<EffectContract<SchemeEffectParameterId>, IrVerifyError> {
    map_contract(store, contract, budget, |ordinal, budget| {
        let replacement = substitutions.get(ordinal)
            .ok_or_else(|| IrVerifyError::new("generic call effect substitution is incomplete"))?;
        if let EffectContract::Known(expression) = replacement {
            budget.charge(expression.variables().count())?;
        }
        Ok(replacement.clone())
    })
}

fn map_contract(
    store: &FullStore, contract: FullEffectContract, budget: &mut SchemeInstantiationBudget,
    mut replacement: impl FnMut(usize, &mut SchemeInstantiationBudget) -> Result<EffectContract<SchemeEffectParameterId>, IrVerifyError>,
) -> Result<EffectContract<SchemeEffectParameterId>, IrVerifyError> {
    budget.charge(1)?;
    let direct = bounded(contract.direct, &store.scheme_effects, "scheme direct effect range is invalid")?;
    let terms = bounded(contract.terms, &store.scheme_effect_terms, "scheme effect term range is invalid")?;
    if contract.reserved != [0; 3] || !contract.known && (!direct.is_empty() || !terms.is_empty()) {
        return Err(IrVerifyError::new("scheme effect contract metadata is invalid"));
    }
    if !contract.known { return Ok(EffectContract::Unknown); }
    budget.charge(direct.len())?;
    let direct_set = EffectSet::new(direct.iter().cloned());
    if direct_set.iter().count() != direct.len() {
        return Err(IrVerifyError::new("scheme direct effects are duplicated"));
    }
    let mut result = EffectContract::Known(EffectExpr::finite(direct_set));
    let mut previous = None;
    for term in terms {
        budget.charge(1)?;
        let mask = EffectSet::from_bits(term.mask).filter(|mask| !mask.is_empty())
            .ok_or_else(|| IrVerifyError::new("scheme effect term mask is invalid"))?;
        if term.reserved != [0; 3] || previous.is_some_and(|previous| previous >= term.binder) {
            return Err(IrVerifyError::new("scheme effect terms are not sorted unique"));
        }
        previous = Some(term.binder);
        result = result.union(replacement(term.binder as usize, budget)?.masked(mask));
    }
    Ok(result)
}

pub(super) fn decode_predicate(
    store: &FullStore, scope: &SchemeScope, predicate: FullEffectPredicate, budget: &mut SchemeInstantiationBudget,
) -> Result<EffectPredicate<SchemeEffectParameterId>, IrVerifyError> {
    Ok(match predicate {
        FullEffectPredicate::Equal(left, right) => EffectPredicate::Equal(
            decode_contract(store, scope, left, budget)?, decode_contract(store, scope, right, budget)?),
        FullEffectPredicate::Included(left, right) => EffectPredicate::Included(
            decode_contract(store, scope, left, budget)?, decode_contract(store, scope, right, budget)?),
        FullEffectPredicate::Covered(left, right) => EffectPredicate::Covered(
            decode_contract(store, scope, left, budget)?, decode_contract(store, scope, right, budget)?),
        FullEffectPredicate::Excluded(expression, excluded) => EffectPredicate::Excluded(
            decode_contract(store, scope, expression, budget)?, EffectSet::from_bits(excluded)
                .ok_or_else(|| IrVerifyError::new("scheme effect exclusion mask is invalid"))?),
    })
}

pub(super) fn validate_contract(
    contract: &EffectContract<SchemeEffectParameterId>, scope: &EffectScope,
) -> Result<(), IrVerifyError> {
    if let EffectContract::Known(expression) = contract {
        for (parameter, _) in expression.variables() {
            if !scope.binders.contains_key(&parameter.0) {
                return Err(IrVerifyError::new("scheme substitution effect binder is out of scope"));
            }
        }
    }
    Ok(())
}

pub(super) fn build_substitution(
    store: &FullStore, callee_scope: &SchemeScope, caller_scope: Option<&SchemeScope>,
    arguments: &[super::super::schemes::FullEffectArgument], budget: &mut SchemeInstantiationBudget,
) -> Result<Vec<EffectContract<SchemeEffectParameterId>>, IrVerifyError> {
    budget.charge(callee_scope.effects.parameters.len())?;
    let mut substitutions = vec![None; callee_scope.effects.parameters.len()];
    for argument in arguments {
        budget.charge(1)?;
        let &binder = callee_scope.effects.binders.get(&argument.binder)
            .ok_or_else(|| IrVerifyError::new("generic call effect binder is out of scope"))?;
        if caller_scope.is_none() && argument.expression.terms.len != 0 {
            return Err(IrVerifyError::new("ground generic call carries symbolic effect arguments"));
        }
        let contract = decode_contract(store, caller_scope.unwrap_or(callee_scope), argument.expression, budget)?;
        if substitutions[binder].replace(contract).is_some() {
            return Err(IrVerifyError::new("generic call effect binder is duplicated"));
        }
    }
    substitutions.into_iter().map(|substitution| substitution
        .ok_or_else(|| IrVerifyError::new("generic call effect substitution is incomplete"))).collect()
}

pub(super) fn substitute_contract(
    contract: &EffectContract<SchemeEffectParameterId>, scope: &SchemeScope,
    substitutions: &[EffectContract<SchemeEffectParameterId>], budget: &mut SchemeInstantiationBudget,
) -> Result<EffectContract<SchemeEffectParameterId>, IrVerifyError> {
    budget.charge(1)?;
    contract.substitute(|parameter| {
        budget.charge(1)?;
        let &binder = scope.effects.binders.get(&parameter.0)
            .ok_or_else(|| IrVerifyError::new("scheme substitution effect binder is out of scope"))?;
        let replacement = substitutions.get(binder)
            .ok_or_else(|| IrVerifyError::new("generic call effect substitution is incomplete"))?;
        if let EffectContract::Known(expression) = replacement {
            budget.charge(expression.variables().count())?;
        }
        Ok(replacement.clone())
    })
}

pub(super) fn verify_predicates(
    store: &FullStore, callee: &super::FullScheme, callee_scope: &SchemeScope,
    substitutions: &[EffectContract<SchemeEffectParameterId>],
    caller: Option<(&super::FullScheme, &SchemeScope)>, budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    for encoded in bounded(callee.effect_predicates, &store.scheme_effect_predicates,
        "scheme effect predicate range is invalid")?
    {
        budget.charge(1)?;
        let predicate = decode_predicate(store, callee_scope, *encoded, budget)?
            .try_map_contracts(|contract| substitute_contract(contract, callee_scope, substitutions, budget))?;
        match predicate.satisfaction() {
            Some(true) => continue,
            Some(false) => return Err(IrVerifyError::new("generic call effect predicate is not satisfied")),
            None => {}
        }
        let Some((caller, caller_scope)) = caller else {
            return Err(IrVerifyError::new("ground generic call effect predicate remains unresolved"));
        };
        let mut retained = false;
        for requirement in bounded(caller.effect_predicates, &store.scheme_effect_predicates,
            "scheme effect predicate range is invalid")?
        {
            budget.charge(1)?;
            if decode_predicate(store, caller_scope, *requirement, budget)? == predicate {
                retained = true;
                break;
            }
        }
        if !retained {
            return Err(IrVerifyError::new("generic call effect predicate has no caller requirement"));
        }
    }
    Ok(())
}

pub(in crate::runtime::eval::indexed::full) struct ScopedEffectProof<'a> {
    store: &'a FullStore,
    scopes: &'a super::VerifiedSchemeScopes,
}

impl<'a> ScopedEffectProof<'a> {
    pub(in crate::runtime::eval::indexed::full) fn new(store: &'a FullStore, scopes: &'a super::VerifiedSchemeScopes) -> Self {
        Self { store, scopes }
    }

    fn function_scope(&self, function: usize) -> Result<Option<super::super::schemes::FullSchemeId>, IrVerifyError> {
        let function = self.store.functions.get(function).ok_or_else(|| IrVerifyError::new("effect callable owner is out of bounds"))?;
        Ok(match function.contract { super::FullCallableContract::Ground(_) => None, super::FullCallableContract::Scheme(id) => Some(id) })
    }

    pub(in crate::runtime::eval::indexed::full) fn function_contract(&self, function: usize) -> Result<std::borrow::Cow<'a, EffectContract<SchemeEffectParameterId>>, IrVerifyError> {
        if self.store.function_metadata.get(function).is_some_and(|metadata| metadata.callable_kind == crate::sema::types::CallableKind::Pure) {
            return Ok(std::borrow::Cow::Owned(EffectContract::finite(EffectSet::default())));
        }
        let function = self.store.functions.get(function).ok_or_else(|| IrVerifyError::new("effect callable owner is out of bounds"))?;
        Ok(match function.contract {
            super::FullCallableContract::Ground(signature) => std::borrow::Cow::Owned(EffectContract::from_clause(self.store.semantic.signature_effects(signature)?)),
            super::FullCallableContract::Scheme(id) => {
                let invocation = self.scopes.invocations.get(id.index()).and_then(Option::as_ref)
                    .ok_or_else(|| IrVerifyError::new("effect callable has no verified invocation contract"))?;
                std::borrow::Cow::Borrowed(&invocation.contract)
            }
        })
    }

    pub(in crate::runtime::eval::indexed::full) fn call_contract(&self, owner: usize, instruction: usize, target: usize, budget: &mut SchemeInstantiationBudget) -> Result<EffectContract<SchemeEffectParameterId>, IrVerifyError> {
        let Some(target_id) = self.function_scope(target)? else { return Ok(self.function_contract(target)?.into_owned()); };
        let scope = self.scopes.get(target_id)?;
        let caller = self.function_scope(owner)?.map(|id| self.scopes.get(id)).transpose()?;
        let index = self.store.call_instantiations.binary_search_by_key(&(instruction as u32), |instance| instance.instruction)
            .map_err(|_| IrVerifyError::new("generic invocation effects have no call substitution"))?;
        let instance = &self.store.call_instantiations[index];
        let arguments = bounded(instance.effect_arguments, &self.store.call_effect_arguments, "generic invocation effect argument range is invalid")?;
        let substitutions = build_substitution(self.store, scope, caller, arguments, budget)?;
        substitute_contract(self.function_contract(target)?.as_ref(), scope, &substitutions, budget)
    }

    pub(in crate::runtime::eval::indexed::full) fn application_may_error(&self, owner: usize, instruction: usize, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
        let id = self.function_scope(owner)?.ok_or_else(|| IrVerifyError::new("application error flow has no generic owner"))?;
        let scope = self.scopes.get(id)?;
        let site = self.store.scheme_application_calls.binary_search_by_key(&(instruction as u32), |site| site.instruction)
            .map_err(|_| IrVerifyError::new("application error flow has no checked call site"))?;
        let ordinal = self.store.scheme_application_calls[site].application;
        let application = bounded(self.store.scheme(id)?.applications, &self.store.scheme_applications, "scheme application range is invalid")?
            .get(ordinal as usize).ok_or_else(|| IrVerifyError::new("application error flow ordinal is out of scope"))?;
        let effects = decode_contract(self.store, scope, application.effects, budget)?;
        let possible = match effects {
            EffectContract::Unknown => true,
            EffectContract::Known(expression) => expression.direct().iter().any(|effect| effect == crate::syntax::node::Effect::Error)
                || expression.variables().any(|(_, mask)| mask.iter().any(|effect| effect == crate::syntax::node::Effect::Error)),
        };
        if !possible { return Ok(false); }
        let result = super::decode_scheme_type(self.store, scope, application.result, budget, 0)?;
        Ok(match result {
            crate::sema::schemes::SchemeType::Result(_, _) | crate::sema::schemes::SchemeType::Stream(_) => false,
            crate::sema::schemes::SchemeType::Ground(result) => result.callee_error_flow(Some(&[crate::syntax::node::Effect::Error])) == crate::sema::types::CalleeErrorFlow::Immediate,
            _ => true,
        })
    }

    pub(in crate::runtime::eval::indexed::full) fn permits_direct(&self, owner: usize, effect: &crate::syntax::node::Effect) -> Result<bool, IrVerifyError> {
        Ok(match self.function_contract(owner)?.as_ref() {
            EffectContract::Unknown => true,
            EffectContract::Known(expression) => expression.direct().iter().any(|permission| crate::sema::effects::effect_covers(&permission, effect)),
        })
    }

    pub(in crate::runtime::eval::indexed::full) fn require(&self, owner: usize, required: &EffectContract<SchemeEffectParameterId>, captured: bool, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
        let required = if captured {
            if let EffectContract::Known(expression) = required { budget.charge(expression.variables().count())?; }
            required.clone().capture_error()
        } else {
            if let EffectContract::Known(expression) = required { budget.charge(expression.variables().count())?; }
            required.clone()
        };
        let ceiling = self.function_contract(owner)?;
        if let EffectContract::Known(expression) = ceiling.as_ref() { budget.charge(expression.variables().count())?; }
        let predicate = EffectPredicate::Covered(required, ceiling.into_owned());
        match predicate.satisfaction() {
            Some(true) => return Ok(()),
            Some(false) => return Err(IrVerifyError::new("executable operation requires effects outside its owner contract")),
            None => {}
        }
        if let Some(id) = self.function_scope(owner)? {
            let invocation = self.scopes.invocations[id.index()].as_ref()
                .ok_or_else(|| IrVerifyError::new("effect callable has no verified invocation contract"))?;
            for retained in &invocation.predicates {
                budget.charge(1)?;
                for contract in retained.contracts() {
                    if let EffectContract::Known(expression) = contract { budget.charge(expression.variables().count())?; }
                }
                if retained == &predicate { return Ok(()); }
            }
        }
        Err(IrVerifyError::new("executable operation effect requirement remains unresolved"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::IrRange;
    use crate::source::SourceId;
    use crate::syntax::node::Effect;

    #[test]
    fn prepared_effect_arguments_preserve_one_scoped_substitution() {
        use super::super::super::schemes::FullEffectArgument;
        let callee = SchemeEffectParameterId(17);
        let caller = SchemeEffectParameterId(29);
        let scope = |id: SchemeEffectParameterId| SchemeScope {
            operations: IrRange::default(),
            methods: IrRange::default(),
            type_parameters: Vec::new(), row_parameters: Vec::new(),
            type_binders: BTreeMap::new(), row_binders: BTreeMap::new(), row_lacks: Vec::new(),
            effects: EffectScope::new(vec![FullEffectParameter { id: id.0, binder: 0 }]).unwrap(),
        };
        let callee_scope = scope(callee);
        let caller_scope = scope(caller);
        let mut builder = super::super::super::FullBuilder::new(SourceId::new(0));
        let actual = EffectContract::variable(caller).capture_error()
            .union(EffectContract::finite(EffectSet::singleton(Effect::Env)));
        let expression = builder.intern_effect_contract(&actual, &BTreeMap::from([(caller, 0)])).unwrap();
        let argument = FullEffectArgument { binder: callee.0, expression };
        let build = |arguments: &[FullEffectArgument], caller| build_substitution(
            &builder.store, &callee_scope, caller, arguments, &mut SchemeInstantiationBudget::new(&builder.store));
        let substitutions = build(&[argument], Some(&caller_scope)).unwrap();
        let substituted = substitute_contract(&EffectContract::variable(callee), &callee_scope,
            &substitutions, &mut SchemeInstantiationBudget::new(&builder.store)).unwrap();
        assert_eq!(substituted, actual);
        assert!(substituted.concrete().is_err());
        assert!(validate_contract(&substituted, &caller_scope.effects).is_ok());
        assert!(validate_contract(&substituted, &callee_scope.effects).is_err());
        assert!(build(&[], Some(&caller_scope)).unwrap_err().message.contains("incomplete"));
        assert!(build(&[argument, argument], Some(&caller_scope)).unwrap_err().message.contains("duplicated"));
        assert!(build(&[FullEffectArgument { binder: caller.0, expression }], Some(&caller_scope))
            .unwrap_err().message.contains("out of scope"));
        assert!(build(&[argument], None).unwrap_err().message.contains("symbolic effect arguments"));
        let unknown = builder.intern_effect_contract(&EffectContract::Unknown, &BTreeMap::new()).unwrap();
        let substitutions = build_substitution(&builder.store, &callee_scope, None,
            &[FullEffectArgument { binder: callee.0, expression: unknown }],
            &mut SchemeInstantiationBudget::new(&builder.store)).unwrap();
        assert_eq!(substitute_contract(&EffectContract::variable(callee).capture_error(), &callee_scope,
            &substitutions, &mut SchemeInstantiationBudget::new(&builder.store)).unwrap(), EffectContract::Unknown);
    }

    #[test]
    fn prepared_effect_contracts_preserve_scopes_masks_and_unknown() {
        let parameter = SchemeEffectParameterId(17);
        let mut builder = super::super::super::FullBuilder::new(SourceId::new(0));
        let expression = EffectContract::variable(parameter).capture_error()
            .union(EffectContract::finite(EffectSet::singleton(Effect::Env)));
        let encoded = builder.intern_effect_contract(&expression, &BTreeMap::from([(parameter, 0)])).unwrap();
        let scope = SchemeScope {
            operations: IrRange::default(),
            methods: IrRange::default(),
            type_parameters: Vec::new(), row_parameters: Vec::new(),
            type_binders: BTreeMap::new(), row_binders: BTreeMap::new(), row_lacks: Vec::new(),
            effects: EffectScope::new(vec![FullEffectParameter { id: parameter.0, binder: 0 }]).unwrap(),
        };
        let decode = |store: &FullStore, contract| decode_contract(store, &scope, contract, &mut SchemeInstantiationBudget::new(store));
        assert_eq!(decode(&builder.store, encoded).unwrap(), expression);
        assert!(encoded.concrete(&builder.store).is_err());
        let mut wrong_scope = builder.store.clone();
        wrong_scope.scheme_effect_terms[encoded.terms.start as usize].binder = 1;
        assert!(decode(&wrong_scope, encoded).unwrap_err().message.contains("out of scope"));
        let mut wrong_mask = builder.store.clone();
        wrong_mask.scheme_effect_terms[encoded.terms.start as usize].mask = 0x80;
        assert!(decode(&wrong_mask, encoded).unwrap_err().message.contains("mask is invalid"));
        let mut unknown = encoded;
        unknown.known = false;
        assert!(decode(&builder.store, unknown).unwrap_err().message.contains("metadata is invalid"));
        assert!(builder.intern_effect_contract(&expression, &BTreeMap::new()).is_err());
        let unknown = builder.intern_effect_contract(&EffectContract::Unknown, &BTreeMap::new()).unwrap();
        assert_eq!(decode(&builder.store, unknown).unwrap(), EffectContract::Unknown);
        assert_eq!(unknown.concrete(&builder.store).unwrap(), None);
        let empty = builder.intern_effect_contract(&EffectContract::finite(EffectSet::default()), &BTreeMap::new()).unwrap();
        assert_eq!(empty.concrete(&builder.store).unwrap(), Some(Vec::new()));
        assert_ne!(decode(&builder.store, unknown).unwrap(), decode(&builder.store, empty).unwrap());
    }
}
