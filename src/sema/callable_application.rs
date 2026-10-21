use super::effects::EffectContract;
use super::types::{CallableKind, Type};
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
pub(crate) struct ProcReturnEnvelope<T> {
    pub(crate) result: T,
    pub(crate) propagated_errors: Arc<[T]>,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableApplication<T, V> {
    pub(crate) callee: T,
    pub(crate) arguments: Arc<[CallableApplicationArgument<T>]>,
    pub(crate) result: T,
    pub(crate) effects: EffectContract<V>,
    pub(crate) proc_returns: Arc<[ProcReturnEnvelope<T>]>,
    pub(crate) kind: CallableKindRequirement,
    pub(crate) span: Span,
}

pub(crate) fn inferred_proc_return_envelope(value: Type, propagated: &[Type]) -> Type {
    match value {
        Type::Result(ok, error) => Type::Result(ok, Box::new(propagated.iter().fold(*error, |joined, error| join_error_types(&joined, error)))),
        value => Type::Result(Box::new(value), Box::new(Type::Error)),
    }
}

pub(crate) fn join_error_types(left: &Type, right: &Type) -> Type {
    match (left, right) {
        _ if left == right => left.clone(),
        (Type::Unknown, other) | (other, Type::Unknown) => other.clone(),
        (Type::ErrorVariant { family, .. }, Type::ErrorVariant { family: other, .. })
        | (Type::ErrorVariant { family, .. }, Type::ErrorFamily(other))
        | (Type::ErrorFamily(family), Type::ErrorVariant { family: other, .. })
            if family == other => Type::ErrorFamily(*family),
        _ => Type::Error,
    }
}
