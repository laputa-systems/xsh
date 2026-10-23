use super::effects::EffectContract;
use super::types::{CallableKind, Type};
use crate::source::Span;
use super::arguments::StaticArgumentKind;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallableKindRequirement {
    Pure,
    Effectful,
}

impl CallableKindRequirement {
    pub(crate) fn accepts(self, kind: CallableKind) -> bool {
        matches!((self, kind), (_, CallableKind::Pure) | (Self::Effectful, CallableKind::Proc | CallableKind::Stream))
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
        Type::Unit if !propagated.is_empty() => Type::Result(
            Box::new(Type::Unit),
            Box::new(join_propagated_error_types(propagated)),
        ),
        value => Type::Result(Box::new(value), Box::new(Type::Error)),
    }
}

fn join_propagated_error_types(propagated: &[Type]) -> Type {
    let (first, rest) = propagated.split_first().expect("propagated errors are nonempty");
    rest.iter()
        .fold(first.clone(), |joined, error| join_error_types(&joined, error))
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
