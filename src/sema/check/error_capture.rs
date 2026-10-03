use super::*;
use crate::sema::inference::{InferenceContext, InferenceError, RequirementId, RequirementTemplate, ScopedRequirementRoot, ScopedRoot, TypeId, TypeNode};
use crate::syntax::arena::BlockId;

/// A captured failure retains the reached error ports and the exact join that
/// related them to the boundary's output. An output annotation cannot replace
/// those original contributions.
#[derive(Clone, Debug)]
pub(crate) struct SolvedErrorCapture {
    pub origin: ExpressionIdentity,
    pub block: BlockId,
    pub caller: Option<DeclarationIdentity>,
    pub requirement: RequirementId,
    pub inputs: Box<[TypeId]>,
    pub result: TypeId,
    pub bound: Option<TypeId>,
}

impl SolvedTypes<InferenceContext> {
    pub(super) fn record_error_capture(&mut self, origin: ExpressionIdentity, block: BlockId,
        caller: Option<DeclarationIdentity>, requirement: RequirementId,
        inputs: Vec<TypeId>, result: TypeId, bound: Option<TypeId>,
    ) -> Result<(), InferenceError> {
        let RequirementTemplate::ErrorJoin { join } = self.graph.requirement_template(requirement)? else {
            return Err(InferenceError::InvalidScheme);
        };
        let join = self.graph.error_join(join)?;
        if join.inputs != inputs || join.result != result || join.bound != bound || inputs.is_empty() {
            return Err(InferenceError::Boundary("capture changes its original error join"));
        }
        self.graph.charge_source_fact_nodes(1)?;
        self.graph.charge_source_fact_edges(inputs.len() as u64 + 5)?;
        self.graph.charge_source_fact_work(inputs.len() as u64 + 1)?;
        let receipt = Arc::new(SolvedErrorCapture { origin, block, caller, requirement,
            inputs: inputs.into_boxed_slice(), result, bound });
        self.error_captures.insert(origin, Arc::clone(&receipt));
        self.original_error_captures.insert(origin, receipt);
        Ok(())
    }
}

impl<Graph> SolvedTypes<Graph> {
    pub(crate) fn error_capture_receipt(&self, origin: ExpressionIdentity) -> Result<Option<Arc<SolvedErrorCapture>>, InferenceError> {
        self.error_capture(origin)?;
        Ok(self.error_captures.get(&origin).cloned())
    }

    pub(crate) fn error_capture(&self, origin: ExpressionIdentity) -> Result<Option<&SolvedErrorCapture>, InferenceError> {
        match (self.error_captures.get(&origin), self.original_error_captures.get(&origin)) {
            (None, None) => Ok(None),
            (Some(receipt), Some(original)) if Arc::ptr_eq(receipt, original) => Ok(Some(receipt)),
            _ => Err(InferenceError::Boundary("capture differs from its original error boundary")),
        }
    }

    pub(super) fn validate_error_captures(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        if self.error_captures.len() != self.original_error_captures.len() {
            return Err(InferenceError::Boundary("capture error boundary ledger is incomplete"));
        }
        for (&origin, receipt) in &self.error_captures {
            self.error_capture(origin)?;
            if receipt.origin != origin || receipt.inputs.is_empty()
                || self.expression_owners.get(&origin).copied() != receipt.caller {
                return Err(InferenceError::Boundary("capture changes its original error owner"));
            }
            let RequirementTemplate::ErrorJoin { join } = graph.requirement_template(receipt.requirement)? else { return Err(InferenceError::InvalidScheme); };
            let join = graph.error_join(join)?;
            if join.inputs.as_slice() != receipt.inputs.as_ref() || join.result != receipt.result || join.bound != receipt.bound {
                return Err(InferenceError::Boundary("capture changes its original reached error ports"));
            }
            let carrier = *self.expressions.get(&origin).ok_or(InferenceError::InvalidScheme)?;
            let TypeNode::Result(_, error) = graph.node(graph.resolved(carrier)?)? else { return Err(InferenceError::InvalidScheme); };
            // Reimporting a concrete composite payload can allocate another
            // node for the same exact type. Its original join ports remain
            // authenticated independently of the output node's representation.
            let same_error = graph.resolved(*error)? == graph.resolved(receipt.result)?
                || matches!((graph.export_type(*error), graph.export_type(receipt.result)), (Ok(actual), Ok(joined)) if actual == joined);
            if !same_error {
                return Err(InferenceError::Boundary("capture output changes its original error join"));
            }
        }
        Ok(())
    }

    pub(super) fn error_capture_roots(&self, graph: &InferenceContext) -> Result<Vec<ScopedRoot>, InferenceError> {
        self.validate_error_captures(graph)?;
        let mut roots = Vec::new();
        for (&origin, receipt) in &self.error_captures {
            let scope = self.expression_scope(origin, receipt.caller)?;
            roots.extend(receipt.inputs.iter().copied().chain(std::iter::once(receipt.result)).chain(receipt.bound)
                .map(|ty| ScopedRoot { ty, scope }));
        }
        Ok(roots)
    }

    pub(super) fn error_capture_requirements(&self) -> Result<Vec<ScopedRequirementRoot>, InferenceError> {
        self.error_captures.iter().map(|(&origin, receipt)| {
            self.error_capture(origin)?;
            Ok(ScopedRequirementRoot { requirement: receipt.requirement, scope: self.expression_scope(origin, receipt.caller)? })
        }).collect()
    }

    pub(super) fn error_capture_source_edges(&self) -> u64 {
        self.error_captures.values().map(|receipt| receipt.inputs.len() as u64 + 5).sum()
    }

    pub(super) fn error_capture_retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.error_captures.values().map(|receipt| size_of::<SolvedErrorCapture>()
            + 2 * size_of::<usize>() + receipt.inputs.len() * size_of::<TypeId>()).sum()
    }
}
