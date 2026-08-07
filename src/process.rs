//! Canonical process and cancellation types for `libxsh` consumers.
//!
//! These operations have host effects. Callers must account for signal-handler
//! installation, terminal ownership, blocking waits, and child ownership
//! transfer as documented by the underlying operation.
//!
//! `install_*_signal_handlers` installs process-wide handlers and returns a
//! guard that restores the prior state. The interactive process-group helpers
//! require a controlling terminal on supported Unix hosts. `run_*` and
//! `wait_managed` may block until child processes finish; managed-child and
//! process-group handles retain or transfer child ownership according to their
//! explicit release/reaper operations.

pub use crate::runtime::process::{
    CAPTURE_LIMIT, Cancellation, CancellationDecision, CancellationPolicy, ChildWaitOutcome,
    FileRedirectionMode, ForegroundTerminal, InteractiveProcessGroupGuard, ManagedChild,
    ManagedStdio, ProcessEnd, ProcessGroup, ProcessGroupConfig, ProcessInvocation, ProcessOutput,
    ProcessRedirection, ProcessSegmentStatus, ProcessSegmentStatusKind, ProcessStatus,
    ProcessStatusKind, RedirectionStream, SignalHandlerGuard, SpawnManagedOptions, SpawnOptions,
    SpawnedProcess, WaitMode, cancel_managed, cancellation_escalated_signal,
    cancellation_requested_signal, clear_cancellation_request,
    initialize_interactive_process_group, install_cancellation_signal_handlers,
    install_immediate_cancellation_signal_handlers, install_interactive_signal_handlers,
    path_bytes, poll_managed, release_to_reaper, resolve_executable, run_capture,
    run_capture_with_policy, run_capture_with_stderr, run_capture_with_stderr_policy, run_inherit,
    run_inherit_with_policy, run_pipeline_inherit, run_pipeline_inherit_with_policy,
    run_quiet_with_policy, spawn_command, spawn_managed, wait_managed,
};
