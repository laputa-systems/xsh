use super::{FullStore, SchemeScope, SchemeInstantiationBudget, IrVerifyError, bounded, BTreeSet, body_types_equal, FullScheme};
use super::applications::ApplicationSubstitution;
use super::super::schemes::{FullMethodBinding, FullSchemeMethod};
use crate::sema::schemes::{MethodFamilyRequirement, SchemeType};
use crate::sema::types::Type;
use crate::syntax::node::Effect;
use xsh_registry::signature::{MethodFamilyId, RegisteredMethodId};

enum MethodSelection {
    Selected(RegisteredMethodId),
    Unresolved,
    Unsupported,
    Ambiguous,
}

pub(super) fn decode(
    store: &FullStore,
    scope: &SchemeScope,
    encoded: FullSchemeMethod,
    substitution: &ApplicationSubstitution<'_>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<MethodFamilyRequirement<SchemeType>, IrVerifyError> {
    budget.charge(1)?;
    if encoded.reserved != [0; 3] {
        return Err(IrVerifyError::new("scheme method metadata is invalid"));
    }
    let slots = bounded(encoded.argument_slots, &store.scheme_method_slots, "scheme method argument slot range is invalid")?;
    let arguments = bounded(encoded.arguments, &store.scheme_method_arguments, "scheme method argument range is invalid")?;
    if slots.len() != arguments.len() || slots.iter().copied().collect::<BTreeSet<_>>().len() != slots.len() {
        return Err(IrVerifyError::new("scheme method argument layout is invalid"));
    }
    budget.charge(2 + arguments.len())?;
    Ok(MethodFamilyRequirement {
        family: encoded.family,
        receiver: substitution.ty(store, scope, encoded.receiver, budget)?,
        argument_slots: slots.to_vec(),
        arguments: arguments.iter().map(|argument| substitution.ty(store, scope, *argument, budget)).collect::<Result<_, _>>()?,
        result: substitution.ty(store, scope, encoded.result, budget)?,
    })
}

pub(super) fn verify_body_site(
    store: &FullStore,
    scope: &SchemeScope,
    ordinal: u32,
    name: &str,
    receiver: &SchemeType,
    physical_arguments: &[SchemeType],
    result: &SchemeType,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let encoded = bounded(scope.methods, &store.scheme_methods, "scheme method range is invalid")?
        .get(ordinal as usize)
        .copied()
        .ok_or_else(|| IrVerifyError::new("body method ordinal is out of scope"))?;
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    if !body_types_equal(receiver, &requirement.receiver)
        || physical_arguments.len() != requirement.arguments.len()
        || requirement.argument_slots.iter().any(|slot| *slot as usize >= physical_arguments.len())
        || !body_types_equal(result, &requirement.result)
    {
        return Err(IrVerifyError::new("generic method site disagrees with its retained family contract"));
    }
    budget.charge(physical_arguments.len())?;
    for (expected, slot) in requirement.arguments.iter().zip(&requirement.argument_slots) {
        let actual = &physical_arguments[*slot as usize];
        if !body_types_equal(actual, expected) {
            return Err(IrVerifyError::new("generic method site disagrees with its retained family contract"));
        }
    }
    validate_requirement(name, &requirement, budget)
}

pub(super) fn verify_bindings(
    store: &FullStore,
    callee: &FullScheme,
    scope: &SchemeScope,
    substitution: ApplicationSubstitution<'_>,
    caller: Option<(&FullScheme, &SchemeScope)>,
    bindings: &[FullMethodBinding],
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let methods = bounded(callee.methods, &store.scheme_methods, "scheme method range is invalid")?;
    if methods.len() != bindings.len() {
        return Err(IrVerifyError::new("generic call method binding is incomplete"));
    }
    budget.charge(methods.len())?;
    for (&encoded, &binding) in methods.iter().zip(bindings) {
        let requirement = decode(store, scope, encoded, &substitution, budget)?;
        validate_requirement_for_binding(&requirement, budget)?;
        match (select(&requirement, budget)?, binding) {
            (MethodSelection::Selected(expected), FullMethodBinding::Selected(actual)) if expected == actual => {}
            (MethodSelection::Selected(_), FullMethodBinding::Selected(_)) => {
                return Err(IrVerifyError::new("selected method differs from its canonical input signature"));
            }
            (MethodSelection::Unresolved, FullMethodBinding::Forwarded(ordinal)) => {
                let Some((caller, caller_scope)) = caller else {
                    return Err(IrVerifyError::new("ground generic call forwards a method family"));
                };
                let caller_method = bounded(caller.methods, &store.scheme_methods, "caller method range is invalid")?
                    .get(ordinal as usize)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("forwarded method ordinal is out of scope"))?;
                let actual = decode(store, caller_scope, caller_method, &ApplicationSubstitution::Identity, budget)?;
                if !requirements_equal(&requirement, &actual) {
                    return Err(IrVerifyError::new("forwarded method differs from its actual caller requirement"));
                }
            }
            (MethodSelection::Unresolved, FullMethodBinding::Selected(_)) => {
                return Err(IrVerifyError::new("selected method has unresolved canonical inputs"));
            }
            (MethodSelection::Selected(_), FullMethodBinding::Forwarded(_)) => {
                return Err(IrVerifyError::new("resolved method family is forwarded instead of selected"));
            }
            (MethodSelection::Unsupported, _) => {
                return Err(IrVerifyError::new("generic method inputs have no canonical family member"));
            }
            (MethodSelection::Ambiguous, _) => {
                return Err(IrVerifyError::new("generic method inputs select multiple canonical family members"));
            }
        }
    }
    Ok(())
}

fn validate_requirement_for_binding(
    requirement: &MethodFamilyRequirement<SchemeType>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    reject_any_or_recovery(std::iter::once(&requirement.receiver)
        .chain(requirement.arguments.iter())
        .chain(std::iter::once(&requirement.result)), budget)?;
    family_contract(requirement.family, budget)?;
    Ok(())
}

fn validate_requirement(
    name: &str,
    requirement: &MethodFamilyRequirement<SchemeType>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    validate_requirement_for_binding(requirement, budget)?;
    let (family_name, return_type, _, _) = family_contract(requirement.family, budget)?;
    if family_name != name || !body_types_equal(&requirement.result, &SchemeType::Ground(return_type)) {
        return Err(IrVerifyError::new("generic method family disagrees with its canonical result contract"));
    }
    let api = crate::modules::api_spec();
    budget.charge(api.method_family_member_count(requirement.family))?;
    let mut possible = false;
    for (_, domain, _, signature) in api.method_family_members(requirement.family) {
        budget.charge(1)?;
        possible |= receiver_may_match(domain, &requirement.receiver)
            && argument_layout_may_match(&signature.sig.params, &requirement.argument_slots, &requirement.arguments);
    }
    if !possible { return Err(IrVerifyError::new("generic method family has no canonical input layout")); }
    Ok(())
}

fn family_contract(
    family: MethodFamilyId,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(&'static str, Type, bool, Vec<Effect>), IrVerifyError> {
    let api = crate::modules::api_spec();
    let mut members = api.method_family_members(family);
    let Some((_, _, name, first)) = members.next() else {
        return Err(IrVerifyError::new("generic method family has no canonical members"));
    };
    budget.charge(1)?;
    let return_type = first.sig.return_ty.clone();
    budget.charge_clone(&return_type)?;
    let pure = first.sig.pure;
    let effects = first.sig.effects.to_vec();
    for (_, _, candidate_name, candidate) in members {
        budget.charge(1 + candidate.sig.effects.len())?;
        if candidate_name != name || candidate.sig.return_ty != return_type || candidate.sig.pure != pure || candidate.sig.effects != effects {
            return Err(IrVerifyError::new("generic method family has inconsistent canonical contracts"));
        }
    }
    Ok((name, return_type, pure, effects))
}

fn receiver_may_match(receiver: crate::modules::MethodReceiver, ty: &SchemeType) -> bool {
    use crate::modules::MethodReceiver as R;
    match ty {
        SchemeType::Parameter(_) => true,
        SchemeType::Validated(_, inner) => receiver_may_match(receiver, inner),
        SchemeType::Ground(ty) => receiver_type_matches(receiver, ty.unvalidated()),
        SchemeType::List(_) => receiver == R::List,
        SchemeType::Map(_, _) => receiver == R::Map,
        SchemeType::Set(_) => receiver == R::Set,
        _ => false,
    }
}

fn receiver_type_matches(receiver: crate::modules::MethodReceiver, ty: &Type) -> bool {
    use crate::modules::MethodReceiver as R;
    use Type as T;
    matches!((receiver, ty),
        (R::List, T::List(_))
            | (R::Map, T::Map(_, _))
            | (R::Set, T::Set(_))
            | (R::Bytes, T::Bytes)
            | (R::Str, T::Str)
            | (R::Path, T::Path))
}

fn argument_layout_may_match(
    parameters: &[crate::modules::signature::ParamSig],
    slots: &[u32],
    arguments: &[SchemeType],
) -> bool {
    if slots.len() != arguments.len() { return false; }
    let mut supplied = BTreeSet::new();
    for (&slot, argument) in slots.iter().zip(arguments) {
        let slot = slot as usize;
        let Some(parameter) = parameters.get(slot) else { return false; };
        if !supplied.insert(slot) || !argument_may_match(argument, &parameter.ty) { return false; }
    }
    parameters.iter().enumerate().all(|(slot, parameter)| parameter.defaulted || supplied.contains(&slot))
}

fn argument_may_match(actual: &SchemeType, expected: &Type) -> bool {
    match actual {
        SchemeType::Parameter(_) => true,
        SchemeType::Validated(_, inner) => argument_may_match(inner, expected),
        SchemeType::Ground(actual) => actual.unvalidated() == expected.unvalidated(),
        _ => false,
    }
}

fn select(
    requirement: &MethodFamilyRequirement<SchemeType>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<MethodSelection, IrVerifyError> {
    budget.charge_scheme_clone(&requirement.receiver)?;
    let Some(receiver) = requirement.receiver.closed_ground_type() else { return Ok(MethodSelection::Unresolved) };
    let mut arguments = Vec::with_capacity(requirement.arguments.len());
    for argument in &requirement.arguments {
        budget.charge_scheme_clone(argument)?;
        let Some(argument) = argument.closed_ground_type() else { return Ok(MethodSelection::Unresolved) };
        arguments.push(argument);
    }
    budget.charge(crate::modules::api_spec().method_family_member_count(requirement.family))?;
    Ok(match crate::modules::api_spec().select_method_family(
        requirement.family,
        &receiver,
        &requirement.argument_slots,
        &arguments,
    ) {
        crate::modules::signature::MethodFamilySelection::Selected(method) => MethodSelection::Selected(method),
        crate::modules::signature::MethodFamilySelection::Unsupported => MethodSelection::Unsupported,
        crate::modules::signature::MethodFamilySelection::Ambiguous => MethodSelection::Ambiguous,
    })
}

fn requirements_equal(
    left: &MethodFamilyRequirement<SchemeType>,
    right: &MethodFamilyRequirement<SchemeType>,
) -> bool {
    left.family == right.family
        && left.argument_slots == right.argument_slots
        && body_types_equal(&left.receiver, &right.receiver)
        && left.arguments.iter().zip(&right.arguments).all(|(left, right)| body_types_equal(left, right))
        && left.arguments.len() == right.arguments.len()
        && body_types_equal(&left.result, &right.result)
}

fn reject_any_or_recovery<'a>(
    types: impl Iterator<Item = &'a SchemeType>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let mut pending = types.collect::<Vec<_>>();
    while let Some(ty) = pending.pop() {
        budget.charge(1)?;
        match ty {
            SchemeType::Ground(ty) => {
                budget.charge_clone(ty)?;
                if ty.contains_any() || ty.contains_recovery() {
                    return Err(IrVerifyError::new("generic method family contains an ungrounded recovery type"));
                }
            }
            SchemeType::List(inner) | SchemeType::Set(inner) | SchemeType::Stream(inner) | SchemeType::Optional(inner) | SchemeType::Validated(_, inner) => pending.push(inner),
            SchemeType::Map(key, value) | SchemeType::Result(key, value) => { pending.push(key); pending.push(value); }
            SchemeType::Record { fields, .. } => pending.extend(fields.values()),
            SchemeType::Union(members) => pending.extend(members),
            SchemeType::SchemeCallable(_) | SchemeType::Parameter(_) | SchemeType::Callable { .. } => {}
        }
    }
    Ok(())
}
