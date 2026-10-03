use super::{RuntimeError, Value};
use std::fmt;
use std::sync::Arc;

/// Immutable error metadata shares the complete typed cause rather than copying
/// its descendants whenever an error is propagated or given another context.
#[derive(Clone)]
pub struct ErrorCause {
    value: Option<Arc<Value>>,
}

impl ErrorCause {
    fn new(value: Value) -> Self {
        Self { value: Some(Arc::new(value)) }
    }

    pub fn as_value(&self) -> &Value {
        self.value.as_deref().expect("a live cause owns its value")
    }
}

impl fmt::Debug for ErrorCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ErrorCause")
            .field("kind", &self.as_value().error_kind())
            .field("message", &self.as_value().error_message()).finish_non_exhaustive()
    }
}

impl PartialEq for ErrorCause {
    fn eq(&self, other: &Self) -> bool {
        let mut left = Some(self.as_value());
        let mut right = Some(other.as_value());
        while let (Some(a), Some(b)) = (left, right) {
            if std::ptr::eq(a, b) { return true; }
            let mut a_outer = a.clone();
            let mut b_outer = b.clone();
            a_outer.take_error_cause();
            b_outer.take_error_cause();
            if a_outer != b_outer { return false; }
            left = a.error_cause().map(Self::as_value);
            right = b.error_cause().map(Self::as_value);
        }
        left.is_none() && right.is_none()
    }
}

impl Eq for ErrorCause {}

impl Drop for ErrorCause {
    fn drop(&mut self) {
        let mut next = self.value.take();
        // Consuming the final shared reference iteratively avoids recursive
        // destruction of a long, uniquely owned chain. Shared suffixes survive.
        while let Some(value) = next {
            let Some(mut value) = Arc::into_inner(value) else { break; };
            next = value.take_error_cause().and_then(|mut cause| cause.value.take());
        }
    }
}

impl Value {
    pub fn error_cause(&self) -> Option<&ErrorCause> {
        match self {
            Self::Error(error) => error.cause.as_ref(),
            Self::RunError(error) => error.cause.as_ref(),
            _ => None,
        }
    }

    fn take_error_cause(&mut self) -> Option<ErrorCause> {
        match self {
            Self::Error(error) => error.cause.take(),
            Self::RunError(error) => error.cause.take(),
            _ => None,
        }
    }

    pub(crate) fn with_error_cause(mut self, cause: Value) -> Result<Self, RuntimeError> {
        for value in [&self, &cause] {
            if let Self::Error(error) = value && error.abort.is_some() { return Err((**error).clone()); }
            if !matches!(value, Self::Error(_) | Self::RunError(_)) {
                return Err(RuntimeError::new("type-error", "Err with cause requires Error values"));
            }
        }
        let cause = Some(ErrorCause::new(cause));
        match &mut self {
            Self::Error(error) => error.cause = cause,
            Self::RunError(error) => error.cause = cause,
            _ => unreachable!("error values were checked before attaching metadata"),
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_cause_long_shared_chains_clone_compare_and_drop_without_recursion() {
        let mut chain = Value::Error(Box::new(RuntimeError::new("leaf", "original")));
        for _ in 0..100_000 {
            chain = Value::Error(Box::new(RuntimeError::new("outer", "translation")))
                .with_error_cause(chain).unwrap();
        }
        let alias = chain.clone();
        let independent_outer = Value::Error(Box::new(RuntimeError::new("outer", "translation")))
            .with_error_cause(chain.error_cause().unwrap().as_value().clone()).unwrap();
        assert_eq!(chain, independent_outer);
        drop(chain);
        assert_eq!(alias.error_cause().unwrap().as_value().error_kind(), Some("outer"));
        drop(independent_outer);
        drop(alias);
        let make_chain = || {
            let mut chain = Value::Error(Box::new(RuntimeError::new("leaf", "original")));
            for _ in 0..10_000 {
                chain = Value::Error(Box::new(RuntimeError::new("outer", "translation")))
                    .with_error_cause(chain).unwrap();
            }
            chain
        };
        assert_eq!(make_chain(), make_chain());
    }

    #[test]
    fn typed_cause_preserves_original_span_context_and_full_process_status() {
        use crate::runtime::process::ProcessStatus;
        use crate::runtime::value::{ErrorContext, RunError};
        use crate::source::{SourceId, Span};
        let span = Span::new(SourceId::new(7), 11, 17);
        let context = ErrorContext { kind: "ctx".into(), message: Some("original operation".into()), span: Some(span) };
        let status = ProcessStatus::exited(7);
        let inner = Value::RunError(Box::new(RunError::from_status(status.clone())
            .with_span(span).with_context(context.clone())));
        let outer = Value::Error(Box::new(RuntimeError::new("outer", "translation")));
        let attached = outer.clone().with_error_cause(inner).unwrap();
        assert!(outer.error_cause().is_none());
        let Value::RunError(inner) = attached.error_cause().unwrap().as_value() else { panic!("typed process cause") };
        assert_eq!(inner.span, Some(span));
        assert_eq!(inner.contexts, vec![context]);
        assert_eq!(inner.status.as_deref(), Some(&status));
    }
}
