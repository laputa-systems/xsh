//! Process-handle bookkeeping shared by the lowered runtime: registering a
//! spawned child as a `ProcessHandleValue`, tracing spawn/wait/cancel events,
//! and waiting on one or many handles. These helpers were factored out of the
//! (now-deleted) recursive expression evaluator because the lowered `spawn`
//! support in `lowered_run.rs` still needs them.

use super::{Evaluator, LiveProcessHandle, trace_env_overlay, trace_status};
use crate::runtime::process::{
    ChildWaitOutcome, ManagedChild, ProcessInvocation, WaitMode, path_bytes, wait_managed,
};
use crate::runtime::value::{ProcessHandleValue, RunError, RuntimeError, Value};
use crate::source::Span;
use crate::trace::{TraceArg, TraceError, TraceKind, TracePayload, TraceStatus};
use rustc_hash::FxHashSet;
use std::sync::Arc;
use std::time::Duration;

impl Evaluator {
    pub(super) fn process_handle_value(
        &mut self,
        child: ManagedChild,
        span: Span,
    ) -> ProcessHandleValue {
        let id = self.next_process_handle_id;
        self.next_process_handle_id += 1;
        let command: Arc<str> = String::from_utf8_lossy(&child.target).into_owned().into();
        let argv = std::iter::once(child.target.as_slice())
            .chain(child.argv.iter().map(Vec::as_slice))
            .map(|arg| Arc::<str>::from(String::from_utf8_lossy(arg).into_owned()))
            .collect::<Vec<_>>();
        let handle = ProcessHandleValue {
            id,
            pid: i64::from(child.pid),
            command,
            argv: Arc::from(argv.into_boxed_slice()),
            detached: child.detached,
        };
        self.process_handles.insert(
            id,
            LiveProcessHandle {
                owner_scope: self.current_scope_id(),
                child,
                span,
            },
        );
        handle
    }

    pub(super) fn trace_spawn_start(
        &mut self,
        span: Span,
        invocation: &ProcessInvocation,
        detached: bool,
    ) {
        let name = String::from_utf8_lossy(&invocation.target);
        self.trace_leaf(
            TraceKind::SpawnStart,
            Some(span),
            Some(name.as_ref()),
            TracePayload::SpawnStart {
                handle_id: None,
                target: TraceArg::bytes(invocation.target.clone()),
                argv: invocation
                    .argv
                    .iter()
                    .cloned()
                    .map(TraceArg::bytes)
                    .collect(),
                cwd: TraceArg::bytes(path_bytes(&invocation.cwd)),
                env: trace_env_overlay(&invocation.env_overlay),
                detached,
            },
        );
    }

    fn trace_wait_start(&mut self, span: Span, handle_ids: Vec<u64>) {
        self.trace_leaf(
            TraceKind::WaitStart,
            Some(span),
            None,
            TracePayload::WaitStart { handle_ids },
        );
    }

    fn trace_wait_end(
        &mut self,
        span: Span,
        handle_id: Option<u64>,
        pid: Option<u32>,
        status: Option<TraceStatus>,
        error: Option<&RunError>,
    ) {
        self.trace_leaf(
            TraceKind::WaitEnd,
            Some(span),
            None,
            TracePayload::WaitEnd {
                handle_id,
                pid,
                status,
                error: error.map(|error| TraceError::new(&error.kind, &error.message)),
            },
        );
    }

    pub(super) fn trace_spawn_cancel(
        &mut self,
        span: Span,
        handle_id: u64,
        pid: Option<u32>,
        signal: &str,
        kill_after: Duration,
        error: Option<&RunError>,
    ) {
        self.trace_leaf(
            TraceKind::SpawnCancel,
            Some(span),
            None,
            TracePayload::SpawnCancel {
                handle_id,
                pid,
                signal: signal.to_string(),
                kill_after_ms: kill_after.as_millis().try_into().unwrap_or(u64::MAX),
                error: error.map(|error| TraceError::new(&error.kind, &error.message)),
            },
        );
    }

    pub(super) fn wait_one_process_handle(
        &mut self,
        handle: ProcessHandleValue,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        self.trace_wait_start(span, vec![handle.id]);
        let Some(mut live) = self.process_handles.remove(&handle.id) else {
            let error = invalid_process_handle_error(handle.id, span);
            self.trace_wait_end(span, Some(handle.id), None, None, Some(&error));
            return Ok(process_handle_error(error));
        };
        let pid = Some(live.child.pid);
        match wait_managed(&mut live.child, WaitMode::Script, self) {
            Ok((ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status), None)) => {
                self.trace_wait_end(
                    span,
                    Some(handle.id),
                    pid,
                    Some(trace_status(&status)),
                    None,
                );
                self.last_status = Some(status.clone());
                Ok(Value::ok(Value::Status(status)))
            }
            Ok((
                ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status),
                Some(cancellation),
            )) => {
                let traced_status = trace_status(&status);
                let error = cancellation.error(Some(status)).with_span(span);
                self.trace_wait_end(
                    span,
                    Some(handle.id),
                    pid,
                    Some(traced_status),
                    Some(&error),
                );
                Ok(process_handle_error(error))
            }
            Ok((ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning, _)) => {
                let error = RunError::new("wait", "process did not exit").with_span(span);
                self.trace_wait_end(span, Some(handle.id), pid, None, Some(&error));
                Ok(process_handle_error(error))
            }
            Err(error) => {
                let error = error.with_span(span);
                self.trace_wait_end(span, Some(handle.id), pid, None, Some(&error));
                Ok(process_handle_error(error))
            }
        }
    }

    pub(super) fn wait_process_handle_list(
        &mut self,
        items: Vec<Value>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let mut seen = FxHashSet::default();
        let mut statuses = Vec::new();
        let mut first_error = None;
        let handle_ids = items
            .iter()
            .filter_map(|item| match item {
                Value::ProcessHandle(handle) => Some(handle.id),
                _ => None,
            })
            .collect();
        self.trace_wait_start(span, handle_ids);
        for item in items {
            let Value::ProcessHandle(handle) = item else {
                let error = RunError::new("unknown", "wait list items must be ProcessHandle")
                    .with_span(span);
                self.trace_wait_end(span, None, None, None, Some(&error));
                if first_error.is_none() {
                    first_error = Some(error);
                }
                continue;
            };
            if !seen.insert(handle.id) {
                let error = RunError::new("unknown", "process handle was already requested")
                    .with_span(span);
                self.trace_wait_end(span, Some(handle.id), None, None, Some(&error));
                if first_error.is_none() {
                    first_error = Some(error);
                }
                continue;
            }
            let Some(mut live) = self.process_handles.remove(&handle.id) else {
                let error =
                    RunError::new("unknown", "process handle is no longer live").with_span(span);
                self.trace_wait_end(span, Some(handle.id), None, None, Some(&error));
                if first_error.is_none() {
                    first_error = Some(error);
                }
                continue;
            };
            let pid = Some(live.child.pid);
            match wait_managed(&mut live.child, WaitMode::Script, self) {
                Ok((
                    ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status),
                    None,
                )) => {
                    self.trace_wait_end(
                        span,
                        Some(handle.id),
                        pid,
                        Some(trace_status(&status)),
                        None,
                    );
                    statuses.push(Value::Status(status));
                }
                Ok((
                    ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status),
                    Some(cancellation),
                )) => {
                    let traced_status = trace_status(&status);
                    let error = cancellation.error(Some(status)).with_span(span);
                    self.trace_wait_end(
                        span,
                        Some(handle.id),
                        pid,
                        Some(traced_status),
                        Some(&error),
                    );
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Ok((ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning, _)) => {
                    let error = RunError::new("wait", "process did not exit").with_span(span);
                    self.trace_wait_end(span, Some(handle.id), pid, None, Some(&error));
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(error) => {
                    let error = error.with_span(span);
                    self.trace_wait_end(span, Some(handle.id), pid, None, Some(&error));
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if let Some(error) = first_error {
            Ok(process_handle_error(error))
        } else {
            if let Some(Value::Status(status)) = statuses.last() {
                self.last_status = Some(status.clone());
            }
            Ok(Value::ok(Value::List(statuses)))
        }
    }
}

pub(super) fn process_handle_error(error: RunError) -> Value {
    Value::err(Value::RunError(Box::new(error)))
}

pub(super) fn invalid_process_handle_error(id: u64, span: Span) -> RunError {
    RunError::new("unknown", format!("process handle {id} is no longer live")).with_span(span)
}
