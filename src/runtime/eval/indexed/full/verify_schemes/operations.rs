use super::*;
use super::applications::ApplicationSubstitution;
use super::super::schemes::{FullOperationBinding, FullSealedOperation};
use crate::sema::sealed_operation::{SealedBinaryOperation, arithmetic_operator, arithmetic_selection};

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

pub(super) fn decode(store: &FullStore, scope: &SchemeScope, encoded: FullSealedOperation, substitution: &ApplicationSubstitution<'_>, budget: &mut SchemeInstantiationBudget) -> Result<SealedBinaryOperation<SchemeType>, IrVerifyError> {
    budget.charge(1)?;
    if encoded.reserved != [0; 3] { return Err(IrVerifyError::new("sealed operation metadata is invalid")); }
    Ok(SealedBinaryOperation {
        operator: encoded.operator,
        left: substitution.ty(store, scope, encoded.left, budget)?,
        right: substitution.ty(store, scope, encoded.right, budget)?,
        result: substitution.ty(store, scope, encoded.result, budget)?,
        span: crate::source::Span::at(store.source_id, 0),
    })
}

fn selection(operation: &SealedBinaryOperation<SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<crate::sema::sealed_operation::ArithmeticSignature, IrVerifyError> {
    let left = operation.left.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected operation has no closed left input"))?;
    let right = operation.right.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected operation has no closed right input"))?;
    budget.charge_clone(&left)?;
    budget.charge_clone(&right)?;
    let (signature, result) = arithmetic_selection(operation.operator, &left, &right).ok_or_else(|| IrVerifyError::new("sealed operation input tuple has no canonical signature"))?;
    budget.charge_clone(&result)?;
    if !body_type_matches(&SchemeType::Ground(result), &operation.result, budget)? { return Err(IrVerifyError::new("sealed operation result disagrees with its input signature")); }
    Ok(signature)
}

pub(super) fn verify_ground(store: &FullStore, scope: &SchemeScope, substitution: &CallSubstitution, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    for encoded in bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")? {
        budget.charge(1)?;
        let operation = SealedBinaryOperation {
            operator: encoded.operator,
            left: SchemeType::Ground(instantiate_bound_type(store, encoded.left, substitution, budget, 0)?),
            right: SchemeType::Ground(instantiate_bound_type(store, encoded.right, substitution, budget, 0)?),
            result: SchemeType::Ground(instantiate_bound_type(store, encoded.result, substitution, budget, 0)?),
            span: crate::source::Span::at(store.source_id, 0),
        };
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
                if operation.operator != required.operator || !body_types_equal(&operation.left, &required.left) || !body_types_equal(&operation.right, &required.right) || !body_types_equal(&operation.result, &required.result) {
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
    if arithmetic_operator(operator) != Some(operation.operator) || !body_types_equal(left, &operation.left) || !body_types_equal(right, &operation.right) || !body_types_equal(result, &operation.result) {
        return Err(IrVerifyError::new("generic binary site disagrees with its sealed operation"));
    }
    if !crate::sema::sealed_operation::ArithmeticSignature::ALL.into_iter().any(|signature| signature.accepts_operator(operation.operator) && {
        let (left, right) = signature.domains();
        domain_accepts(left, &operation.left) && domain_accepts(right, &operation.right)
    }) { return Err(IrVerifyError::new("generic binary site has no canonical input domain")); }
    if operation.left.closed_ground_type().is_some() && operation.right.closed_ground_type().is_some() { selection(&operation, budget)?; }
    Ok(())
}
