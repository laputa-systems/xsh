use super::*;
use super::applications::ApplicationSubstitution;
use super::super::schemes::{FullOperationBinding, FullSealedOperation};
use crate::sema::sealed_operation::{SealedBinaryOperation, SealedOperation, SealedOperationSignature, arithmetic_operator, arithmetic_selection, index_selection, slice_selection};

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
        FullSealedOperation::Binary { left, right, result, operator } => SealedOperation::Binary(SealedBinaryOperation {
            operator, left: substitution.ty(store, scope, left, budget)?, right: substitution.ty(store, scope, right, budget)?, result: substitution.ty(store, scope, result, budget)?, span,
        }),
        FullSealedOperation::Index { receiver, key, result, syntax, constant_key } => SealedOperation::Index {
            receiver: substitution.ty(store, scope, receiver, budget)?, key: substitution.ty(store, scope, key, budget)?, result: substitution.ty(store, scope, result, budget)?, syntax,
            constant_key: if constant_key == IR_NONE { None } else { Some(Name::intern(store.string(constant_key)?)) }, span,
        },
        FullSealedOperation::Slice { receiver, result, bounds } => SealedOperation::Slice { receiver: substitution.ty(store, scope, receiver, budget)?, result: substitution.ty(store, scope, result, budget)?, bounds, span },
    })
}

fn selection(operation: &SealedOperation<SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<SealedOperationSignature, IrVerifyError> {
    let closed = operation.try_map(|ty| {
        budget.charge_scheme_clone(ty)?;
        ty.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected operation has no closed input tuple"))
    })?;
    let selected = match &closed {
        SealedOperation::Binary(operation) => arithmetic_selection(operation.operator, &operation.left, &operation.right).map(|(signature, result)| (SealedOperationSignature::Binary(signature), result)),
        SealedOperation::Index { receiver, key, constant_key, .. } => index_selection(receiver, key, *constant_key),
        SealedOperation::Slice { receiver, .. } => slice_selection(receiver),
    };
    let (signature, result) = selected.ok_or_else(|| IrVerifyError::new("sealed operation input tuple has no canonical signature"))?;
    budget.charge_clone(&result)?;
    if !body_type_matches(&SchemeType::Ground(result), operation.result(), budget)? { return Err(IrVerifyError::new("sealed operation result disagrees with its input signature")); }
    Ok(signature)
}

pub(super) fn symbolic_output_binders(
    store: &FullStore, scope: &SchemeScope, encoded: FullSealedOperation,
    substitution: &SymbolicCallSubstitution, budget: &mut SchemeInstantiationBudget,
) -> Result<BTreeSet<usize>, IrVerifyError> {
    let mut evidence = BTreeSet::new();
    if matches!(encoded, FullSealedOperation::Binary { .. }) { return Ok(evidence); }
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
        SealedOperation::Binary(_) => unreachable!(),
    };
    if let Some(projection) = projection {
        let parameter = decode_scheme_type(store, scope, encoded.result(), budget, 0)?;
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

pub(super) fn verify_bindings(store: &FullStore, callee: &FullScheme, scope: &SchemeScope, substitution: ApplicationSubstitution<'_>, caller: Option<(&FullScheme, &SchemeScope)>, bindings: &[FullOperationBinding], budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let operations = bounded(callee.operations, &store.scheme_operations, "scheme operation range is invalid")?;
    if operations.len() != bindings.len() { return Err(IrVerifyError::new("generic call operation binding is incomplete")); }
    for (encoded, binding) in operations.iter().zip(bindings) {
        let operation = decode(store, scope, *encoded, &substitution, budget)?;
        match *binding {
            FullOperationBinding::Selected(actual) => {
                if actual != selection(&operation, budget)? { return Err(IrVerifyError::new("selected operation differs from its canonical signature")); }
            }
            FullOperationBinding::Forwarded(ordinal) => {
                let Some((caller, caller_scope)) = caller else { return Err(IrVerifyError::new("ground operation has no forwarding owner")); };
                let encoded = bounded(caller.operations, &store.scheme_operations, "caller operation range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("forwarded operation ordinal is out of scope"))?;
                let required = decode(store, caller_scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
                if !operation.same_kind(&required) || !operation.types().zip(required.types()).all(|(actual, required)| body_types_equal(actual, required)) {
                    return Err(IrVerifyError::new("forwarded operation differs from its actual caller requirement"));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn verify_body_site(store: &FullStore, scope: &SchemeScope, ordinal: u32, operator: crate::syntax::node::BinaryOp, left: &SchemeType, right: &SchemeType, result: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let encoded = bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("body operation ordinal is out of scope"))?;
    let operation = decode(store, scope, *encoded, &ApplicationSubstitution::Identity, budget)?;
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
        SchemeType::Map(required, _) => body_type_matches(key, required, budget)?,
        SchemeType::Record { fields, .. } => constant_key.is_some_and(|key| fields.contains_key(&key)),
        SchemeType::Ground(receiver) => match receiver.unvalidated() {
            Type::List(_) => body_type_matches_ground(key, &Type::Int),
            Type::Map(required, _) => body_type_matches_ground(key, required),
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
