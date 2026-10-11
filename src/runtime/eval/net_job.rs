//! Evaluator-side ownership for opaque `NetJob` values.
//!
//! Transport state stays in `xsh_net`; this module retains only evaluator
//! ownership, completion consumption, and lexical-cleanup information.

use super::{Evaluator, Flow};
#[cfg(feature = "net")]
use crate::modules::net;
use crate::modules::net::NetOperation;
#[cfg(feature = "net")]
use crate::modules::net::NetOperationMetrics;
#[cfg(all(test, feature = "net"))]
use crate::modules::net::NetRuntimeOwner;
use crate::runtime::value::{NetJobValue, RuntimeError, Value};
use crate::source::Span;
use crate::trace::{TraceError, TraceKind, TracePayload, TraceTiming};
#[cfg(feature = "net")]
use std::time::Duration;

const MAX_LIVE_NET_JOBS: usize = 64;
const MAX_RESERVED_NET_JOB_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

pub(super) enum NetJobTask {
    Completed(Result<Value, RuntimeError>),
    #[cfg(feature = "net")]
    Transport(NetOperation),
}

pub(super) struct LiveNetJob {
    pub(super) owner_scope: u64,
    pub(super) start_span: Span,
    reserved_response_bytes: u64,
    task: NetJobTask,
}

/// A deterministic, test-only view across the evaluator and transport
/// boundary. It deliberately reports only resource accounting: it never
/// exposes an operation, request, response value, or evaluator-owned data to
/// the driver or to XSH code.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EvaluatorNetRuntimeSnapshot {
    pub(super) driver_started: bool,
    pub(super) runtime_state: EvaluatorNetRuntimeState,
    pub(super) active_transport: usize,
    pub(super) queued_transport: usize,
    pub(super) live_jobs: usize,
    pub(super) completed_unconsumed_jobs: usize,
    pub(super) completed_response_bytes: usize,
    pub(super) agent_count: usize,
    pub(super) file_io_active: usize,
    pub(super) file_io_queued: usize,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EvaluatorNetRuntimeState {
    Uninitialized,
    Open,
    ShuttingDown,
    Failed,
    Stopped,
}

#[cfg(test)]
impl NetJobTask {
    /// `Some` distinguishes a terminal transport error (zero buffered bytes)
    /// from a job whose transport has not yet reached a terminal state.
    fn completed_response_bytes(&self) -> Option<usize> {
        match self {
            Self::Completed(result) => Some(net_job_result_body_bytes(result)),
            #[cfg(feature = "net")]
            Self::Transport(operation) => {
                let metrics = operation.metrics();
                metrics.completed_at_us.map(|_| metrics.response_bytes)
            }
        }
    }
}

/// Evaluator-owned copy of safe transport facts. Keeping this type here means
/// tracing remains available in no-network builds while the driver itself
/// never obtains evaluator trace state.
#[derive(Clone, Debug, Eq, PartialEq)]
struct NetJobTraceMetrics {
    accepted_at_us: u64,
    transport_started_at_us: Option<u64>,
    completed_at_us: Option<u64>,
    queue_duration_us: Option<u64>,
    transport_duration_us: Option<u64>,
    status: Option<i64>,
    response_bytes: usize,
    terminal_error_kind: Option<String>,
}

#[cfg(feature = "net")]
impl From<NetOperationMetrics> for NetJobTraceMetrics {
    fn from(metrics: NetOperationMetrics) -> Self {
        Self {
            accepted_at_us: metrics.accepted_at_us,
            transport_started_at_us: metrics.transport_started_at_us,
            completed_at_us: metrics.completed_at_us,
            queue_duration_us: metrics.queue_duration_us,
            transport_duration_us: metrics.transport_duration_us,
            status: metrics.status,
            response_bytes: metrics.response_bytes,
            terminal_error_kind: metrics.terminal_error_kind,
        }
    }
}

impl Evaluator {
    #[cfg(test)]
    pub(super) fn net_runtime_snapshot(&self) -> EvaluatorNetRuntimeSnapshot {
        #[cfg(feature = "net")]
        let runtime = self.net_runtime.as_ref().map(NetRuntimeOwner::snapshot);

        let (completed_unconsumed_jobs, completed_response_bytes) = self
            .net_jobs
            .values()
            .filter_map(|job| job.task.completed_response_bytes())
            .fold((0_usize, 0_usize), |(jobs, bytes), response_bytes| {
                (jobs + 1, bytes.saturating_add(response_bytes))
            });

        #[cfg(feature = "net")]
        let (
            driver_started,
            runtime_state,
            active_transport,
            queued_transport,
            file_io_active,
            file_io_queued,
        ) = runtime.map_or(
            (false, EvaluatorNetRuntimeState::Uninitialized, 0, 0, 0, 0),
            |snapshot| {
                let state = match snapshot.state {
                    xsh_net::NetRuntimeState::Open => EvaluatorNetRuntimeState::Open,
                    xsh_net::NetRuntimeState::ShuttingDown => {
                        EvaluatorNetRuntimeState::ShuttingDown
                    }
                    xsh_net::NetRuntimeState::Failed => EvaluatorNetRuntimeState::Failed,
                    xsh_net::NetRuntimeState::Stopped => EvaluatorNetRuntimeState::Stopped,
                };
                (
                    snapshot.driver_started,
                    state,
                    snapshot.active_transport,
                    snapshot.queued_transport,
                    snapshot.file_io_active,
                    snapshot.file_io_queued,
                )
            },
        );
        #[cfg(not(feature = "net"))]
        let (
            driver_started,
            runtime_state,
            active_transport,
            queued_transport,
            file_io_active,
            file_io_queued,
        ) = (false, EvaluatorNetRuntimeState::Uninitialized, 0, 0, 0, 0);

        EvaluatorNetRuntimeSnapshot {
            driver_started,
            runtime_state,
            active_transport,
            queued_transport,
            live_jobs: self.net_jobs.len(),
            completed_unconsumed_jobs,
            completed_response_bytes,
            agent_count: self.net_agents.len(),
            file_io_active,
            file_io_queued,
        }
    }

    /// Reserves the public-job and conservative buffered-response capacity
    /// before transport admission. The reservation lasts until `wait`,
    /// `cancel`, scope cleanup, or evaluator teardown consumes the handle.
    pub(super) fn admit_net_job(
        &mut self,
        reserved_response_bytes: u64,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if self.net_jobs.len() >= MAX_LIVE_NET_JOBS
            || reserved_response_bytes > MAX_RESERVED_NET_JOB_RESPONSE_BYTES
            || self
                .net_job_reserved_response_bytes
                .saturating_add(reserved_response_bytes)
                > MAX_RESERVED_NET_JOB_RESPONSE_BYTES
        {
            return Err(RuntimeError::new(
                "net-overload",
                "network job admission or buffered-response capacity is full",
            )
            .with_span(span));
        }
        self.net_job_reserved_response_bytes += reserved_response_bytes;
        Ok(())
    }

    pub(super) fn release_net_job_admission(&mut self, reserved_response_bytes: u64) {
        self.net_job_reserved_response_bytes = self
            .net_job_reserved_response_bytes
            .saturating_sub(reserved_response_bytes);
    }

    pub(super) fn net_job_value(
        &mut self,
        task: NetJobTask,
        start_span: Span,
        reserved_response_bytes: u64,
    ) -> NetJobValue {
        let id = self.next_net_job_id;
        self.next_net_job_id += 1;
        let metrics = match &task {
            NetJobTask::Completed(_) => None,
            #[cfg(feature = "net")]
            NetJobTask::Transport(operation) => Some(operation.metrics().into()),
        };
        self.register_owned_resource(self.current_scope_id(), super::ResourceKind::Activity);
        self.net_jobs.insert(
            id,
            LiveNetJob {
                owner_scope: self.current_scope_id(),
                start_span,
                reserved_response_bytes,
                task,
            },
        );
        self.trace_net_job(
            TraceKind::NetJobAccepted,
            start_span,
            id,
            reserved_response_bytes,
            0,
            metrics.as_ref(),
            None,
        );
        NetJobValue { id, owner: self.resource_owner.clone() }
    }

    pub(super) fn wait_net_job(
        &mut self,
        handle: NetJobValue,
        method_span: Span,
    ) -> Result<Value, RuntimeError> {
        if !std::sync::Arc::ptr_eq(&self.resource_owner, &handle.owner) {
            return Err(invalid_net_job_error(handle.id, method_span));
        }
        let Some(live) = self.take_net_job(&handle.id) else {
            return Err(invalid_net_job_error(handle.id, method_span));
        };
        let (result, metrics) =
            self.wait_net_job_task(live.task, live.start_span, method_span, true);
        if let Some(metrics) = metrics.as_ref() {
            self.trace_net_transport_lifecycle(
                live.start_span,
                handle.id,
                live.reserved_response_bytes,
                metrics,
            );
        }
        self.trace_net_job(
            TraceKind::NetJobWait,
            method_span,
            handle.id,
            live.reserved_response_bytes,
            net_job_result_body_bytes(&result),
            metrics.as_ref(),
            result.as_ref().err(),
        );
        self.release_net_job_admission(live.reserved_response_bytes);
        result
    }

    pub(super) fn cancel_net_job(
        &mut self,
        handle: NetJobValue,
        method_span: Span,
    ) -> Result<(), RuntimeError> {
        #[cfg(not(feature = "net"))]
        let _ = method_span;
        if !std::sync::Arc::ptr_eq(&self.resource_owner, &handle.owner) {
            return Err(invalid_net_job_error(handle.id, method_span));
        }
        let Some(live) = self.take_net_job(&handle.id) else {
            return Err(invalid_net_job_error(handle.id, method_span));
        };
        let (result, metrics): (Result<(), RuntimeError>, Option<NetJobTraceMetrics>) =
            match live.task {
                NetJobTask::Completed(_) => (Ok(()), None),
                #[cfg(feature = "net")]
                NetJobTask::Transport(operation) => {
                    let result = match operation.cancel() {
                        Ok(()) => {
                            let _ = self.wait_transport_operation(
                                &operation,
                                live.start_span,
                                method_span,
                                false,
                            );
                            Ok(())
                        }
                        Err(error) => {
                            Err(net::runtime_error(error, method_span))
                        }
                    };
                    (result, Some(operation.metrics().into()))
                }
            };
        if let Some(metrics) = metrics.as_ref() {
            self.trace_net_transport_lifecycle(
                live.start_span,
                handle.id,
                live.reserved_response_bytes,
                metrics,
            );
        }
        self.trace_net_job(
            TraceKind::NetJobCancel,
            method_span,
            handle.id,
            live.reserved_response_bytes,
            0,
            metrics.as_ref(),
            result.as_ref().err(),
        );
        self.release_net_job_admission(live.reserved_response_bytes);
        result
    }

    pub(super) fn cleanup_net_jobs(
        &mut self,
        scope_id: u64,
        primary: Result<Flow, RuntimeError>,
    ) -> Result<Flow, RuntimeError> {
        if self.signal_state.shutdown_force {
            return primary;
        }
        let mut primary_failed = matches!(primary, Err(_) | Ok(Flow::Propagate(_)));
        let mut result = primary;
        let ids = self
            .net_jobs
            .iter()
            .filter_map(|(id, live)| (live.owner_scope == scope_id).then_some(*id))
            .collect::<Vec<_>>();
        for id in ids {
            let Some(live) = self.take_net_job(&id) else {
                continue;
            };
            self.release_net_job_admission(live.reserved_response_bytes);
            let (cleanup, metrics): (Result<(), RuntimeError>, Option<NetJobTraceMetrics>) =
                match live.task {
                    NetJobTask::Completed(_) => (Ok(()), None),
                    #[cfg(feature = "net")]
                    NetJobTask::Transport(operation) => {
                        let cleanup = match operation.cancel() {
                            Ok(()) => {
                                let _ = self.wait_transport_operation(
                                    &operation,
                                    live.start_span,
                                    live.start_span,
                                    false,
                                );
                                Ok(())
                            }
                            Err(error) => Err(net::runtime_error(error, live.start_span)),
                        };
                        (cleanup, Some(operation.metrics().into()))
                    }
                };
            if let Some(metrics) = metrics.as_ref() {
                self.trace_net_transport_lifecycle(
                    live.start_span,
                    id,
                    live.reserved_response_bytes,
                    metrics,
                );
            }
            self.trace_net_job(
                TraceKind::NetJobCleanup,
                live.start_span,
                id,
                live.reserved_response_bytes,
                0,
                metrics.as_ref(),
                cleanup.as_ref().err(),
            );
            if let Err(error) = cleanup
                && !primary_failed
            {
                result = Err(error.with_span(live.start_span));
                primary_failed = true;
            }
        }
        result
    }

    pub(super) fn cancel_net_jobs_for_signal(
        &mut self,
        method_span: Span,
    ) -> Result<(), RuntimeError> {
        #[cfg(not(feature = "net"))]
        let _ = method_span;
        let ids = self.net_jobs.keys().copied().collect::<Vec<_>>();
        let mut first_error = None;
        for id in ids {
            let Some(live) = self.take_net_job(&id) else {
                continue;
            };
            self.release_net_job_admission(live.reserved_response_bytes);
            let (cleanup, metrics): (Result<(), RuntimeError>, Option<NetJobTraceMetrics>) =
                match live.task {
                    NetJobTask::Completed(_) => (Ok(()), None),
                    #[cfg(feature = "net")]
                    NetJobTask::Transport(operation) => {
                        let cleanup =
                            match operation.cancel() {
                                Ok(()) => {
                                    let _ = self.wait_transport_operation(
                                        &operation,
                                        live.start_span,
                                        method_span,
                                        false,
                                    );
                                    Ok(())
                                }
                                Err(error) => Err(net::runtime_error(error, method_span)),
                            };
                        (cleanup, Some(operation.metrics().into()))
                    }
                };
            if let Some(metrics) = metrics.as_ref() {
                self.trace_net_transport_lifecycle(
                    method_span,
                    id,
                    live.reserved_response_bytes,
                    metrics,
                );
            }
            self.trace_net_job(
                TraceKind::NetJobShutdownCancel,
                method_span,
                id,
                live.reserved_response_bytes,
                0,
                metrics.as_ref(),
                cleanup.as_ref().err(),
            );
            if let Err(error) = cleanup
                && first_error.is_none()
            {
                first_error = Some(error.with_span(live.start_span));
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn wait_net_job_task(
        &mut self,
        task: NetJobTask,
        start_span: Span,
        method_span: Span,
        checkpoint: bool,
    ) -> (Result<Value, RuntimeError>, Option<NetJobTraceMetrics>) {
        #[cfg(not(feature = "net"))]
        let _ = (method_span, checkpoint);
        match task {
            NetJobTask::Completed(result) => {
                (result.map_err(|error| error.with_span(start_span)), None)
            }
            #[cfg(feature = "net")]
            NetJobTask::Transport(operation) => {
                let result =
                    self.wait_transport_operation(&operation, start_span, method_span, checkpoint);
                (result, Some(operation.metrics().into()))
            }
        }
    }

    #[cfg(feature = "net")]
    pub(super) fn wait_transport_operation(
        &mut self,
        operation: &NetOperation,
        start_span: Span,
        method_span: Span,
        checkpoint: bool,
    ) -> Result<Value, RuntimeError> {
        self.network_wait_depth += 1;
        let result =
            self.wait_transport_operation_inner(operation, start_span, method_span, checkpoint);
        self.network_wait_depth -= 1;
        result
    }

    #[cfg(feature = "net")]
    fn wait_transport_operation_inner(
        &mut self,
        operation: &NetOperation,
        start_span: Span,
        method_span: Span,
        checkpoint: bool,
    ) -> Result<Value, RuntimeError> {
        loop {
            if checkpoint && let Err(error) = self.service_pending_signal(method_span) {
                let _ = operation.cancel();
                let _ = operation.try_receive(Duration::from_millis(25));
                return Err(error);
            }
            match operation
                .try_receive(Duration::from_millis(25))
                .map_err(|error| net::runtime_error(error, start_span))? {
                Some(Ok(response)) => return Ok(net::response_value(response)),
                Some(Err(error)) => {
                    return Err(net::runtime_error(error, start_span));
                }
                None => {}
            }
        }
    }

    #[cfg(not(feature = "net"))]
    pub(super) fn wait_transport_operation(
        &mut self,
        _operation: &crate::modules::net::NetOperation,
        _start_span: Span,
        method_span: Span,
        _checkpoint: bool,
    ) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new("net-disabled", "net feature is disabled").with_span(method_span))
    }

    pub(super) fn wait_for_net_batch_completion(
        &mut self,
        operations: &[&NetOperation],
        span: Span,
    ) -> Result<(usize, Result<Value, RuntimeError>), RuntimeError> {
        self.network_wait_depth += 1;
        let result = self.wait_for_net_batch_completion_inner(operations, span);
        self.network_wait_depth -= 1;
        result
    }

    #[cfg(feature = "net")]
    fn wait_for_net_batch_completion_inner(
        &mut self,
        operations: &[&NetOperation],
        span: Span,
    ) -> Result<(usize, Result<Value, RuntimeError>), RuntimeError> {
        loop {
            self.service_pending_signal(span)?;
            if let Some(completion) = net::receive_any(operations, Duration::from_millis(25), span)?
            {
                return Ok(completion);
            }
        }
    }

    #[cfg(not(feature = "net"))]
    fn wait_for_net_batch_completion_inner(
        &mut self,
        _operations: &[&NetOperation],
        span: Span,
    ) -> Result<(usize, Result<Value, RuntimeError>), RuntimeError> {
        Err(RuntimeError::new("net-disabled", "net feature is disabled").with_span(span))
    }

    pub(super) fn cancel_net_batch_operations(
        &mut self,
        operations: Vec<NetOperation>,
        span: Span,
    ) {
        #[cfg(feature = "net")]
        {
            let mut operations = operations;
            for operation in &operations {
                let _ = operation.cancel();
            }
            while !operations.is_empty() {
                let active = operations.iter().collect::<Vec<_>>();
                match net::receive_any(&active, Duration::from_millis(25), span) {
                    Ok(Some((index, _))) => {
                        operations.remove(index);
                    }
                    Ok(None) => {}
                    Err(_) => return,
                }
            }
        }
        #[cfg(not(feature = "net"))]
        let _ = (operations, span);
    }

    fn trace_net_job(
        &mut self,
        kind: TraceKind,
        span: Span,
        job_id: u64,
        reserved_response_bytes: u64,
        completed_response_bytes: usize,
        metrics: Option<&NetJobTraceMetrics>,
        error: Option<&RuntimeError>,
    ) {
        self.trace_leaf(
            kind,
            Some(span),
            None,
            TracePayload::NetJob {
                job_id,
                reserved_response_bytes,
                completed_response_bytes,
                accepted_at_us: metrics.map(|metrics| metrics.accepted_at_us),
                transport_started_at_us: metrics
                    .and_then(|metrics| metrics.transport_started_at_us),
                completed_at_us: metrics.and_then(|metrics| metrics.completed_at_us),
                queue_duration_us: metrics.and_then(|metrics| metrics.queue_duration_us),
                transport_duration_us: metrics.and_then(|metrics| metrics.transport_duration_us),
                status: metrics.and_then(|metrics| metrics.status),
                terminal_error_kind: metrics
                    .and_then(|metrics| metrics.terminal_error_kind.clone()),
                error: error.map(|error| TraceError::new(&error.kind, &error.message)),
            },
        );
    }

    fn trace_net_transport_lifecycle(
        &mut self,
        span: Span,
        job_id: u64,
        reserved_response_bytes: u64,
        metrics: &NetJobTraceMetrics,
    ) {
        let payload = || TracePayload::NetJob {
            job_id,
            reserved_response_bytes,
            completed_response_bytes: metrics.response_bytes,
            accepted_at_us: Some(metrics.accepted_at_us),
            transport_started_at_us: metrics.transport_started_at_us,
            completed_at_us: metrics.completed_at_us,
            queue_duration_us: metrics.queue_duration_us,
            transport_duration_us: metrics.transport_duration_us,
            status: metrics.status,
            terminal_error_kind: metrics.terminal_error_kind.clone(),
            error: None,
        };
        self.trace_leaf_with_timing(
            TraceKind::NetJobScheduled,
            Some(span),
            None,
            payload(),
            TraceTiming::new(Some(metrics.accepted_at_us), None),
        );
        if let Some(started_at_us) = metrics.transport_started_at_us {
            self.trace_leaf_with_timing(
                TraceKind::NetTransportStarted,
                Some(span),
                None,
                payload(),
                TraceTiming::new(Some(started_at_us), None),
            );
        }
        if let Some(completed_at_us) = metrics.completed_at_us {
            self.trace_leaf_with_timing(
                TraceKind::NetTransportCompleted,
                Some(span),
                None,
                payload(),
                TraceTiming::new(Some(completed_at_us), metrics.transport_duration_us),
            );
        }
    }
}

fn net_job_result_body_bytes(result: &Result<Value, RuntimeError>) -> usize {
    let Ok(Value::Record(record)) = result else {
        return 0;
    };
    match record.get("body") {
        Some(Value::Bytes(body)) => body.len(),
        _ => 0,
    }
}

pub(super) fn invalid_net_job_error(id: u64, span: Span) -> RuntimeError {
    RuntimeError::new("net-job-not-live", format!("NetJob {id} is no longer live")).with_span(span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::value::RecordMap;
    use crate::symbol::Name;

#[test]
fn middle_scope_job_assigned_from_deeper_scope_reaches_outer_owner() {
    let span = super::super::zero_span();
    let mut evaluator = Evaluator::new(Vec::new());
    let outer = evaluator.current_scope_id();
    let middle = evaluator.enter_owned_host_scope();
    let job = evaluator.net_job_value(NetJobTask::Completed(Ok(Value::Int(7))), span, 0);
    let inner = evaluator.enter_owned_host_scope();
    evaluator.transfer_owned_host_resources_in_lowered_value(
        &super::super::LoweredValue::NetJob(Box::new(job.clone())), outer,
    );
    evaluator.exit_owned_host_scope(inner).expect("close inner scope");
    evaluator.exit_owned_host_scope(middle).expect("close middle scope");
    assert_eq!(evaluator.wait_net_job(job, span).expect("transferred job stays live"), Value::Int(7));
}

#[test]
fn specialized_stats_blob_transfers_handles_in_its_map() {
    let span = super::super::zero_span();
    let mut evaluator = Evaluator::new(Vec::new());
    let outer = evaluator.current_scope_id();
    let scope = evaluator.enter_owned_host_scope();
    let job = evaluator.net_job_value(NetJobTask::Completed(Ok(Value::Int(7))), span, 0);
    let summary = super::super::LoweredValue::StatsBlob(Box::new(super::super::LoweredStatsValue {
        blanks: 0,
        blobs: std::collections::BTreeMap::from([(crate::runtime::value::MapKey::Str("job".into()), super::super::LoweredValue::NetJob(Box::new(job.clone())))]),
        code: 0,
        comments: 0,
    }));
    evaluator.transfer_owned_host_resources_in_lowered_value(&summary, outer);
    evaluator.exit_owned_host_scope(scope).expect("close source scope");
    assert_eq!(evaluator.wait_net_job(job, span).expect("nested job stays live"), Value::Int(7));
}

#[test]
fn borrowing_a_caller_owned_job_does_not_move_it_into_the_callee() {
    let span = super::super::zero_span();
    let mut evaluator = Evaluator::new(Vec::new());
    let job = evaluator.net_job_value(NetJobTask::Completed(Ok(Value::Int(7))), span, 0);
    let callee = evaluator.enter_owned_host_scope();
    evaluator.transfer_owned_host_resources_in_lowered_value(
        &super::super::LoweredValue::NetJob(Box::new(job.clone())), callee,
    );
    evaluator.exit_owned_host_scope(callee).expect("close borrower scope");
    assert_eq!(evaluator.wait_net_job(job, span).expect("caller retains its job"), Value::Int(7));
}

    #[test]
    fn detached_descendant_received_in_the_current_scope_moves_to_its_consumer() {
        let span = super::super::zero_span();
        let mut evaluator = Evaluator::new(Vec::new());
        let consumer = evaluator.current_scope_id();
        let producer = evaluator.enter_owned_host_scope();
        let job = evaluator.net_job_value(NetJobTask::Completed(Ok(Value::Int(7))), span, 0);
        evaluator.detach_owned_host_scopes(1);
        evaluator.transfer_owned_host_resources_in_value(&Value::NetJob(Box::new(job.clone())), consumer);
        evaluator.reattach_owned_host_scopes(&[producer]);
        evaluator.exit_owned_host_scope(producer).expect("close producer");
        assert_eq!(evaluator.wait_net_job(job, span).expect("consumer retains detached descendant"), Value::Int(7));
    }

    #[test]
    fn detached_producer_scopes_transfer_into_a_sibling_consumer() {
        let span = super::super::zero_span();
        let mut evaluator = Evaluator::new(Vec::new());
        let first_consumer = evaluator.enter_owned_host_scope();
        let producer = evaluator.enter_owned_host_scope();
        let job = evaluator.net_job_value(NetJobTask::Completed(Ok(Value::Int(7))), span, 0);
        let nested = evaluator.enter_owned_host_scope();
        evaluator.detach_owned_host_scopes(2);
        evaluator.exit_owned_host_scope(first_consumer).expect("close first consumer");
        let consumer = evaluator.enter_owned_host_scope();
        evaluator.reattach_owned_host_scopes(&[producer, nested]);
        evaluator.transfer_owned_host_resources_in_value(&Value::NetJob(Box::new(job.clone())), consumer);
        evaluator.exit_owned_host_scope(nested).expect("close producer block");
        evaluator.exit_owned_host_scope(producer).expect("close producer");
        assert_eq!(evaluator.wait_net_job(job, span).expect("new consumer retains yielded job"), Value::Int(7));
        evaluator.exit_owned_host_scope(consumer).expect("close consumer");
    }

    #[test]
    fn typed_error_cause_transfers_its_nested_job() {
        let symbols = crate::symbol::SymbolOwner::default();
        let _symbols = symbols.enter();
        let span = super::super::zero_span();
        let mut evaluator = Evaluator::new(Vec::new());
        let outer = evaluator.current_scope_id();
        let scope = evaluator.enter_owned_host_scope();
        let job = evaluator.net_job_value(NetJobTask::Completed(Ok(Value::Int(7))), span, 0);
        let mut payload = RuntimeError::new("inner", "payload");
        payload.payload = RecordMap::from([("job".into(), Value::NetJob(Box::new(job.clone())))]);
        let error = Value::RunError(Box::new(crate::runtime::value::RunError::new("unknown", "outer")))
            .with_error_cause(Value::Error(Box::new(payload))).expect("error holds cause");
        evaluator.transfer_owned_host_resources_in_lowered_value(&super::super::LoweredValue::Error(Box::new(error)), outer);
        evaluator.exit_owned_host_scope(scope).expect("close source scope");
        assert_eq!(evaluator.wait_net_job(job, span).expect("error retains its nested job"), Value::Int(7));
    }

    #[test]
    fn foreign_job_wait_does_not_consume_a_matching_local_id() {
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);
        let mut parent = Evaluator::new(Vec::new());
        let mut worker = Evaluator::new(Vec::new());
        let foreign = parent.net_job_value(NetJobTask::Completed(Ok(Value::Int(1))), span, 0);
        let local = worker.net_job_value(NetJobTask::Completed(Ok(Value::Int(2))), span, 0);
        assert_eq!(foreign.id, local.id);

        let rejected = worker.wait_net_job(foreign.clone(), span);
        assert_eq!(rejected.expect_err("foreign job must be rejected").kind, "net-job-not-live");
        assert_eq!(worker.wait_net_job(local, span).expect("local job stays live"), Value::Int(2));
        assert_eq!(parent.wait_net_job(foreign, span).expect("parent job stays live"), Value::Int(1));
    }

    #[test]
    fn foreign_job_cancel_does_not_consume_a_matching_local_id() {
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);
        let mut parent = Evaluator::new(Vec::new());
        let mut worker = Evaluator::new(Vec::new());
        let foreign = parent.net_job_value(NetJobTask::Completed(Ok(Value::Int(1))), span, 0);
        let local = worker.net_job_value(NetJobTask::Completed(Ok(Value::Int(2))), span, 0);
        assert_eq!(foreign.id, local.id);

        let rejected = worker.cancel_net_job(foreign.clone(), span);
        assert_eq!(rejected.expect_err("foreign job must be rejected").kind, "net-job-not-live");
        assert_eq!(worker.wait_net_job(local, span).expect("local job stays live"), Value::Int(2));
        assert_eq!(parent.wait_net_job(foreign, span).expect("parent job stays live"), Value::Int(1));
    }

    #[test]
    fn runtime_snapshot_counts_completed_mock_jobs_without_starting_a_driver() {
        let mut evaluator = Evaluator::new(Vec::new());
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);

        assert_eq!(
            evaluator.net_runtime_snapshot(),
            EvaluatorNetRuntimeSnapshot {
                driver_started: false,
                runtime_state: EvaluatorNetRuntimeState::Uninitialized,
                active_transport: 0,
                queued_transport: 0,
                live_jobs: 0,
                completed_unconsumed_jobs: 0,
                completed_response_bytes: 0,
                agent_count: 0,
                file_io_active: 0,
                file_io_queued: 0,
            }
        );

        let response = Value::Record(RecordMap::from_name_values(vec![(
            Name::intern("body"),
            Value::Bytes(b"job".to_vec()),
        )]));
        evaluator.admit_net_job(8, span).expect("admit mock job");
        let job = evaluator.net_job_value(NetJobTask::Completed(Ok(response)), span, 8);

        let snapshot = evaluator.net_runtime_snapshot();
        assert!(!snapshot.driver_started);
        assert_eq!(
            snapshot.runtime_state,
            EvaluatorNetRuntimeState::Uninitialized
        );
        assert_eq!(snapshot.live_jobs, 1);
        assert_eq!(snapshot.completed_unconsumed_jobs, 1);
        assert_eq!(snapshot.completed_response_bytes, 3);
        assert_eq!(snapshot.agent_count, 0);
        assert_eq!(snapshot.active_transport, 0);
        assert_eq!(snapshot.queued_transport, 0);
        assert_eq!(snapshot.file_io_active, 0);
        assert_eq!(snapshot.file_io_queued, 0);

        let response = evaluator.wait_net_job(job, span).expect("wait mock job");
        assert_eq!(net_job_result_body_bytes(&Ok(response)), 3);
        let snapshot = evaluator.net_runtime_snapshot();
        assert_eq!(snapshot.live_jobs, 0);
        assert_eq!(snapshot.completed_unconsumed_jobs, 0);
        assert_eq!(snapshot.completed_response_bytes, 0);
    }
}
