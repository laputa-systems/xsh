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
pub(crate) struct CallableInvocation<T, V> {
    pub(crate) callee: T,
    pub(crate) arguments: Arc<[CallableApplicationArgument<T>]>,
    pub(crate) result: T,
    pub(crate) effects: EffectContract<V>,
    pub(crate) proc_returns: Arc<[ProcReturnEnvelope<T>]>,
    pub(crate) kind: CallableKindRequirement,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KnownArgumentParameter<T> {
    pub(crate) layout: super::arguments::ArgumentParameterLayout,
    pub(crate) ty: T,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KnownCallArguments<T> {
    pub(crate) parameters: Arc<[KnownArgumentParameter<T>]>,
    pub(crate) arguments: Arc<[CallableApplicationArgument<T>]>,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CallableApplication<T, V> {
    Invocation(CallableInvocation<T, V>),
    KnownArguments(KnownCallArguments<T>),
}

impl<T, V> CallableApplication<T, V> {
    pub(crate) fn invocation(&self) -> Option<&CallableInvocation<T, V>> {
        match self { Self::Invocation(value) => Some(value), Self::KnownArguments(_) => None }
    }

    pub(crate) fn invocation_mut(&mut self) -> Option<&mut CallableInvocation<T, V>> {
        match self { Self::Invocation(value) => Some(value), Self::KnownArguments(_) => None }
    }

    pub(crate) fn arguments(&self) -> &[CallableApplicationArgument<T>] {
        match self { Self::Invocation(value) => &value.arguments, Self::KnownArguments(value) => &value.arguments }
    }

    pub(crate) fn types(&self) -> impl Iterator<Item = &T> {
        let invocation = self.invocation();
        invocation.map(|value| &value.callee).into_iter()
            .chain(invocation.map(|value| &value.result))
            .chain(self.arguments().iter().map(|value| &value.ty))
            .chain(match self { Self::KnownArguments(value) => Some(value), Self::Invocation(_) => None }
                .into_iter().flat_map(|value| value.parameters.iter().map(|parameter| &parameter.ty)))
            .chain(invocation.into_iter().flat_map(|value| value.proc_returns.iter())
                .flat_map(|value| std::iter::once(&value.result).chain(value.propagated_errors.iter())))
    }

    pub(crate) fn try_map_types<U, E>(&self, mut map: impl FnMut(&T) -> Result<U, E>) -> Result<CallableApplication<U, V>, E>
    where V: Clone {
        Ok(match self {
            Self::Invocation(value) => {
                let callee = map(&value.callee)?;
                let arguments = value.arguments.iter().map(|argument| Ok(CallableApplicationArgument {
                    kind: argument.kind, ty: map(&argument.ty)?, span: argument.span,
                })).collect::<Result<Vec<_>, E>>()?.into();
                let result = map(&value.result)?;
                let proc_returns = value.proc_returns.iter().map(|envelope| Ok(ProcReturnEnvelope {
                    result: map(&envelope.result)?,
                    propagated_errors: envelope.propagated_errors.iter().map(&mut map).collect::<Result<Vec<_>, E>>()?.into(),
                    span: envelope.span,
                })).collect::<Result<Vec<_>, E>>()?.into();
                CallableApplication::Invocation(CallableInvocation {
                    callee, arguments, result, effects: value.effects.clone(), proc_returns, kind: value.kind, span: value.span,
                })
            }
            Self::KnownArguments(value) => {
                let parameters = value.parameters.iter().map(|parameter| Ok(KnownArgumentParameter {
                    layout: parameter.layout, ty: map(&parameter.ty)?,
                })).collect::<Result<Vec<_>, E>>()?.into();
                let arguments = value.arguments.iter().map(|argument| Ok(CallableApplicationArgument {
                    kind: argument.kind, ty: map(&argument.ty)?, span: argument.span,
                })).collect::<Result<Vec<_>, E>>()?.into();
                CallableApplication::KnownArguments(KnownCallArguments { parameters, arguments, span: value.span })
            }
        })
    }

    pub(crate) fn try_map_effects<W, E>(self, map: impl FnOnce(EffectContract<V>) -> Result<EffectContract<W>, E>) -> Result<CallableApplication<T, W>, E> {
        Ok(match self {
            Self::Invocation(value) => CallableApplication::Invocation(CallableInvocation {
                callee: value.callee, arguments: value.arguments, result: value.result,
                effects: map(value.effects)?, proc_returns: value.proc_returns, kind: value.kind, span: value.span,
            }),
            Self::KnownArguments(value) => CallableApplication::KnownArguments(value),
        })
    }

    pub(crate) fn for_each_type_mut(&mut self, mut visit: impl FnMut(&mut T))
    where T: Clone {
        match self {
            Self::Invocation(value) => {
                visit(&mut value.callee);
                visit(&mut value.result);
                for argument in Arc::make_mut(&mut value.arguments) { visit(&mut argument.ty); }
                for envelope in Arc::make_mut(&mut value.proc_returns) {
                    visit(&mut envelope.result);
                    for error in Arc::make_mut(&mut envelope.propagated_errors) { visit(error); }
                }
            }
            Self::KnownArguments(value) => {
                for parameter in Arc::make_mut(&mut value.parameters) { visit(&mut parameter.ty); }
                for argument in Arc::make_mut(&mut value.arguments) { visit(&mut argument.ty); }
            }
        }
    }
}

impl<T, V> From<CallableInvocation<T, V>> for CallableApplication<T, V> {
    fn from(value: CallableInvocation<T, V>) -> Self { Self::Invocation(value) }
}

impl<T, V> From<KnownCallArguments<T>> for CallableApplication<T, V> {
    fn from(value: KnownCallArguments<T>) -> Self { Self::KnownArguments(value) }
}

pub(crate) fn inferred_proc_return_envelope(value: Type, propagated: &[Type]) -> Type {
    match value {
        Type::Result(ok, error) => Type::Result(ok, Box::new(propagated.iter().fold(*error, |joined, error| join_error_types(&joined, error)))),
        Type::Unit if !propagated.is_empty() => Type::Result(
            Box::new(Type::Unit),
            Box::new(join_propagated_error_types(propagated)),
        ),
        value => Type::Result(
            Box::new(value),
            Box::new(if propagated.is_empty() {
                Type::Error
            } else {
                join_propagated_error_types(propagated)
            }),
        ),
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
