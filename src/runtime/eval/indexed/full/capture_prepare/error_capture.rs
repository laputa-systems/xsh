use super::*;
use crate::sema::inference::{RequirementTemplate, ScopedRequirementRoot};

/// The boundary's output remains related to its original reached error ports
/// after the checker graph is released. Equal output types do not identify an
/// error boundary or authorize replacing one of its contributions.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedCaptureErrorRelation {
    origin: ExpressionIdentity,
    block: crate::syntax::arena::BlockId,
    source_type: ScopedRoot,
    requirement: ScopedRequirementRoot,
    owner: InstructionOwner,
    inputs: Box<[(ScopedRoot, Type)]>,
    result: ScopedRoot,
    output: Type,
    bound: Option<(ScopedRoot, Type)>,
}

impl PreparedCaptureErrorRelation {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>() + 2 * size_of::<usize>() + self.inputs.len() * size_of::<(ScopedRoot, Type)>()
            + self.inputs.iter().map(|(_, ty)| ty.retained_bytes().saturating_sub(size_of::<Type>())).sum::<usize>()
            + self.output.retained_bytes().saturating_sub(size_of::<Type>())
            + self.bound.as_ref().map_or(0, |(_, ty)| ty.retained_bytes().saturating_sub(size_of::<Type>()))
    }

    pub(in crate::runtime::eval) fn verify(&self, source: &TryCaptureSource) -> Result<(), IrVerifyError> {
        let Type::Result(_, error) = &source.original_carrier else { return Err(IrVerifyError::new("capture error boundary lacks its original Result")); };
        if self.origin != source.origin || self.block != source.block || self.source_type != source.source_type
            || self.owner != source.owner || self.inputs.is_empty() || **error != self.output
            || self.result.scope != self.source_type.scope || self.requirement.scope != self.source_type.scope
            || self.inputs.iter().any(|(root, _)| root.scope != self.source_type.scope)
            || self.bound.as_ref().is_some_and(|(root, _)| root.scope != self.source_type.scope) {
            return Err(IrVerifyError::new("capture changes its original error boundary relationship"));
        }
        Ok(())
    }
}

impl FullBuilder {
    pub(super) fn prepare_capture_error_relation(&self, original: &BuildTryCaptureOrigin,
        owner: InstructionOwner, solved: &crate::sema::check::SolvedTypes, carrier: &Type,
    ) -> Result<Option<Arc<PreparedCaptureErrorRelation>>, IrBuildError> {
        if original.retry.is_some() || original.propagation.is_some() { return Ok(None); }
        let Some(receipt) = &original.error_capture else { return Ok(None); };
        let authenticated = solved.error_capture_receipt(original.origin)
            .map_err(|_| problem("capture_original_error_boundary_changed"))?
            .ok_or_else(|| problem("capture_original_error_boundary_missing"))?;
        if !Arc::ptr_eq(receipt, &authenticated) || receipt.origin != original.origin || receipt.block != original.block
            || receipt.caller != solved.expression_owners.get(&original.origin).copied() {
            return Err(problem("capture_original_error_boundary_changed"));
        }
        let scope = solved.expression_scope(original.origin, receipt.caller).map_err(|_| problem("capture_original_error_boundary_scope"))?;
        if scope != original.source_type.scope { return Err(problem("capture_original_error_boundary_scope")); }
        let requirement = ScopedRequirementRoot { requirement: receipt.requirement, scope };
        solved.graph.validate_requirement_scoped(requirement).map_err(|_| problem("capture_original_error_join_not_published"))?;
        let RequirementTemplate::ErrorJoin { join } = solved.graph.requirement_template(receipt.requirement)
            .map_err(|_| problem("capture_original_error_join_changed"))? else { return Err(problem("capture_original_error_join_changed")); };
        let join = solved.graph.error_join(join).map_err(|_| problem("capture_original_error_join_changed"))?;
        if join.inputs.as_slice() != receipt.inputs.as_ref() || join.result != receipt.result || join.bound != receipt.bound || receipt.inputs.is_empty() {
            return Err(problem("capture_original_error_ports_changed"));
        }
        let root_type = |ty| {
            let root = ScopedRoot { ty, scope };
            solved.graph.validate_scoped(root).map_err(|_| problem("capture_original_error_port_scope"))?;
            let ty = graph_ground_type(&solved.graph, ty).map_err(|_| problem("capture_original_error_port_requires_ground"))?;
            Ok::<_, IrBuildError>((root, ty))
        };
        let inputs = receipt.inputs.iter().copied().map(root_type).collect::<Result<Vec<_>, _>>()?.into_boxed_slice();
        let (result, output) = root_type(receipt.result)?;
        let bound = receipt.bound.map(root_type).transpose()?;
        let Type::Result(_, error) = carrier else { return Err(problem("capture_original_error_result_changed")); };
        if **error != output { return Err(problem("capture_original_error_result_changed")); }
        Ok(Some(Arc::new(PreparedCaptureErrorRelation { origin: original.origin, block: original.block,
            source_type: original.source_type, requirement, owner, inputs, result, output, bound })))
    }
}

#[cfg(test)]
mod tests;
