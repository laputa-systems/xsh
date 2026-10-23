use super::*;
use super::super::schemes::{FullApplicationArgumentKind, FullApplicationBinding, FullSchemeApplication};
use crate::sema::arguments::{ArgumentParameterLayout, StaticArgumentKind};
use crate::sema::callable_application::CallableKindRequirement;

#[derive(Clone, Copy)]
pub(super) enum ApplicationSubstitution<'a> {
    Ground(&'a CallSubstitution),
    Symbolic(&'a SymbolicCallSubstitution),
    Identity,
}

impl ApplicationSubstitution<'_> {
    pub(super) fn ty(&self, store: &FullStore, scope: &SchemeScope, ty: SchemeTypeId, budget: &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError> {
        match self {
            Self::Ground(substitution) => Ok(SchemeType::Ground(instantiate_scheme_type(store, scope, ty, substitution, budget, 0)?)),
            Self::Symbolic(substitution) => substitute_scheme_type(&decode_scheme_type(store, scope, ty, budget, 0)?, scope, substitution, budget, 0),
            Self::Identity => decode_scheme_type(store, scope, ty, budget, 0),
        }
    }

    pub(super) fn contract(&self, store: &FullStore, scope: &SchemeScope, contract: super::super::schemes::FullEffectContract, budget: &mut SchemeInstantiationBudget) -> Result<EffectContract<SchemeEffectParameterId>, IrVerifyError> {
        let contract = effects::decode_contract(store, scope, contract, budget)?;
        match self {
            Self::Ground(substitution) => effects::substitute_contract(&contract, scope, &substitution.effects, budget),
            Self::Symbolic(substitution) => effects::substitute_contract(&contract, scope, &substitution.effects, budget),
            Self::Identity => Ok(contract),
        }
    }
}

struct Application {
    callee: SchemeType,
    result: SchemeType,
    arguments: Vec<(StaticArgumentKind, SchemeType)>,
    effects: EffectContract<SchemeEffectParameterId>,
    proc_returns: Vec<(SchemeType, Vec<SchemeType>)>,
    kind: CallableKindRequirement,
}

fn decode(store: &FullStore, scope: &SchemeScope, encoded: FullSchemeApplication, substitution: &ApplicationSubstitution<'_>, budget: &mut SchemeInstantiationBudget) -> Result<Application, IrVerifyError> {
    budget.charge(1 + encoded.arguments.len as usize + encoded.proc_returns.len as usize)?;
    let callee = substitution.ty(store, scope, encoded.callee, budget)?;
    let result = substitution.ty(store, scope, encoded.result, budget)?;
    let effects = substitution.contract(store, scope, encoded.effects, budget)?;
    let mut arguments = Vec::new();
    for argument in bounded(encoded.arguments, &store.scheme_application_arguments, "scheme application argument range is invalid")? {
        let kind = match argument.kind {
            FullApplicationArgumentKind::Positional => StaticArgumentKind::Positional,
            FullApplicationArgumentKind::Named(name) => StaticArgumentKind::Named(Name::intern(store.string(name)?)),
            FullApplicationArgumentKind::Splice => StaticArgumentKind::Splice,
        };
        arguments.push((kind, substitution.ty(store, scope, argument.ty, budget)?));
    }
    let mut proc_returns = Vec::new();
    for envelope in bounded(encoded.proc_returns, &store.scheme_proc_returns, "scheme application return range is invalid")? {
        budget.charge(envelope.propagated_errors.len as usize)?;
        let result = substitution.ty(store, scope, envelope.result, budget)?;
        let mut errors = Vec::new();
        for error in bounded(envelope.propagated_errors, &store.scheme_children, "scheme application error range is invalid")? {
            errors.push(substitution.ty(store, scope, *error, budget)?);
        }
        proc_returns.push((result, errors));
    }
    Ok(Application { callee, result, arguments, effects, proc_returns, kind: encoded.kind })
}

fn same_type(left: &SchemeType, right: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    budget.charge_scheme_clone(left)?;
    budget.charge_scheme_clone(right)?;
    Ok(body_types_equal(left, right))
}

fn same_application(left: &Application, right: &Application, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    budget.charge(3 + left.arguments.len() + left.proc_returns.len())?;
    for contract in [&left.effects, &right.effects] {
        if let EffectContract::Known(expression) = contract { budget.charge(expression.variables().count())?; }
    }
    if left.kind != right.kind || left.effects != right.effects
        || !same_type(&left.callee, &right.callee, budget)? || !same_type(&left.result, &right.result, budget)?
        || left.arguments.len() != right.arguments.len() || left.proc_returns.len() != right.proc_returns.len()
    { return Ok(false); }
    for ((left_kind, left), (right_kind, right)) in left.arguments.iter().zip(&right.arguments) {
        if left_kind != right_kind || !same_type(left, right, budget)? { return Ok(false); }
    }
    for ((left, left_errors), (right, right_errors)) in left.proc_returns.iter().zip(&right.proc_returns) {
        budget.charge(left_errors.len())?;
        if !same_type(left, right, budget)? || left_errors.len() != right_errors.len() { return Ok(false); }
        for (left, right) in left_errors.iter().zip(right_errors) {
            if !same_type(left, right, budget)? { return Ok(false); }
        }
    }
    Ok(true)
}

fn selected(store: &FullStore, application: &Application, argument_slots: IrRange, omitted_slots: IrRange, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let (kind, parameters, result, effects) = match &application.callee {
        SchemeType::Ground(Type::Callable(callable)) => {
            budget.charge(callable.sig.params.len())?;
            let parameters = callable.sig.params.iter().map(|parameter| {
                budget.charge_clone(&parameter.ty)?;
                Ok((ArgumentParameterLayout { name: parameter.name, defaulted: parameter.defaulted, rest: parameter.rest },
                    SchemeType::Ground(parameter.ty.clone())))
            }).collect::<Result<Vec<_>, IrVerifyError>>()?;
            budget.charge_clone(&callable.sig.return_ty)?;
            let effects = callable.sig.effects.contract();
            if let EffectContract::Known(expression) = effects { budget.charge(expression.variables().count())?; }
            let kind = callable.kind;
            let effects = EffectContract::from_clause(effects.clone().for_invocation(kind).concrete().map_err(|_| IrVerifyError::new("ground application callable effects remain unresolved"))?);
            (kind, parameters,
                SchemeType::Ground(callable.sig.return_ty.as_ref().clone()), effects)
        }
        SchemeType::Callable { kind, monotype, effects } => {
            budget.charge(monotype.params().len())?;
            let parameters = monotype.params().iter().map(|parameter| {
                budget.charge_scheme_clone(&parameter.ty)?;
                Ok((ArgumentParameterLayout { name: parameter.name, defaulted: parameter.defaulted, rest: parameter.rest }, parameter.ty.clone()))
            }).collect::<Result<Vec<_>, IrVerifyError>>()?;
            budget.charge_scheme_clone(monotype.return_type())?;
            if let EffectContract::Known(expression) = effects { budget.charge(expression.variables().count())?; }
            let effects = effects.clone().for_invocation(*kind);
            (*kind, parameters, monotype.return_type().clone(), effects)
        }
        _ => return Err(IrVerifyError::new("selected application has no actual callable contract")),
    };
    if !application.kind.accepts(kind) || !same_type(&application.result, &result, budget)? || application.effects != effects {
        return Err(IrVerifyError::new("selected application disagrees with its callable contract"));
    }
    budget.charge(parameters.len() + application.arguments.len())?;
    let layout = parameters.iter().map(|(layout, _)| *layout).collect::<Vec<_>>();
    let span = crate::source::Span::at(store.source_id, 0);
    let binding = crate::sema::arguments::bind_parameter_layout(&layout, application.arguments.iter().map(|(kind, _)| (*kind, span)))
        .map_err(|_| IrVerifyError::new("selected application has an invalid argument layout"))?;
    let actual = bounded(argument_slots, &store.call_application_slots, "application argument slot range is invalid")?;
    let omitted = bounded(omitted_slots, &store.call_application_slots, "application omitted slot range is invalid")?;
    budget.charge(actual.len() + omitted.len())?;
    if !actual.iter().copied().map(|slot| slot as usize).eq(binding.argument_slots.iter().copied())
        || !omitted.iter().copied().map(|slot| slot as usize).eq(binding.omitted_slots.iter().copied())
    { return Err(IrVerifyError::new("selected application layout differs from its actual callable")); }
    for ((kind, actual), slot) in application.arguments.iter().zip(binding.argument_slots) {
        let (layout, parameter) = &parameters[slot];
        let expected = if layout.rest && *kind != StaticArgumentKind::Splice {
            match parameter {
                SchemeType::List(item) => item.as_ref(),
                SchemeType::Ground(Type::List(item)) => {
                    budget.charge_clone(item)?;
                    if !body_type_matches(actual, &SchemeType::Ground(item.as_ref().clone()), budget)? {
                        return Err(IrVerifyError::new("selected application argument disagrees with its parameter"));
                    }
                    continue;
                }
                _ => return Err(IrVerifyError::new("selected application rest parameter is not a list")),
            }
        } else { parameter };
        if !body_type_matches(actual, expected, budget)? {
            return Err(IrVerifyError::new("selected application argument disagrees with its parameter"));
        }
    }
    for (actual, errors) in &application.proc_returns {
        budget.charge_scheme_clone(&application.result)?;
        for error in errors { budget.charge_scheme_clone(error)?; }
        let value = application.result.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected application return envelope remains unresolved"))?;
        let errors = errors.iter().map(|error| error.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected application error envelope remains unresolved"))).collect::<Result<Vec<_>, _>>()?;
        let expected = crate::sema::callable_application::inferred_proc_return_envelope(value, &errors);
        if !same_type(actual, &SchemeType::Ground(expected), budget)? {
            return Err(IrVerifyError::new("selected application return envelope differs from its actual result"));
        }
    }
    Ok(())
}

pub(super) fn verify_bindings(store: &FullStore, callee: &FullScheme, scope: &SchemeScope, substitution: ApplicationSubstitution<'_>, caller: Option<(&FullScheme, &SchemeScope)>, bindings: &[FullApplicationBinding], budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let applications = bounded(callee.applications, &store.scheme_applications, "scheme application range is invalid")?;
    if applications.len() != bindings.len() { return Err(IrVerifyError::new("generic call application binding is incomplete")); }
    for (encoded, binding) in applications.iter().zip(bindings) {
        let application = decode(store, scope, *encoded, &substitution, budget)?;
        match *binding {
            FullApplicationBinding::Selected { argument_slots, omitted_slots } => selected(store, &application, argument_slots, omitted_slots, budget)?,
            FullApplicationBinding::Forwarded(ordinal) => {
                let Some((caller, caller_scope)) = caller else { return Err(IrVerifyError::new("ground application has no forwarding owner")); };
                let caller_applications = bounded(caller.applications, &store.scheme_applications, "caller application range is invalid")?;
                let encoded = caller_applications.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("forwarded application ordinal is out of scope"))?;
                let required = decode(store, caller_scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
                if !same_application(&application, &required, budget)? {
                    return Err(IrVerifyError::new("forwarded application differs from its actual caller requirement"));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn extend_call_binder_evidence(
    store: &FullStore, scheme: &FullScheme, scope: &SchemeScope, substitution: ApplicationSubstitution<'_>, evidence: &mut SchemeBinderEvidence,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let projections = super::wrappers::binder_projections(store, scheme, scope, &substitution, budget)?;
    let selected_sources = !matches!(substitution, ApplicationSubstitution::Symbolic(_));
    let applications = if selected_sources { bounded(scheme.applications, &store.scheme_applications, "scheme application range is invalid")? } else { &[] };
    budget.charge(applications.len())?;
    let mut outputs = Vec::with_capacity(applications.len());
    let mut remaining = Vec::with_capacity(applications.len());
    let mut watchers = BTreeMap::<(bool, usize), Vec<usize>>::new();
    let mut ready = Vec::new();
    for (ordinal, application) in applications.iter().enumerate() {
        let mut inputs = BTreeSet::new();
        let mut output = BTreeSet::new();
        let arguments = bounded(application.arguments, &store.scheme_application_arguments, "scheme application argument range is invalid")?;
        let envelopes = bounded(application.proc_returns, &store.scheme_proc_returns, "scheme application return range is invalid")?;
        budget.charge(1 + arguments.len() + envelopes.len())?;
        let mut input_roots = vec![application.callee];
        input_roots.extend(arguments.iter().map(|argument| argument.ty));
        let mut output_roots = vec![application.result];
        for envelope in envelopes {
            let errors = bounded(envelope.propagated_errors, &store.scheme_children, "scheme application error range is invalid")?;
            budget.charge(errors.len())?;
            input_roots.extend(errors.iter().copied());
            output_roots.push(envelope.result);
        }
        for (roots, binders) in [(input_roots, &mut inputs), (output_roots, &mut output)] {
            for root in roots {
                let (types, rows) = scheme_type_binders(store, root, budget)?;
                budget.charge(types.len() + rows.len())?;
                binders.extend(types.into_iter().map(|id| (false, id)));
                binders.extend(rows.into_iter().map(|id| (true, id)));
            }
        }
        let mut missing = 0;
        for key @ (row, binder) in inputs {
            budget.charge(1)?;
            if if row { evidence.rows.contains(&binder) } else { evidence.types.contains(&binder) } { continue; }
            watchers.entry(key).or_default().push(ordinal);
            missing += 1;
        }
        remaining.push(missing);
        outputs.push(output);
        if missing == 0 { ready.push(ordinal); }
    }
    for parameter in bounded(scheme.type_parameters, &store.scheme_type_parameters, "scheme type parameter range is invalid")?.iter().filter(|_| selected_sources) {
        let Some(iteration) = parameter.iteration()? else { continue };
        budget.charge(1)?;
        let ordinal = outputs.len();
        let mut output = BTreeSet::new();
        for root in iteration.types() {
            let (types, rows) = scheme_type_binders(store, *root, budget)?;
            budget.charge(types.len() + rows.len())?;
            output.extend(types.into_iter().map(|id| (false, id)));
            output.extend(rows.into_iter().map(|id| (true, id)));
        }
        let source = parameter.binder as usize;
        let missing = usize::from(!evidence.types.contains(&source));
        if missing != 0 { watchers.entry((false, source)).or_default().push(ordinal); }
        remaining.push(missing);
        outputs.push(output);
        if missing == 0 { ready.push(ordinal); }
    }
    for operation in bounded(scheme.operations, &store.scheme_operations, "scheme operation range is invalid")?.iter().filter(|_| selected_sources) {
        budget.charge(1)?;
        let ordinal = outputs.len();
        let mut inputs = BTreeSet::new();
        let mut output = BTreeSet::new();
        for (roots, binders) in [([Some(operation.left), Some(operation.right)], &mut inputs), ([Some(operation.result), None], &mut output)] {
            for root in roots.into_iter().flatten() {
                let (types, rows) = scheme_type_binders(store, root, budget)?;
                binders.extend(types.into_iter().map(|id| (false, id)));
                binders.extend(rows.into_iter().map(|id| (true, id)));
            }
        }
        let mut missing = 0;
        for key @ (row, binder) in inputs {
            budget.charge(1)?;
            if if row { evidence.rows.contains(&binder) } else { evidence.types.contains(&binder) } { continue; }
            watchers.entry(key).or_default().push(ordinal);
            missing += 1;
        }
        remaining.push(missing);
        outputs.push(output);
        if missing == 0 { ready.push(ordinal); }
    }
    for (wrapper_ordinal, wrapper) in bounded(scheme.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.iter().enumerate() {
        budget.charge(1)?;
        let ordinal = outputs.len();
        let mut inputs = BTreeSet::new();
        let mut output = BTreeSet::new();
        let (input_roots, output_roots) = super::wrappers::evidence_roots(store, *wrapper)?;
        for (roots, binders) in [(input_roots, &mut inputs), (output_roots, &mut output)] {
            for root in roots {
                let (types, rows) = scheme_type_binders(store, root, budget)?;
                binders.extend(types.into_iter().map(|id| (false, id)));
                binders.extend(rows.into_iter().map(|id| (true, id)));
            }
        }
        output.retain(|&(row, binder)| row || projections.permits(wrapper_ordinal, binder));
        let mut missing = 0;
        for key @ (row, binder) in inputs {
            budget.charge(1)?;
            if if row { evidence.rows.contains(&binder) } else { evidence.types.contains(&binder) } { continue; }
            watchers.entry(key).or_default().push(ordinal);
            missing += 1;
        }
        remaining.push(missing);
        outputs.push(output);
        if missing == 0 { ready.push(ordinal); }
    }
    while let Some(ordinal) = ready.pop() {
        budget.charge(1)?;
        for key @ (row, binder) in std::mem::take(&mut outputs[ordinal]) {
            budget.charge(1)?;
            let fresh = if row { evidence.rows.insert(binder) } else { evidence.types.insert(binder) };
            if !fresh { continue; }
            for dependent in watchers.remove(&key).unwrap_or_default() {
                budget.charge(1)?;
                remaining[dependent] -= 1;
                if remaining[dependent] == 0 { ready.push(dependent); }
            }
        }
    }
    Ok(())
}

pub(super) fn verify_site(
    store: &FullStore, decoder: &FullDecoder<'_>, instruction: usize, scope: &SchemeScope, effect_scopes: &VerifiedSchemeScopes, captured: bool,
    types: &BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget,
) -> Result<Vec<usize>, IrVerifyError> {
    let index = store.scheme_application_calls.binary_search_by_key(&(instruction as u32), |site| site.instruction)
        .map_err(|_| IrVerifyError::new("generic dynamic call has no application obligation"))?;
    let site = &store.scheme_application_calls[index];
    let owner = IrFunctionId::from_raw(decoder.owner).ok_or_else(|| IrVerifyError::new("generic application owner is invalid"))?.index();
    let function = store.functions.get(owner).ok_or_else(|| IrVerifyError::new("generic application owner is invalid"))?;
    let FullCallableContract::Scheme(scheme_id) = function.contract else { return Err(IrVerifyError::new("generic application has no scheme owner")); };
    let encoded = *bounded(store.scheme(scheme_id)?.applications, &store.scheme_applications, "scheme application range is invalid")?
        .get(site.application as usize).ok_or_else(|| IrVerifyError::new("generic application ordinal is out of scope"))?;
    let application = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    let kind = store.function_metadata.get(owner).ok_or_else(|| IrVerifyError::new("generic application owner metadata is missing"))?.callable_kind;
    if kind == crate::sema::types::CallableKind::Pure && application.kind != CallableKindRequirement::Pure {
        return Err(IrVerifyError::new("pure body has an impure application requirement"));
    }
    ScopedEffectProof::new(store, effect_scopes).require(owner, &application.effects,
        captured || kind == crate::sema::types::CallableKind::Pure, budget)?;
    let schema::ExprView::DynamicCall { callee, mut args, .. } = decoder.expression_view(instruction)? else {
        return Err(IrVerifyError::new("generic application has no dynamic call"));
    };
    let actual = |instruction| types.get(&instruction).ok_or_else(|| IrVerifyError::new("generic application operand has no checked type"));
    if !same_type(actual(callee.raw() as usize)?, &application.callee, budget)?
        || !same_type(actual(instruction)?, &application.result, budget)? || args.len() != application.arguments.len()
    { return Err(IrVerifyError::new("generic application operands disagree with its obligation")); }
    budget.charge(1 + args.len())?;
    let mut children = vec![callee.raw() as usize];
    for (kind, expected) in &application.arguments {
        let argument = args.next().ok_or_else(|| IrVerifyError::new("generic application argument is missing"))??;
        let value = match (kind, argument) {
            (StaticArgumentKind::Splice, super::super::decode::CallArgView::Splice(value)) => value,
            (StaticArgumentKind::Named(_) | StaticArgumentKind::Positional, super::super::decode::CallArgView::Single(value)) => value,
            _ => return Err(IrVerifyError::new("generic application argument kind differs from its obligation")),
        };
        if !same_type(actual(value.raw() as usize)?, expected, budget)? {
            return Err(IrVerifyError::new("generic application argument differs from its obligation"));
        }
        children.push(value.raw() as usize);
    }
    Ok(children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::callable_application::{CallableApplication, CallableApplicationArgument};
    use crate::sema::effects::{CheckedEffects, EffectSet};
    use crate::sema::schemes::{CallableRequirements, CallableScheme, TypeScheme};
    use crate::sema::types::{CallableKind, CallableParamType, CallableType, TypedCallable};
    use crate::source::{SourceId, Span};
    use crate::symbol::SymbolOwner;

    fn application_scheme(first: u32, effect: u32) -> TypeScheme {
        let parameters = (first..first + 4).map(SchemeTypeParameterId).collect::<Vec<_>>();
        let effects = [SchemeEffectParameterId(effect), SchemeEffectParameterId(effect + 1)];
        let span = Span::at(SourceId::new(0), 0);
        let applications = (0..2).map(|index| CallableApplication {
            callee: SchemeType::Parameter(parameters[index]),
            arguments: vec![CallableApplicationArgument { kind: StaticArgumentKind::Positional,
                ty: SchemeType::Parameter(parameters[index + 2]), span }].into(),
            result: SchemeType::Parameter(parameters[index + 2]),
            effects: EffectContract::variable(effects[index]),
            proc_returns: Arc::from([]), kind: CallableKindRequirement::Pure, span,
        }).collect();
        let params = parameters.iter().enumerate().map(|(index, id)| SchemeParameter {
            name: Name::intern(&format!("parameter{index}")), ty: SchemeType::Parameter(*id),
            schema_expectation: None, defaulted: false, rest: false,
        }).collect();
        let callable = CallableScheme::new(parameters.clone(), Vec::new(), params,
            SchemeType::Parameter(parameters[2]), None, false,
            CallableRequirements { iteration_parameters: BTreeMap::new(), operations: Vec::new(), methods: Vec::new(), wrappers: Vec::new(), map_key_parameters: Vec::new(), conversion_parameters: BTreeMap::new(), effect_parameters: effects.into(), effect_predicates: Vec::new(), applications }).unwrap();
        TypeScheme::with_callable(CallableKind::Pure, Arc::new(callable), CheckedEffects::finite(Vec::new()), true).unwrap()
    }

    fn scope(store: &FullStore, scheme: &FullScheme) -> SchemeScope {
        make_scope(store,
            bounded(scheme.type_parameters, &store.scheme_type_parameters, "types").unwrap().to_vec(),
            Vec::new(), effects::EffectScope::new(bounded(scheme.effect_parameters,
                &store.scheme_effect_parameters, "effects").unwrap().to_vec()).unwrap()).unwrap()
    }

    #[test]
    fn prepared_applications_reject_same_owner_forwarded_ordinal_swaps() {
        SymbolOwner::new().with_current(|| {
            let mut builder = super::super::super::FullBuilder::new(SourceId::new(0));
            let callee = builder.intern_scheme(&application_scheme(10, 30)).unwrap();
            let caller = builder.intern_scheme(&application_scheme(40, 60)).unwrap();
            let callee = builder.store.scheme(callee).unwrap();
            let caller = builder.store.scheme(caller).unwrap();
            let callee_scope = scope(&builder.store, callee);
            let caller_scope = scope(&builder.store, caller);
            let substitution = SymbolicCallSubstitution {
                types: (40..44).map(|id| SchemeType::Parameter(SchemeTypeParameterId(id))).collect(), rows: Vec::new(),
                effects: (60..62).map(|id| EffectContract::variable(SchemeEffectParameterId(id))).collect(),
            };
            let verify = |bindings: &[FullApplicationBinding]| verify_bindings(&builder.store, callee, &callee_scope,
                ApplicationSubstitution::Symbolic(&substitution), Some((caller, &caller_scope)), bindings,
                &mut SchemeInstantiationBudget::new(&builder.store));
            verify(&[FullApplicationBinding::Forwarded(0), FullApplicationBinding::Forwarded(1)]).unwrap();
            assert!(verify(&[FullApplicationBinding::Forwarded(1), FullApplicationBinding::Forwarded(0)])
                .unwrap_err().message.contains("actual caller requirement"));
            assert!(verify(&[FullApplicationBinding::Forwarded(0)]).unwrap_err().message.contains("incomplete"));
            assert!(verify(&[FullApplicationBinding::Forwarded(0), FullApplicationBinding::Forwarded(2)])
                .unwrap_err().message.contains("out of scope"));
        });
    }

    #[test]
    fn prepared_applications_anchor_result_binders_in_actual_inputs() {
        SymbolOwner::new().with_current(|| {
            let parameters = (10..14).map(SchemeTypeParameterId).collect::<Vec<_>>();
            let span = Span::at(SourceId::new(0), 0);
            let applications = (0..2).map(|index| CallableApplication {
                callee: SchemeType::Parameter(parameters[index]),
                arguments: vec![CallableApplicationArgument { kind: StaticArgumentKind::Positional,
                    ty: if index == 0 { SchemeType::Ground(Type::Int) } else { SchemeType::Parameter(parameters[2]) }, span }].into(),
                result: SchemeType::Parameter(parameters[index + 2]),
                effects: EffectContract::finite(EffectSet::default()), proc_returns: Arc::from([]),
                kind: CallableKindRequirement::Pure, span,
            }).collect();
            let params = parameters[..2].iter().enumerate().map(|(index, id)| SchemeParameter {
                name: Name::intern(&format!("callback{index}")), ty: SchemeType::Parameter(*id),
                schema_expectation: None, defaulted: false, rest: false,
            }).collect();
            let callable = CallableScheme::new(parameters, Vec::new(), params, SchemeType::Parameter(SchemeTypeParameterId(13)),
                None, false, CallableRequirements { iteration_parameters: BTreeMap::new(), operations: Vec::new(), methods: Vec::new(), wrappers: Vec::new(), map_key_parameters: Vec::new(), conversion_parameters: BTreeMap::new(), effect_parameters: Vec::new(), effect_predicates: Vec::new(), applications }).unwrap();
            let scheme = TypeScheme::with_callable(CallableKind::Pure, Arc::new(callable), CheckedEffects::finite(Vec::new()), true).unwrap();
            let mut builder = super::super::super::FullBuilder::new(SourceId::new(0));
            let id = builder.intern_scheme(&scheme).unwrap();
            let scheme = *builder.store.scheme(id).unwrap();
            let initial = || SchemeBinderEvidence { types: BTreeSet::from([0, 1]), rows: BTreeSet::new() };
            let mut evidence = initial();
            extend_call_binder_evidence(&builder.store, &scheme, &scope(&builder.store, &scheme), ApplicationSubstitution::Identity, &mut evidence, &mut SchemeInstantiationBudget::new(&builder.store)).unwrap();
            assert_eq!(evidence.types, BTreeSet::from([0, 1, 2, 3]));
            let applications = bounded(scheme.applications, &builder.store.scheme_applications, "applications").unwrap();
            let first = applications[0];
            let last_result = applications[1].result;
            builder.store.scheme_application_arguments[first.arguments.start as usize].ty = last_result;
            let mut evidence = initial();
            extend_call_binder_evidence(&builder.store, &scheme, &scope(&builder.store, &scheme), ApplicationSubstitution::Identity, &mut evidence, &mut SchemeInstantiationBudget::new(&builder.store)).unwrap();
            assert_eq!(evidence.types, BTreeSet::from([0, 1]));
        });
    }

    #[test]
    fn prepared_applications_authenticate_actual_default_and_rest_layouts() {
        SymbolOwner::new().with_current(|| {
            let mut builder = super::super::super::FullBuilder::new(SourceId::new(0));
            let id = builder.intern_scheme(&application_scheme(10, 30)).unwrap();
            let scheme = *builder.store.scheme(id).unwrap();
            let scope = scope(&builder.store, &scheme);
            let callback = |ty: Type, rest: bool, defaulted: bool| Type::Callable(Arc::new(TypedCallable {
                kind: crate::sema::types::CallableKind::Pure, sig: CallableType {
                    params: if rest { vec![CallableParamType { name: Name::intern("items"), ty: Type::List(Box::new(ty.clone())), defaulted: false, rest: true }] }
                        else { vec![CallableParamType { name: Name::intern("payload"), ty: ty.clone(), defaulted: false, rest: false },
                            CallableParamType { name: Name::intern("scale"), ty: Type::Int, defaulted, rest: false }] },
                    return_ty: Box::new(ty), effects: crate::sema::types::CallableEffects::from_clause(Some(Vec::new())),
                },
            }));
            let mut substitution = CallSubstitution {
                types: vec![callback(Type::Int, false, true), callback(Type::Str, true, false), Type::Int, Type::Str],
                rows: Vec::new(), effects: vec![EffectContract::finite(EffectSet::default()); 2],
            };
            builder.store.call_application_slots.extend([0, 1, 0]);
            let bindings = [FullApplicationBinding::Selected { argument_slots: IrRange { start: 0, len: 1 }, omitted_slots: IrRange { start: 1, len: 1 } },
                FullApplicationBinding::Selected { argument_slots: IrRange { start: 2, len: 1 }, omitted_slots: IrRange::default() }];
            let verify = |substitution: &CallSubstitution, bindings: &[FullApplicationBinding]| verify_bindings(&builder.store, &scheme, &scope,
                ApplicationSubstitution::Ground(substitution), None, bindings, &mut SchemeInstantiationBudget::new(&builder.store));
            verify(&substitution, &bindings).unwrap();
            let mut wrong = bindings;
            wrong[0] = FullApplicationBinding::Selected { argument_slots: IrRange { start: 1, len: 1 }, omitted_slots: IrRange { start: 0, len: 1 } };
            assert!(verify(&substitution, &wrong).unwrap_err().message.contains("actual callable"));
            substitution.types[0] = callback(Type::Int, false, false);
            assert!(verify(&substitution, &bindings).unwrap_err().message.contains("invalid argument layout"));
        });
    }
}

pub(super) fn equivalent_contracts(store: &FullStore, left_scope: &SchemeScope, left: FullSchemeApplication, right_scope: &SchemeScope, right: FullSchemeApplication, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    let left = decode(store, left_scope, left, &ApplicationSubstitution::Identity, budget)?;
    let right = decode(store, right_scope, right, &ApplicationSubstitution::Identity, budget)?;
    same_application(&left, &right, budget)
}
