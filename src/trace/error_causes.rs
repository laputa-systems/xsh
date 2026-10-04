use super::{TraceError, TraceStatus, TraceStatusKind};
use crate::runtime::process::ProcessStatusKind;
use crate::runtime::value::{ErrorContext, RunError, RuntimeError, Value};
use crate::source::Span;

pub const ERROR_CAUSE_DEPTH_LIMIT: usize = 32;
const ERROR_TEXT_LIMIT: usize = 4096;

/// A bounded diagnostic snapshot retains nominal identity independently of the
/// error's customizable kind and message. Causes form a flat ordered sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceErrorDetail {
    pub family: String,
    pub variant: String,
    pub kind: String,
    pub message: String,
    pub facets: Vec<String>,
    pub span: Option<Span>,
    pub contexts: Vec<ErrorContext>,
    pub status: Option<TraceStatus>,
}

fn bounded_text(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut output: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        output.push('…');
    }
    output
}

impl TraceErrorDetail {
    fn from_error(error: &RuntimeError) -> Self {
        let status = error
            .propagated_run_error
            .as_ref()
            .and_then(|original| Self::from_run_error(original).status);
        Self::snapshot(
            &error.family,
            &error.variant,
            &error.kind,
            &error.message,
            &error.facets,
            error.span,
            &error.contexts,
            status,
        )
    }

    fn from_run_error(error: &RunError) -> Self {
        let status = error.status.as_ref().map(|status| TraceStatus {
            success: status.success,
            kind: match status.kind {
                ProcessStatusKind::Exit => TraceStatusKind::Exit,
                ProcessStatusKind::Signal => TraceStatusKind::Signal,
                ProcessStatusKind::Exec => TraceStatusKind::Exec,
            },
            code: status.code,
        });
        Self::snapshot(
            "ProcessError",
            error.variant_name(),
            &error.kind,
            &error.message,
            &error.facets(),
            error.span,
            &error.contexts,
            status,
        )
    }

    fn snapshot(
        family: &str,
        variant: &str,
        kind: &str,
        message: &str,
        facets: &[String],
        span: Option<Span>,
        contexts: &[ErrorContext],
        status: Option<TraceStatus>,
    ) -> Self {
        Self {
            family: bounded_text(family, 256),
            variant: bounded_text(variant, 256),
            kind: bounded_text(kind, 256),
            message: bounded_text(message, ERROR_TEXT_LIMIT),
            facets: facets
                .iter()
                .take(16)
                .map(|facet| bounded_text(facet, 256))
                .collect(),
            span,
            contexts: contexts
                .iter()
                .take(16)
                .map(|context| {
                    let mut context = context.clone();
                    context.kind = bounded_text(&context.kind, 256);
                    context.message = context
                        .message
                        .as_deref()
                        .map(|message| bounded_text(message, 1024));
                    context
                })
                .collect(),
            status,
        }
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Error(error) => Some(Self::from_error(error)),
            Value::RunError(error) => Some(Self::from_run_error(error)),
            _ => None,
        }
    }
}

impl TraceError {
    pub fn caused_from_value(value: &Value) -> Option<Self> {
        value.error_cause().map(|_| Self::from_value(value))
    }

    pub(crate) fn from_propagated_value(value: &Value) -> Self {
        if value.error_kind().is_none() {
            Self::new("error", "propagated error")
        } else {
            Self::from_value(value)
        }
    }

    pub fn from_run_error(error: &RunError) -> Self {
        if error.cause.is_none() {
            return Self::new(&error.kind, &error.message);
        }
        Self::from_value(&Value::RunError(Box::new(error.clone())))
    }

    pub fn from_value(value: &Value) -> Self {
        let mut trace = Self::new(
            value.error_kind().unwrap_or("runtime-error"),
            value.error_message().unwrap_or("runtime error"),
        );
        let mut cause = value.error_cause();
        if cause.is_none() {
            return trace;
        }
        trace.detail = TraceErrorDetail::from_value(value);
        trace.kind = bounded_text(&trace.kind, 256);
        trace.message = bounded_text(&trace.message, ERROR_TEXT_LIMIT);
        while let Some(current) = cause {
            if trace.causes.len() == ERROR_CAUSE_DEPTH_LIMIT {
                trace.causes_truncated = true;
                break;
            }
            let value = current.as_value();
            if let Some(detail) = TraceErrorDetail::from_value(value) {
                trace.causes.push(detail);
            }
            cause = value.error_cause();
        }
        trace
    }

    pub fn from_runtime_error(error: &RuntimeError) -> Self {
        if error.cause.is_none() {
            return Self::new(&error.kind, &error.message);
        }
        Self::from_value(&Value::Error(Box::new(error.clone())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceMap;
    use crate::trace::{Traceback, TracebackRenderer};

    #[test]
    fn typed_cause_long_chain_rendering_is_bounded_and_escaped() {
        let mut chain = Value::Error(Box::new(RuntimeError::new("leaf", "\x1b[31m\nforged")));
        for _ in 0..10_000 {
            chain = Value::Error(Box::new(RuntimeError::new("outer", "x".repeat(10_000))))
                .with_error_cause(chain)
                .unwrap();
        }
        let trace = TraceError::from_value(&chain);
        assert_eq!(trace.causes.len(), ERROR_CAUSE_DEPTH_LIMIT);
        assert!(trace.causes_truncated);
        assert!(trace.causes.iter().all(|cause| cause.message.len() < 4100));
        let traceback = Traceback {
            failing_span: None,
            exe_path: String::new(),
            operation_kind: "result.propagate".into(),
            error: trace,
            frames: Vec::new(),
        };
        let output = TracebackRenderer::new().render(&traceback, &SourceMap::new());
        assert!(output.len() < 150_000);
        assert!(output.contains("cause chain truncated"));
        let short = Value::Error(Box::new(RuntimeError::new("outer", "outer")))
            .with_error_cause(Value::Error(Box::new(RuntimeError::new(
                "leaf",
                "\x1b[31m\nforged",
            ))))
            .unwrap();
        let traceback = Traceback {
            error: TraceError::from_value(&short),
            ..traceback
        };
        let output = TracebackRenderer::new().render(&traceback, &SourceMap::new());
        assert!(!output.contains('\x1b'));
        assert!(!output.contains("\nforged"));
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("\x1b[31m\nforged-file", "x");
        let inner = Value::Error(Box::new(
            RuntimeError::new("leaf", "original").with_span(Span::at(source_id, 0)),
        ));
        let error = Value::Error(Box::new(RuntimeError::new("outer", "translation")))
            .with_error_cause(inner)
            .unwrap();
        let traceback = Traceback {
            error: TraceError::from_value(&error),
            ..traceback
        };
        let output = TracebackRenderer::new().render(&traceback, &sources);
        assert!(!output.contains('\x1b'));
        assert!(!output.contains("\nforged-file"));
    }
}
