use super::RuntimeOp;
use crate::modules::cli::CliDescriptorPlan;
use std::sync::Arc;

/// Prepared interpretation accompanies the same slots used by ordinary calls.
#[derive(Clone, Debug)]
pub(super) enum ModuleCallPlan {
    SignatureCli(Arc<CliDescriptorPlan>),
}

impl ModuleCallPlan {
    pub(super) fn same_identity(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SignatureCli(left), Self::SignatureCli(right)) => Arc::ptr_eq(left, right),
        }
    }

    pub(super) fn matches_operation(&self, op: RuntimeOp) -> bool {
        match self {
            Self::SignatureCli(plan) => plan.matches_operation(op),
        }
    }

    pub(super) fn cli_descriptor(&self) -> Option<&CliDescriptorPlan> {
        match self {
            Self::SignatureCli(plan) => Some(plan),
        }
    }
}
