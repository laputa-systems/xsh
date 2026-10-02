use super::*;
use super::super::generic::{OperationSourceOrigin, PreparedOperationAuthority};
use crate::sema::inference::Atom;
use crate::sema::operation_graph::{ArithmeticDomain, PreparedLanguageOperation};

impl FullVerifier {
    pub(super) fn verify_language_result_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) { return Ok(false); }
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        let PreparedOperationAuthority::Language { operation: language, .. } = operation.authority else { return Ok(false); };
        let (op, domain, result) = match language {
            PreparedLanguageOperation::Arithmetic { op, domain: ArithmeticDomain::Float } => (op, Type::Float, Type::Float),
            PreparedLanguageOperation::Arithmetic { op, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } } => (op, Type::Int, Type::Int),
            PreparedLanguageOperation::Ordering { op, left: Atom::Str, right: Atom::Str }
            | PreparedLanguageOperation::Equality { op } => (op, Type::Str, Type::Bool),
            PreparedLanguageOperation::Ordering { op, left: Atom::Int, right: Atom::Int } => (op, Type::Int, Type::Bool),
            _ => return Ok(false),
        };
        let source = generic.operation_source(operation.source)?;
        let OperationSourceOrigin::Expression(expression) = source.origin else { return Err(IrVerifyError::new("language result loses its original expression")); };
        if source.instruction != instruction || source.owner != owner || source.expected != operation.authority
            || source.identity != operation.authority.identity()
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner)) {
            return Err(IrVerifyError::new("language result changes its original operation or owner"));
        }
        let TypeRef::Ground(result_type) = operation.result else { return Err(IrVerifyError::new("language result lacks its checked ground type")); };
        if result != *expected || store.semantic.to_type(result_type)? != result
            || operation.arguments.len() != 2 || operation.binding.operands.len() != 2 || operation.receiver.is_some() {
            return Err(IrVerifyError::new("language result changes its checked result or argument relationship"));
        }
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) { return Err(IrVerifyError::new("language result changes its original instruction kind")); }
        let words = store.payload(store.data[instruction as usize].range())?;
        if words.len() != 4 || words.first().and_then(|&index| store.binary_ops.get(index as usize)) != Some(&op)
            || words.get(1..3) != Some(operation.binding.operands.as_ref())
            || IrLocationId::from_raw(words[3]).and_then(|location| store.location_sources.get(location.index())) != Some(&expression.source) {
            return Err(IrVerifyError::new("language result changes its original operator, operands, or source"));
        }
        for (&operand, ty) in operation.binding.operands.iter().zip(operation.arguments.iter()) {
            let Some(TypeRef::Ground(ty)) = ty else { return Err(IrVerifyError::new("language result operand lacks its checked ground type")); };
            if store.semantic.to_type(*ty)? != domain { return Err(IrVerifyError::new("language result changes its checked operand domain")); }
            Self::verify_generic_source(store, generic, operand, owner, &domain, instance, active)?;
        }
        Ok(true)
    }
}
