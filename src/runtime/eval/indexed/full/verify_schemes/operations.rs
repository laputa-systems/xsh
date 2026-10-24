use super::{SchemeType, FullStore, SchemeScope, SchemeInstantiationBudget, IrVerifyError, IR_NONE, Name, bounded, body_type_matches, SymbolicCallSubstitution, BTreeSet, body_type_matches_ground, Type, decode_scheme_type, BTreeMap, verify_symbolic_argument_evidence, SymbolicEvidenceSource, CallSubstitution, body_types_equal, FullScheme, SchemeRowTail};
use super::applications::ApplicationSubstitution;
use super::super::schemes::{FullOperationBinding, FullSealedOperation};
use crate::sema::sealed_operation::{SealedBinaryOperation, SealedOperation, SealedOperationSignature, arithmetic_operator, arithmetic_selection, index_selection, slice_selection, record_override_selection, RecordOverrideField};

fn domain_accepts(domain: crate::sema::sealed_operation::ArithmeticDomain, ty: &SchemeType) -> bool {
    use crate::sema::sealed_operation::ArithmeticDomain as D;
    match ty {
        SchemeType::Parameter(_) => true,
        SchemeType::Ground(ty) => crate::sema::sealed_operation::arithmetic_domain_accepts(domain, ty),
        SchemeType::List(_) => domain == D::List,
        SchemeType::Set(_) => domain == D::Set,
        SchemeType::Validated(_, ty) => domain_accepts(domain, ty),
        _ => false,
    }
}

pub(super) fn decode(store: &FullStore, scope: &SchemeScope, encoded: FullSealedOperation, substitution: &ApplicationSubstitution<'_>, budget: &mut SchemeInstantiationBudget) -> Result<SealedOperation<SchemeType>, IrVerifyError> {
    budget.charge(1)?;
    let span = crate::source::Span::at(store.source_id, 0);
    Ok(match encoded {
        FullSealedOperation::Membership { needle, container } => SealedOperation::Membership { needle: substitution.ty(store, scope, needle, budget)?, container: substitution.ty(store, scope, container, budget)?, span },
        FullSealedOperation::TypePredicate { domain, subject: key } => SealedOperation::TypePredicate { domain, subject: substitution.ty(store, scope, key, budget)?, span },
        FullSealedOperation::Binary { left, right, result, operator } => SealedOperation::Binary(SealedBinaryOperation {
            operator, left: substitution.ty(store, scope, left, budget)?, right: substitution.ty(store, scope, right, budget)?, result: substitution.ty(store, scope, result, budget)?, span,
        }),
        FullSealedOperation::Index { receiver, key, result, syntax, constant_key } => SealedOperation::Index {
            receiver: substitution.ty(store, scope, receiver, budget)?, key: substitution.ty(store, scope, key, budget)?, result: substitution.ty(store, scope, result, budget)?, syntax,
            constant_key: if constant_key == IR_NONE { None } else { Some(Name::intern(store.string(constant_key)?)) }, span,
        },
        FullSealedOperation::Slice { receiver, result, bounds } => SealedOperation::Slice { receiver: substitution.ty(store, scope, receiver, budget)?, result: substitution.ty(store, scope, result, budget)?, bounds, span },
        FullSealedOperation::RecordOverride { receiver, overrides, result } => {
            let fields = bounded(overrides, &store.scheme_fields, "record override field range is invalid")?;
            budget.charge(fields.len())?;
            let overrides = fields.iter().map(|field| Ok(RecordOverrideField { name: Name::intern(store.string(field.name)?), ty: substitution.ty(store, scope, field.ty, budget)? })).collect::<Result<Vec<_>, IrVerifyError>>()?;
            SealedOperation::RecordOverride { receiver: substitution.ty(store, scope, receiver, budget)?, overrides: overrides.into(), result: substitution.ty(store, scope, result, budget)?, span }
        }
    })
}

fn selection(operation: &SealedOperation<SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<SealedOperationSignature, IrVerifyError> {
    let closed = operation.try_map(|ty| {
        budget.charge_scheme_clone(ty)?;
        ty.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected operation has no closed input tuple"))
    })?;
    if let SealedOperation::Membership { needle, container, .. } = &closed {
        return crate::sema::sealed_operation::membership_selection(container, needle).map(SealedOperationSignature::Membership)
            .ok_or_else(|| IrVerifyError::new("membership has no supported canonical domain"));
    }
    if let SealedOperation::TypePredicate { domain, subject: key, .. } = &closed {
        return if domain.accepts(key) { Ok(SealedOperationSignature::TypePredicate(*domain)) }
            else { Err(IrVerifyError::new("key predicate has no supported canonical domain")) };
    }
    let selected = match &closed {
        SealedOperation::Binary(operation) => arithmetic_selection(operation.operator, &operation.left, &operation.right).map(|(signature, result)| (SealedOperationSignature::Binary(signature), result)),
        SealedOperation::Index { receiver, key, constant_key, .. } => index_selection(receiver, key, *constant_key),
        SealedOperation::Slice { receiver, .. } => slice_selection(receiver),
        SealedOperation::RecordOverride { receiver, overrides, .. } => record_override_selection(receiver, overrides),
        SealedOperation::TypePredicate { .. } | SealedOperation::Membership { .. } => unreachable!(),
    };
    let (signature, result) = selected.ok_or_else(|| IrVerifyError::new("sealed operation input tuple has no canonical signature"))?;
    budget.charge_clone(&result)?;
    let matches = if matches!(operation, SealedOperation::RecordOverride { .. }) {
        operation.result().unwrap().closed_ground_type().as_ref() == Some(&result)
    } else { body_type_matches(&SchemeType::Ground(result), operation.result().unwrap(), budget)? };
    if !matches { return Err(IrVerifyError::new("sealed operation result disagrees with its input signature")); }
    Ok(signature)
}

pub(super) fn symbolic_output_binders(
    store: &FullStore, scope: &SchemeScope, encoded: FullSealedOperation,
    substitution: &SymbolicCallSubstitution, budget: &mut SchemeInstantiationBudget,
) -> Result<BTreeSet<usize>, IrVerifyError> {
    let mut evidence = BTreeSet::new();
    if matches!(encoded, FullSealedOperation::Membership { .. } | FullSealedOperation::Binary { .. } | FullSealedOperation::TypePredicate { .. }) { return Ok(evidence); }
    let operation = decode(store, scope, encoded, &ApplicationSubstitution::Symbolic(substitution), budget)?;
    let projection = match operation {
        SealedOperation::Index { mut receiver, key, constant_key, .. } => {
            while let SchemeType::Validated(_, inner) = receiver { budget.charge(1)?; receiver = *inner; }
            match receiver {
                SchemeType::List(item) if body_type_matches_ground(&key, &Type::Int) => Some(*item),
                SchemeType::Map(required, item) if body_type_matches(&key, &required, budget)? => Some(*item),
                SchemeType::Record { mut fields, .. } if body_type_matches_ground(&key, &Type::Str) => constant_key.and_then(|name| fields.remove(&name)),
                SchemeType::Ground(receiver) => {
                    budget.charge_clone(&receiver)?;
                    budget.charge_scheme_clone(&key)?;
                    key.closed_ground_type().and_then(|key| index_selection(&receiver, &key, constant_key).map(|(_, result)| SchemeType::Ground(result)))
                }
                _ => None,
            }
        }
        SealedOperation::Slice { mut receiver, .. } => {
            while let SchemeType::Validated(_, inner) = receiver { budget.charge(1)?; receiver = *inner; }
            match receiver {
                SchemeType::List(item) => Some(SchemeType::List(item)),
                SchemeType::Ground(receiver) => {
                    budget.charge_clone(&receiver)?;
                    slice_selection(&receiver).map(|(_, result)| SchemeType::Ground(result))
                }
                _ => None,
            }
        }
        SealedOperation::RecordOverride { mut receiver, overrides, .. } => {
            while let SchemeType::Validated(_, inner) = receiver { budget.charge(1)?; receiver = *inner; }
            let parameter = decode_scheme_type(store, scope, encoded.result().unwrap(), budget, 0)?;
            let SchemeType::Record { fields: expected, .. } = parameter else { return Ok(evidence) };
            let mut fields = match receiver {
                SchemeType::Record { fields, .. } => fields,
                SchemeType::Ground(receiver) => {
                    let Type::Record(fields) = receiver.unvalidated() else { return Ok(evidence) };
                    budget.charge(fields.len())?;
                    fields.iter().map(|(&name, ty)| { budget.charge_clone(ty)?; Ok((name, SchemeType::Ground(ty.clone()))) }).collect::<Result<BTreeMap<_, _>, IrVerifyError>>()?
                }
                _ => return Ok(evidence),
            };
            for field in overrides.iter() { budget.charge(1)?; budget.charge_scheme_clone(&field.ty)?; fields.insert(field.name, field.ty.clone()); }
            for (name, expected) in expected {
                budget.charge(1)?;
                if let Some(actual) = fields.get(&name) { verify_symbolic_argument_evidence(&expected, actual, scope, substitution, SymbolicEvidenceSource::Projection, &mut evidence, budget, 0)?; }
            }
            return Ok(evidence);
        }
        SealedOperation::Binary(_) | SealedOperation::TypePredicate { .. } | SealedOperation::Membership { .. } => unreachable!(),
    };
    if let Some(projection) = projection {
        let parameter = decode_scheme_type(store, scope, encoded.result().unwrap(), budget, 0)?;
        verify_symbolic_argument_evidence(&parameter, &projection, scope, substitution, SymbolicEvidenceSource::Projection, &mut evidence, budget, 0)?;
    }
    Ok(evidence)
}

pub(super) fn verify_ground(store: &FullStore, scope: &SchemeScope, substitution: &CallSubstitution, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    for encoded in bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")? {
        let operation = decode(store, scope, *encoded, &ApplicationSubstitution::Ground(substitution), budget)?;
        selection(&operation, budget)?;
    }
    Ok(())
}

pub(super) fn verify_key_domain(
    store: &FullStore, scope: &SchemeScope, domain: crate::sema::sealed_operation::TypeDomain,
    key: &SchemeType, budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    budget.charge_scheme_clone(key)?;
    if let Some(ty) = key.closed_ground_type() {
        return if domain.accepts(&ty) { Ok(()) } else { Err(IrVerifyError::new("pipeline key has an unsupported canonical domain")) };
    }
    for operation in bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")? {
        budget.charge(1)?;
        let FullSealedOperation::TypePredicate { domain: required, .. } = operation else { continue; };
        if *required != domain { continue; }
        let SealedOperation::TypePredicate { subject: required, .. } = decode(store, scope, *operation, &ApplicationSubstitution::Identity, budget)? else { unreachable!() };
        if body_types_equal(key, &required) { return Ok(()); }
    }
    Err(IrVerifyError::new("generic pipeline key lacks its scoped canonical domain predicate"))
}

pub(super) fn verify_bindings(store: &FullStore, callee: &FullScheme, scope: &SchemeScope, substitution: ApplicationSubstitution<'_>, caller: Option<(&FullScheme, &SchemeScope)>, bindings: &[FullOperationBinding], budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let operations = bounded(scope.owned_requirements(callee)?.operations, &store.scheme_operations, "scheme operation range is invalid")?;
    if operations.len() != bindings.len() { return Err(IrVerifyError::new("generic call operation binding is incomplete")); }
    for (encoded, binding) in operations.iter().zip(bindings) {
        let operation = decode(store, scope, *encoded, &substitution, budget)?;
        match *binding {
            FullOperationBinding::Selected(actual) => {
                if actual != selection(&operation, budget)? { return Err(IrVerifyError::new("selected operation differs from its canonical signature")); }
            }
            FullOperationBinding::Forwarded(ordinal) => {
                let Some((caller, caller_scope)) = caller else { return Err(IrVerifyError::new("ground operation has no forwarding owner")); };
                let encoded = bounded(caller_scope.owned_requirements(caller)?.operations, &store.scheme_operations, "caller operation range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("forwarded operation ordinal is out of scope"))?;
                let required = decode(store, caller_scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
                if !operation.same_kind(&required) || !operation.types().zip(required.types()).all(|(actual, required)| body_types_equal(actual, required)) {
                    return Err(IrVerifyError::new("forwarded operation differs from its actual caller requirement"));
                }
            }
        }
    }
    Ok(())
}

fn record_contract(ty: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<(BTreeMap<Name, SchemeType>, SchemeRowTail), IrVerifyError> {
    let mut ty = ty;
    for _ in 0..=128 {
        budget.charge(1)?;
        return match ty {
            SchemeType::Validated(_, inner) => { ty = inner; continue; }
            SchemeType::Record { fields, tail } => {
                budget.charge_scheme_clone(ty)?;
                Ok((fields.clone(), *tail))
            }
            SchemeType::Ground(ty) => {
                let Type::Record(fields) = ty.unvalidated() else { return Err(IrVerifyError::new("record override has no record domain")); };
                budget.charge(fields.len())?;
                let fields = fields.iter().map(|(&name, ty)| { budget.charge_clone(ty)?; Ok((name, SchemeType::Ground(ty.clone()))) }).collect::<Result<_, IrVerifyError>>()?;
                Ok((fields, SchemeRowTail::Closed))
            }
            _ => Err(IrVerifyError::new("record override has no record domain")),
        };
    }
    Err(IrVerifyError::new("record override type nesting is too deep"))
}

pub(super) fn verify_record_override_site(
    store: &FullStore, scope: &SchemeScope, ordinal: u32, receiver: &SchemeType,
    overrides: &[(Name, SchemeType)], result: &SchemeType, budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let encoded = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(ordinal as usize)
        .ok_or_else(|| IrVerifyError::new("record override ordinal is out of scope"))?;
    let SealedOperation::RecordOverride { receiver: required, overrides: fields, result: output, .. } = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)? else {
        return Err(IrVerifyError::new("record override site has another operation requirement"));
    };
    if !body_types_equal(receiver, &required) || !body_types_equal(result, &output) || fields.len() != overrides.len() {
        return Err(IrVerifyError::new("record override site disagrees with its owned requirement"));
    }
    let (mut derived, input_tail) = record_contract(receiver, budget)?;
    let (output_fields, output_tail) = record_contract(result, budget)?;
    for (required, (name, ty)) in fields.iter().zip(overrides) {
        budget.charge(1)?;
        if required.name != *name || !body_types_equal(&required.ty, ty) { return Err(IrVerifyError::new("record override order or field contract disagrees with its producer")); }
        budget.charge_scheme_clone(ty)?;
        derived.insert(*name, ty.clone());
    }
    for (name, ty) in &derived {
        budget.charge(1)?;
        if output_fields.get(name).is_none_or(|output| !body_types_equal(output, ty)) { return Err(IrVerifyError::new("record override result disagrees with its derived known fields")); }
    }
    match (input_tail, output_tail) {
        (SchemeRowTail::Closed, SchemeRowTail::Closed) if derived.len() == output_fields.len() => {}
        (SchemeRowTail::Parameter(input), SchemeRowTail::Parameter(output)) if input != output => {}
        _ => return Err(IrVerifyError::new("record override result has no independent remainder")),
    }
    Ok(())
}

pub(super) fn may_require_uint_key_validation(receiver: &SchemeType, key: &SchemeType) -> bool {
    let mut receiver = receiver;
    while let SchemeType::Validated(_, inner) = receiver { receiver = inner; }
    let required = match receiver {
        SchemeType::Map(required, _) => Some(required.as_ref()),
        SchemeType::Ground(ty) if matches!(ty.unvalidated(), Type::Map(..)) => {
            return key.closed_ground_type().is_none_or(|key| crate::sema::sealed_operation::may_require_uint_key_validation(ty, &key));
        }
        SchemeType::Parameter(_) => return !matches!(key.closed_ground_type(), Some(Type::UInt | Type::Str)),
        _ => None,
    };
    let Some(required) = required else { return false; };
    if required == key { return false; }
    let required = required.closed_ground_type();
    let key = key.closed_ground_type();
    match (required, key) {
        (Some(Type::UInt), Some(Type::Int)) => true,
        (Some(Type::UInt), _) => false,
        (_, Some(Type::UInt | Type::Str)) => false,
        _ => true,
    }
}

pub(super) fn requires_uint_key_validation(receiver: &SchemeType, key: &SchemeType) -> bool {
    receiver.closed_ground_type().zip(key.closed_ground_type())
        .is_some_and(|(receiver, key)| crate::sema::sealed_operation::requires_uint_key_validation(&receiver, &key))
}

pub(super) fn result_error_type(ty: &SchemeType) -> Option<Option<Type>> {
    match ty {
        SchemeType::Validated(_, inner) | SchemeType::Optional(inner) => result_error_type(inner),
        SchemeType::Result(_, error) => Some(error.closed_ground_type()),
        SchemeType::Ground(ty) => crate::sema::sealed_operation::result_error_contract(ty).cloned().map(Some),
        _ => None,
}
}

pub(super) fn verify_body_site(store: &FullStore, scope: &SchemeScope, ordinal: u32, operator: crate::syntax::node::BinaryOp, left: &SchemeType, right: &SchemeType, result: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let encoded = bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("body operation ordinal is out of scope"))?;
    let operation = decode(store, scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
    if let SealedOperation::Membership { needle, container, .. } = &operation {
        if !matches!(operator, crate::syntax::node::BinaryOp::In | crate::syntax::node::BinaryOp::NotIn)
            || !body_types_equal(left, needle) || !body_types_equal(right, container) || result != &SchemeType::Ground(Type::Bool)
        { return Err(IrVerifyError::new("generic membership site disagrees with its scoped requirement")); }
        if needle.closed_ground_type().is_some() && container.closed_ground_type().is_some() { selection(&operation, budget)?; }
        return Ok(());
    }
    let SealedOperation::Binary(operation) = operation else { return Err(IrVerifyError::new("generic binary site disagrees with its sealed operation")); };
    if arithmetic_operator(operator) != Some(operation.operator) || !body_types_equal(left, &operation.left) || !body_types_equal(right, &operation.right) || !body_types_equal(result, &operation.result) {
        return Err(IrVerifyError::new("generic binary site disagrees with its sealed operation"));
    }
    if !crate::sema::sealed_operation::ArithmeticSignature::ALL.into_iter().any(|signature| signature.accepts_operator(operation.operator) && {
        let (left, right) = signature.domains();
        domain_accepts(left, &operation.left) && domain_accepts(right, &operation.right)
    }) { return Err(IrVerifyError::new("generic binary site has no canonical input domain")); }
    if operation.left.closed_ground_type().is_some() && operation.right.closed_ground_type().is_some() { selection(&SealedOperation::Binary(operation), budget)?; }
    Ok(())
}

pub(super) fn verify_index_site(
    store: &FullStore, scope: &SchemeScope, ordinal: u32, receiver: &SchemeType, key: &SchemeType,
    result: &SchemeType, syntax: crate::sema::sealed_operation::IndexSyntax, constant_key: Option<Name>, budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let encoded = bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("body index ordinal is out of scope"))?;
    let operation = decode(store, scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
    let SealedOperation::Index { receiver: required_receiver, key: required_key, result: required_result, syntax: required_syntax, constant_key: required_constant, .. } = &operation else {
        return Err(IrVerifyError::new("generic index site has another operation kind"));
    };
    if !body_types_equal(receiver, required_receiver) || !body_types_equal(key, required_key) || !body_types_equal(result, required_result)
        || syntax != *required_syntax || constant_key != *required_constant
    { return Err(IrVerifyError::new("generic index site disagrees with its sealed operation")); }
    let supported = match receiver {
        SchemeType::Parameter(_) => true,
        SchemeType::List(_) => body_type_matches_ground(key, &Type::Int),
        SchemeType::Map(required, _) => body_type_matches(key, required, budget)? || may_require_uint_key_validation(receiver, key),
        SchemeType::Record { fields, .. } => constant_key.is_some_and(|key| fields.contains_key(&key)),
        SchemeType::Ground(ground_receiver) => match ground_receiver.unvalidated() {
            Type::List(_) => body_type_matches_ground(key, &Type::Int),
            Type::Map(required, _) => body_type_matches_ground(key, required) || may_require_uint_key_validation(receiver, key),
            Type::Record(fields) => constant_key.is_some_and(|key| fields.contains_key(&key)),
            Type::Module(module) => constant_key.is_some_and(|key| module.exports.contains_key(&key)),
            _ => false,
        },
        _ => false,
    };
    if !supported { return Err(IrVerifyError::new("generic index site has no supported input domain")); }
    if receiver.closed_ground_type().is_some() && key.closed_ground_type().is_some() && result.closed_ground_type().is_some() { selection(&operation, budget)?; }
    Ok(())
}

pub(super) fn verify_slice_site(
    store: &FullStore, scope: &SchemeScope, ordinal: u32, receiver: &SchemeType, result: &SchemeType,
    bounds: crate::sema::sealed_operation::SliceBounds, budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let encoded = bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("body slice ordinal is out of scope"))?;
    let operation = decode(store, scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
    let SealedOperation::Slice { receiver: required_receiver, result: required_result, bounds: required_bounds, .. } = &operation else {
        return Err(IrVerifyError::new("generic slice site has another operation kind"));
    };
    if !body_types_equal(receiver, required_receiver) || !body_types_equal(result, required_result) || bounds != *required_bounds {
        return Err(IrVerifyError::new("generic slice site disagrees with its sealed operation"));
    }
    if !matches!(receiver, SchemeType::Parameter(_) | SchemeType::List(_) | SchemeType::Ground(Type::List(_) | Type::Str | Type::Bytes)) {
        return Err(IrVerifyError::new("generic slice site has no supported input domain"));
    }
    if receiver.closed_ground_type().is_some() && result.closed_ground_type().is_some() { selection(&operation, budget)?; }
    Ok(())
}
