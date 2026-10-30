use super::{FullStore, SchemeScope, SchemeInstantiationBudget, IrVerifyError, bounded, BTreeSet, body_types_equal, FullScheme};
use super::applications::ApplicationSubstitution;
use super::super::schemes::{FullMethodBinding, FullSchemeMethod};
use crate::sema::schemes::{MethodFamilyRequirement, MethodRequirementKind, SchemeType};
use crate::sema::types::Type;
use crate::syntax::node::Effect;
use xsh_registry::signature::{RegisteredFunctionId, RegisteredMethodId};

enum MethodSelection {
    Selected(RegisteredMethodId),
    SelectedFunction(RegisteredFunctionId),
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
    let slots = bounded(encoded.argument_slots, &store.scheme_method_slots, "scheme method argument slot range is invalid")?;
    let arguments = bounded(encoded.arguments, &store.scheme_method_arguments, "scheme method argument range is invalid")?;
    if slots.len() != arguments.len() || slots.iter().copied().collect::<BTreeSet<_>>().len() != slots.len() {
        return Err(IrVerifyError::new("scheme method argument layout is invalid"));
    }
    budget.charge(2 + arguments.len())?;
    Ok(MethodFamilyRequirement {
        kind: encoded.kind,
        receiver: substitution.ty(store, scope, encoded.receiver, budget)?,
        argument_slots: slots.to_vec(),
        arguments: arguments.iter().map(|argument| substitution.ty(store, scope, *argument, budget)).collect::<Result<_, _>>()?,
        result: substitution.ty(store, scope, encoded.result, budget)?,
    })
}

pub(super) fn authenticates_result_evidence(
    store: &FullStore,
    scope: &SchemeScope,
    encoded: FullSchemeMethod,
    substitution: &ApplicationSubstitution<'_>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<bool, IrVerifyError> {
    let requirement = decode(store, scope, encoded, substitution, budget)?;
    validate_requirement_for_binding(&requirement, budget)?;
    if !matches!(crate::modules::api_spec().method_requirement_result(requirement.kind), Some(crate::modules::signature::MethodFamilyResult::SelectedSignature)) { return Ok(false); }
    if matches!(substitution, ApplicationSubstitution::Ground(_)) {
        let MethodSelection::Selected(selected) = select(&requirement, budget)? else { return Err(IrVerifyError::new("method output evidence has no canonical input selection")); };
        let selected_return = selected_return_type(&requirement, selected, budget)?;
        if requirement.result.closed_ground_type().as_ref() != Some(&selected_return) {
            return Err(IrVerifyError::new("method output evidence disagrees with its canonical result"));
        }
    }
    Ok(true)
}

pub(super) fn owns_selected_result(
    store: &FullStore,
    scope: &SchemeScope,
    actual: &SchemeType,
    budget: &mut SchemeInstantiationBudget,
) -> Result<bool, IrVerifyError> {
    if !matches!(actual, SchemeType::Parameter(_)) { return Ok(false); }
    for encoded in bounded(scope.methods, &store.scheme_methods, "scheme method range is invalid")? {
        budget.charge(1)?;
        let requirement = decode(store, scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
        if !body_types_equal(actual, &requirement.result) { continue; }
        validate_requirement_for_binding(&requirement, budget)?;
        if matches!(crate::modules::api_spec().method_requirement_result(requirement.kind), Some(crate::modules::signature::MethodFamilyResult::SelectedSignature)) { return Ok(true); }
    }
    Ok(false)
}

pub(super) fn written_path_return_matches(
    store: &FullStore,
    owner: u32,
    scope: &SchemeScope,
    actual: &SchemeType,
    expected: &SchemeType,
    budget: &mut SchemeInstantiationBudget,
) -> Result<bool, IrVerifyError> {
    let owner = super::IrFunctionId::from_raw(owner).ok_or_else(|| IrVerifyError::new("method return proof owner is invalid"))?;
    let function = store.functions.get(owner.index()).ok_or_else(|| IrVerifyError::new("method return proof owner is out of bounds"))?;
    let super::super::FullCallableContract::Scheme(scheme) = function.contract else { return Ok(false); };
    let scheme = store.scheme(scheme)?;
    if !scheme.explicit_return || scheme.methods != scope.methods { return Ok(false); }
    let header = super::decode_scheme_type(store, scope, scheme.return_type, budget, 0)?;
    if !body_types_equal(&header, expected) { return Err(IrVerifyError::new("method return proof differs from its actual owner header")); }
    budget.charge_scheme_clone(actual)?;
    let payload = match (scheme.callable_kind, &header, actual) {
        (crate::sema::types::CallableKind::Pure, SchemeType::Ground(Type::Path), actual) => actual,
        (crate::sema::types::CallableKind::Proc, SchemeType::Result(ok, error), SchemeType::Result(actual_ok, actual_error))
            if **ok == SchemeType::Ground(Type::Path) && actual_error.matches_body_return(error) => actual_ok,
        (crate::sema::types::CallableKind::Proc, SchemeType::Ground(Type::Result(ok, error)), SchemeType::Result(actual_ok, actual_error))
            if **ok == Type::Path => {
                budget.charge_clone(error)?;
                if !actual_error.matches_body_return(&SchemeType::Ground((**error).clone())) { return Ok(false); }
                actual_ok
            }
        _ => return Ok(false),
    };
    if !matches!(payload, SchemeType::Parameter(_)) { return Ok(false); }
    for encoded in bounded(scope.methods, &store.scheme_methods, "scheme method range is invalid")? {
        budget.charge(1)?;
        let requirement = decode(store, scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
        if !body_types_equal(payload, &requirement.result) { continue; }
        validate_requirement_for_binding(&requirement, budget)?;
        budget.charge(crate::modules::api_spec().method_requirement_member_count(requirement.kind))?;
        if crate::modules::api_spec().method_requirement_returns_paths(requirement.kind) { return Ok(true); }
    }
    Ok(false)
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
    let methods = bounded(scope.owned_requirements(callee)?.methods, &store.scheme_methods, "scheme method range is invalid")?;
    if methods.len() != bindings.len() {
        return Err(IrVerifyError::new("generic call method binding is incomplete"));
    }
    budget.charge(methods.len())?;
    for (&encoded, &binding) in methods.iter().zip(bindings) {
        let requirement = decode(store, scope, encoded, &substitution, budget)?;
        validate_requirement_for_binding(&requirement, budget)?;
        match (select(&requirement, budget)?, binding) {
            (MethodSelection::Selected(expected), FullMethodBinding::Selected(actual)) if expected == actual => {
                let selected_return = selected_return_type(&requirement, expected, budget)?;
                if requirement.result.closed_ground_type().as_ref() != Some(&selected_return) {
                    return Err(IrVerifyError::new("selected method differs from its canonical result signature"));
                }
            }
            (MethodSelection::Selected(_), FullMethodBinding::Selected(_)) => {
                return Err(IrVerifyError::new("selected method differs from its canonical input signature"));
            }
            (MethodSelection::SelectedFunction(expected), FullMethodBinding::SelectedFunction(actual)) if expected == actual => {
                let contract = function_contract(&requirement, budget)?;
                if requirement.result.closed_ground_type().as_ref() != Some(contract.return_ty) {
                    return Err(IrVerifyError::new("selected function differs from its canonical result signature"));
                }
            }
            (MethodSelection::SelectedFunction(_), FullMethodBinding::SelectedFunction(_)) => {
                return Err(IrVerifyError::new("selected function differs from its canonical input signature"));
            }
            (MethodSelection::Selected(_), FullMethodBinding::SelectedFunction(_))
            | (MethodSelection::SelectedFunction(_), FullMethodBinding::Selected(_)) => {
                return Err(IrVerifyError::new("selected member belongs to another operation kind"));
            }
            (MethodSelection::Unresolved, FullMethodBinding::SelectedFunction(_)) => {
                return Err(IrVerifyError::new("selected function has unresolved canonical inputs"));
            }
            (MethodSelection::SelectedFunction(_), FullMethodBinding::Forwarded(_)) => {
                return Err(IrVerifyError::new("resolved function requirement is forwarded instead of selected"));
            }
            (MethodSelection::Unresolved, FullMethodBinding::Forwarded(ordinal)) => {
                let Some((caller, caller_scope)) = caller else {
                    return Err(IrVerifyError::new("ground generic call forwards a method family"));
                };
                let caller_method = bounded(caller_scope.owned_requirements(caller)?.methods, &store.scheme_methods, "caller method range is invalid")?
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

fn selected_return_type(
    requirement: &MethodFamilyRequirement<SchemeType>,
    selected: RegisteredMethodId,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Type, IrVerifyError> {
    budget.charge_scheme_clone(&requirement.receiver)?;
    let receiver = requirement.receiver.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected method has an unresolved receiver"))?;
    crate::modules::api_spec().registered_method(selected).ok_or_else(|| IrVerifyError::new("selected method is not registered"))?;
    crate::modules::api_spec().method_requirement_return_type(requirement.kind, selected, &receiver)
        .ok_or_else(|| IrVerifyError::new("selected method has no canonical result for its receiver"))
}

fn validate_requirement_for_binding(
    requirement: &MethodFamilyRequirement<SchemeType>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    reject_any_or_recovery(std::iter::once(&requirement.receiver)
        .chain(requirement.arguments.iter())
        .chain(std::iter::once(&requirement.result)), budget)?;
    if matches!(requirement.kind, MethodRequirementKind::Function(_)) {
        function_contract(requirement, budget)?;
    } else {
        family_contract(requirement.kind, budget)?;
    }
    Ok(())
}

pub(super) fn function_contract<'a>(
    requirement: &MethodFamilyRequirement<SchemeType>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<crate::modules::signature::FunctionRequirementContract<'a>, IrVerifyError> {
    let MethodRequirementKind::Function(function) = requirement.kind else {
        return Err(IrVerifyError::new("generic function requirement has a method kind"));
    };
    let api = crate::modules::api_spec();
    budget.charge(api.function_requirement_members(function).count())?;
    let contract = api.function_requirement_contract(function)
        .ok_or_else(|| IrVerifyError::new("generic function requirement has inconsistent canonical contracts"))?;
    if requirement.receiver != SchemeType::Ground(Type::Unit) {
        return Err(IrVerifyError::new("generic function requirement has a receiver"));
    }
    if body_types_equal(&requirement.result, &SchemeType::Ground(contract.return_ty.clone())) {
        Ok(contract)
    } else {
        Err(IrVerifyError::new("generic function requirement disagrees with its canonical result contract"))
    }
}

pub(super) fn verify_function_body_site(
    store: &FullStore,
    scope: &SchemeScope,
    ordinal: u32,
    op: crate::modules::RuntimeOp,
    arguments: &[Option<SchemeType>],
    result: &SchemeType,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let encoded = bounded(scope.methods, &store.scheme_methods, "scheme method range is invalid")?
        .get(ordinal as usize)
        .copied()
        .ok_or_else(|| IrVerifyError::new("body function ordinal is out of scope"))?;
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    validate_requirement_for_binding(&requirement, budget)?;
    let contract = function_contract(&requirement, budget)?;
    let supplied = arguments.iter().filter(|argument| argument.is_some()).count();
    if contract.op != op || supplied != requirement.arguments.len() || !body_types_equal(result, &SchemeType::Ground(contract.return_ty.clone())) {
        return Err(IrVerifyError::new("generic function site disagrees with its retained overload contract"));
    }
    budget.charge(arguments.len())?;
    for (expected, &slot) in requirement.arguments.iter().zip(&requirement.argument_slots) {
        match arguments.get(slot as usize) {
            Some(Some(actual)) if body_types_equal(actual, expected) => {}
            _ => return Err(IrVerifyError::new("generic function site disagrees with its retained overload contract")),
        }
    }
    Ok(())
}

fn validate_requirement(
    name: &str,
    requirement: &MethodFamilyRequirement<SchemeType>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    validate_requirement_for_binding(requirement, budget)?;
    let (family_name, return_type, _, _) = family_contract(requirement.kind, budget)?;
    let result_matches = match return_type {
        crate::modules::signature::MethodFamilyResult::UniformGround(return_type) => body_types_equal(&requirement.result, &SchemeType::Ground(return_type)),
        crate::modules::signature::MethodFamilyResult::SelectedSignature if matches!(requirement.kind, MethodRequirementKind::Named(_)) => {
            crate::modules::api_spec().method_requirement_members(requirement.kind).any(|(_, _, _, member)| scheme_result_may_match(&requirement.result, &member.sig.return_ty))
        }
        crate::modules::signature::MethodFamilyResult::SelectedSignature => match &requirement.result {
            SchemeType::Parameter(_) => true,
            SchemeType::Ground(actual) => crate::modules::api_spec().method_requirement_members(requirement.kind).any(|(_, _, _, member)| *actual == member.sig.return_ty),
            SchemeType::Validated(_, _) => requirement.result.closed_ground_type().is_some_and(|actual| crate::modules::api_spec().method_requirement_members(requirement.kind).any(|(_, _, _, member)| actual == member.sig.return_ty)),
            _ => false,
        },
    };
    if family_name != name || !result_matches {
        return Err(IrVerifyError::new("generic method family disagrees with its canonical result contract"));
    }
    let api = crate::modules::api_spec();
    budget.charge(api.method_requirement_member_count(requirement.kind))?;
    let mut possible = false;
    for (_, domain, _, signature) in api.method_requirement_members(requirement.kind) {
        budget.charge(1)?;
        possible |= receiver_may_match(domain, &requirement.receiver)
            && argument_layout_may_match(&signature.sig.params, &requirement.argument_slots, &requirement.arguments);
    }
    if !possible { return Err(IrVerifyError::new("generic method family has no canonical input layout")); }
    Ok(())
}

fn family_contract(
    kind: MethodRequirementKind,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(&'static str, crate::modules::signature::MethodFamilyResult, bool, Vec<Effect>), IrVerifyError> {
    let api = crate::modules::api_spec();
    let mut members = api.method_requirement_members(kind);
    let Some((_, _, name, first)) = members.next() else {
        return Err(IrVerifyError::new("generic method family has no canonical members"));
    };
    budget.charge(1)?;
    let return_type = api.method_requirement_result(kind).ok_or_else(|| IrVerifyError::new("generic method family has inconsistent canonical result contracts"))?;
    if let crate::modules::signature::MethodFamilyResult::UniformGround(ty) = &return_type { budget.charge_clone(ty)?; }
    let pure = first.sig.pure;
    let effects = first.sig.effects.to_vec();
    for (_, _, candidate_name, candidate) in members {
        budget.charge(1 + candidate.sig.effects.len())?;
        if candidate_name != name || candidate.sig.pure != pure || candidate.sig.effects != effects {
            return Err(IrVerifyError::new("generic method family has inconsistent canonical contracts"));
        }
    }
    Ok((name, return_type, pure, effects))
}

fn scheme_result_may_match(actual: &SchemeType, declared: &Type) -> bool {
    match (actual, declared.unvalidated()) {
        (SchemeType::Parameter(_), _) | (_, Type::BuiltinParameter(_)) => true,
        (SchemeType::Ground(actual), declared) => type_may_match(actual, declared),
        (SchemeType::Validated(_, inner), declared) => scheme_result_may_match(inner, declared),
        (SchemeType::List(actual), Type::List(declared)) | (SchemeType::Set(actual), Type::Set(declared)) | (SchemeType::Stream(actual), Type::Stream(declared))
            | (SchemeType::Optional(actual), Type::Optional(declared)) => scheme_result_may_match(actual, declared),
        (SchemeType::Map(actual_key, actual_value), Type::Map(declared_key, declared_value))
            | (SchemeType::Result(actual_key, actual_value), Type::Result(declared_key, declared_value)) =>
            scheme_result_may_match(actual_key, declared_key) && scheme_result_may_match(actual_value, declared_value),
        _ => false,
    }
}

fn type_may_match(actual: &Type, declared: &Type) -> bool {
    match (actual.unvalidated(), declared.unvalidated()) {
        (_, Type::BuiltinParameter(_)) => true,
        (Type::List(actual), Type::List(declared)) | (Type::Set(actual), Type::Set(declared)) | (Type::Stream(actual), Type::Stream(declared))
            | (Type::Optional(actual), Type::Optional(declared)) => type_may_match(actual, declared),
        (Type::Map(actual_key, actual_value), Type::Map(declared_key, declared_value))
            | (Type::Result(actual_key, actual_value), Type::Result(declared_key, declared_value)) =>
            type_may_match(actual_key, declared_key) && type_may_match(actual_value, declared_value),
        (actual, declared) => !declared.contains_builtin_parameter() && actual == declared,
    }
}

fn receiver_may_match(receiver: crate::modules::MethodReceiver, ty: &SchemeType) -> bool {
    use crate::modules::MethodReceiver as R;
    match ty {
        SchemeType::Parameter(_) => true,
        SchemeType::Validated(_, inner) => receiver_may_match(receiver, inner),
        SchemeType::Ground(ty) => receiver_type_matches(receiver, ty),
        SchemeType::List(_) => receiver == R::List,
        SchemeType::Map(_, _) => receiver == R::Map,
        SchemeType::Set(_) => receiver == R::Set,
        SchemeType::Stream(_) => receiver == R::Stream,
        SchemeType::Result(_, _) => receiver == R::Result,
        SchemeType::Record { .. } => receiver == R::Record,
        _ => false,
    }
}

fn receiver_type_matches(receiver: crate::modules::MethodReceiver, ty: &Type) -> bool {
    use crate::modules::MethodReceiver as R;
    if receiver == R::RelPath && matches!(ty, Type::Validated(validated) if validated.validation() == crate::sema::validated::Validation::RelPath) { return true; }
    crate::modules::signature::method_receiver_for_type(ty.unvalidated()) == Some(receiver)
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
        SchemeType::Ground(actual) => type_may_match(actual, expected),
        SchemeType::List(_) | SchemeType::Set(_) | SchemeType::Map(_, _) | SchemeType::Stream(_) | SchemeType::Optional(_) | SchemeType::Result(_, _) => expected.contains_builtin_parameter(),
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
    if let MethodRequirementKind::Function(function) = requirement.kind {
        budget.charge(crate::modules::api_spec().function_requirement_members(function).count())?;
        return Ok(match crate::modules::api_spec().select_function_requirement(function, &requirement.argument_slots, &arguments) {
            crate::modules::signature::FunctionSelection::Selected(selected) => MethodSelection::SelectedFunction(selected),
            crate::modules::signature::FunctionSelection::Unsupported => MethodSelection::Unsupported,
            crate::modules::signature::FunctionSelection::Ambiguous => MethodSelection::Ambiguous,
        });
    }
    budget.charge(crate::modules::api_spec().method_requirement_member_count(requirement.kind))?;
    Ok(match crate::modules::api_spec().select_method_requirement(
        requirement.kind,
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
    left.kind == right.kind
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
