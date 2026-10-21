use super::effects::EffectContract;
use super::types::CallableKind;
use crate::source::Span;
use super::arguments::StaticArgumentKind;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallableKindRequirement {
    Pure,
    Immediate,
}

impl CallableKindRequirement {
    pub(crate) fn accepts(self, kind: CallableKind) -> bool {
        matches!((self, kind), (_, CallableKind::Pure) | (Self::Immediate, CallableKind::Proc))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableApplicationArgument<T> {
    pub(crate) kind: StaticArgumentKind,
    pub(crate) ty: T,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableApplication<T, V> {
    pub(crate) callee: T,
    pub(crate) arguments: Arc<[CallableApplicationArgument<T>]>,
    pub(crate) result: T,
    pub(crate) effects: EffectContract<V>,
    pub(crate) kind: CallableKindRequirement,
    pub(crate) span: Span,
}
