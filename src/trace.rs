//! Structured trace events and traceback data for `libxsh` consumers.
//!
//! This is the supported data tier: event identity, relationships, source
//! spans, payloads, statuses, and errors are emitted here. CLI formatting,
//! normalization, terminal tables, and syscall presentation belong to `xsht`.

#![allow(clippy::single_call_fn)]

use crate::source::{SourceMap, Span};
use std::fmt::Write as _;

/// Structured trace and traceback data exposed by `libxsh`.
///
/// Trace renderers and syscall presentation are owned by `xsht`.
pub mod model {
    pub use super::{
        TraceArg, TraceEnv, TraceError, TraceEvent, TraceKind, TracePayload, TraceStatus,
        TraceStatusKind, TraceTiming, Traceback, TracebackFrame, TracebackFrameKind,
        TracebackRenderer,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    pub event_id: u64,
    pub parent_event_id: Option<u64>,
    pub depth: u32,
    pub kind: TraceKind,
    pub source_span: Option<Span>,
    pub definition_span: Option<Span>,
    pub name: Option<String>,
    pub api_id: Option<String>,
    pub timing: TraceTiming,
    pub payload: TracePayload,
}

impl TraceEvent {
    pub fn new(event_id: u64, kind: TraceKind) -> Self {
        Self {
            event_id,
            parent_event_id: None,
            depth: 0,
            kind,
            source_span: None,
            definition_span: None,
            name: None,
            api_id: None,
            timing: TraceTiming::default(),
            payload: TracePayload::None,
        }
    }

    pub fn with_parent(mut self, parent_event_id: u64, depth: u32) -> Self {
        self.parent_event_id = Some(parent_event_id);
        self.depth = depth;
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.source_span = Some(span);
        self
    }

    pub fn with_definition_span(mut self, span: Span) -> Self {
        self.definition_span = Some(span);
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_api_id(mut self, api_id: impl Into<String>) -> Self {
        self.api_id = Some(api_id.into());
        self
    }

    pub fn with_timing(mut self, timing: TraceTiming) -> Self {
        self.timing = timing;
        self
    }

    pub fn with_payload(mut self, payload: TracePayload) -> Self {
        self.payload = payload;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceKind {
    ScriptEnter,
    ScriptExit,
    ProcEnter,
    ProcExit,
    PureEnter,
    PureExit,
    CoreCall,
    CoreResult,
    ModuleCall,
    ModuleResult,
    MethodCall,
    MethodResult,
    RunStart,
    RunEnd,
    SpawnStart,
    SpawnReady,
    WaitStart,
    WaitEnd,
    SpawnCancel,
    PipelineEnter,
    PipelineExit,
    PipelineSegmentStart,
    PipelineSegmentEnd,
    RedirectionSetup,
    StreamEnter,
    StreamExit,
    StreamStageEnter,
    StreamStageExit,
    StreamItemError,
    ParallelJobStart,
    ParallelJobEnd,
    ParallelCancel,
    RetryAttempt,
    CwdEnter,
    CwdExit,
    SignalReceived,
    SignalHookEnter,
    SignalHookExit,
    SignalForward,
    SignalEscalate,
    ResultPropagate,
    RuntimeError,
}

impl TraceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScriptEnter => "script.enter",
            Self::ScriptExit => "script.exit",
            Self::ProcEnter => "proc.enter",
            Self::ProcExit => "proc.exit",
            Self::PureEnter => "pure.enter",
            Self::PureExit => "pure.exit",
            Self::CoreCall => "core.call",
            Self::CoreResult => "core.result",
            Self::ModuleCall => "module.call",
            Self::ModuleResult => "module.result",
            Self::MethodCall => "method.call",
            Self::MethodResult => "method.result",
            Self::RunStart => "run.start",
            Self::RunEnd => "run.end",
            Self::SpawnStart => "spawn.start",
            Self::SpawnReady => "spawn.ready",
            Self::WaitStart => "wait.start",
            Self::WaitEnd => "wait.end",
            Self::SpawnCancel => "spawn.cancel",
            Self::PipelineEnter => "pipeline.enter",
            Self::PipelineExit => "pipeline.exit",
            Self::PipelineSegmentStart => "pipeline.segment.start",
            Self::PipelineSegmentEnd => "pipeline.segment.end",
            Self::RedirectionSetup => "redirection.setup",
            Self::StreamEnter => "stream.enter",
            Self::StreamExit => "stream.exit",
            Self::StreamStageEnter => "stream.stage.enter",
            Self::StreamStageExit => "stream.stage.exit",
            Self::StreamItemError => "stream.item.error",
            Self::ParallelJobStart => "parallel.job.start",
            Self::ParallelJobEnd => "parallel.job.end",
            Self::ParallelCancel => "parallel.cancel",
            Self::RetryAttempt => "retry.attempt",
            Self::CwdEnter => "cwd.enter",
            Self::CwdExit => "cwd.exit",
            Self::SignalReceived => "signal.received",
            Self::SignalHookEnter => "signal.hook.enter",
            Self::SignalHookExit => "signal.hook.exit",
            Self::SignalForward => "signal.forward",
            Self::SignalEscalate => "signal.escalate",
            Self::ResultPropagate => "result.propagate",
            Self::RuntimeError => "runtime.error",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TraceTiming {
    pub start_time_us: Option<u64>,
    pub duration_us: Option<u64>,
}

impl TraceTiming {
    pub const fn new(start_time_us: Option<u64>, duration_us: Option<u64>) -> Self {
        Self {
            start_time_us,
            duration_us,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TracePayload {
    None,
    Core {
        argv: Vec<TraceArg>,
    },
    RunStart {
        target: TraceArg,
        argv: Vec<TraceArg>,
        cwd: TraceArg,
        env: Vec<TraceEnv>,
    },
    RunEnd {
        pid: Option<u32>,
        status: Option<TraceStatus>,
        error: Option<TraceError>,
    },
    SpawnStart {
        handle_id: Option<u64>,
        target: TraceArg,
        argv: Vec<TraceArg>,
        cwd: TraceArg,
        env: Vec<TraceEnv>,
        detached: bool,
    },
    SpawnReady {
        handle_id: u64,
        pid: Option<u32>,
    },
    WaitStart {
        handle_ids: Vec<u64>,
    },
    WaitEnd {
        handle_id: Option<u64>,
        pid: Option<u32>,
        status: Option<TraceStatus>,
        error: Option<TraceError>,
    },
    SpawnCancel {
        handle_id: u64,
        pid: Option<u32>,
        signal: String,
        kill_after_ms: u64,
        error: Option<TraceError>,
    },
    PipelineEnd {
        status: Option<TraceStatus>,
        error: Option<TraceError>,
    },
    PipelineSegmentStart {
        index: usize,
        target: TraceArg,
        argv: Vec<TraceArg>,
        cwd: TraceArg,
        env: Vec<TraceEnv>,
    },
    PipelineSegmentEnd {
        index: usize,
        pid: Option<u32>,
        status: Option<TraceStatus>,
        error: Option<TraceError>,
    },
    Redirection {
        op: String,
        target: Option<TraceArg>,
        fd: Option<i32>,
        error: Option<TraceError>,
    },
    StreamStage {
        stage: String,
        item_count: Option<usize>,
        error: Option<TraceError>,
    },
    StreamItem {
        stage: String,
        item_index: usize,
        error: Option<TraceError>,
    },
    ParallelJob {
        stage: String,
        item_index: usize,
        error: Option<TraceError>,
    },
    RetryAttempt {
        attempt: usize,
        max_attempts: usize,
        next_delay_ms: Option<u64>,
        error: Option<TraceError>,
    },
    Cwd {
        previous: TraceArg,
        current: TraceArg,
    },
    Signal {
        signal_name: String,
        signal_number: i32,
        phase: String,
        matching_hook: bool,
        forwarded: bool,
        pre_cancel_ms: Option<u64>,
        escalation_signal_name: Option<String>,
        escalation_signal_number: Option<i32>,
        hook_error: Option<TraceError>,
    },
    ResultPropagate {
        error_kind: String,
    },
    RuntimeError {
        error: TraceError,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceArg {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEnv {
    pub name: TraceArg,
    pub value: TraceArg,
}

impl TraceArg {
    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    pub fn text(text: impl AsRef<str>) -> Self {
        Self {
            bytes: text.as_ref().as_bytes().to_vec(),
        }
    }

    pub fn quoted(&self) -> String {
        quote_bytes(&self.bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceStatus {
    pub success: bool,
    pub kind: TraceStatusKind,
    pub code: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceStatusKind {
    Exit,
    Signal,
    Exec,
}

impl TraceStatusKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Signal => "signal",
            Self::Exec => "exec",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceError {
    pub kind: String,
    pub message: String,
}

impl TraceError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Traceback {
    pub failing_span: Option<Span>,
    pub exe_path: String,
    pub operation_kind: String,
    pub error: TraceError,
    pub frames: Vec<TracebackFrame>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TracebackFrame {
    pub kind: TracebackFrameKind,
    pub name: String,
    pub definition_span: Option<Span>,
    pub call_span: Option<Span>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TracebackFrameKind {
    Proc,
    Pure,
}

impl TracebackFrameKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proc => "proc",
            Self::Pure => "pure",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TracebackRenderer;

impl TracebackRenderer {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, traceback: &Traceback, sources: &SourceMap) -> String {
        let mut output = String::new();
        output.push_str("runtime traceback\n");

        if !traceback.exe_path.is_empty() {
            output.push_str("executable: ");
            output.push_str(&traceback.exe_path);
            output.push('\n');
        }

        output.push_str("operation: ");
        output.push_str(&traceback.operation_kind);
        output.push('\n');
        output.push_str("error: ");
        output.push_str(&traceback.error.kind);
        if !traceback.error.message.is_empty() && traceback.error.message != traceback.error.kind {
            output.push_str(": ");
            output.push_str(&traceback.error.message);
        }
        output.push('\n');

        if !traceback.frames.is_empty() {
            output.push_str("call path:\n");
            for (index, frame) in traceback.frames.iter().enumerate() {
                let _ = write!(
                    output,
                    "  {}. {} {}",
                    index + 1,
                    frame.kind.as_str(),
                    frame.name
                );
                if let Some(span) = frame.call_span.or(frame.definition_span) {
                    output.push_str(" at ");
                    render_span_text(span, sources, &mut output);
                }
                output.push('\n');
            }
        }

        output
    }
}

fn render_span_text(span: Span, sources: &SourceMap, output: &mut String) {
    let Some(start) = sources.location(span.source_id, span.start()) else {
        let _ = write!(
            output,
            "<source:{}:{}..{}>",
            span.source_id.raw(),
            span.start(),
            span.end()
        );
        return;
    };
    let Some(end) = sources.location(span.source_id, span.end()) else {
        let _ = write!(output, "{}:{}:{}", start.file, start.line, start.column);
        return;
    };
    let _ = write!(
        output,
        "{}:{}:{}-{}:{}",
        start.file, start.line, start.column, end.line, end.column
    );
}
fn quote_bytes(bytes: &[u8]) -> String {
    let mut output = String::from("b\"");
    for &byte in bytes {
        match byte {
            b'\\' => output.push_str("\\\\"),
            b'"' => output.push_str("\\\""),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\t' => output.push_str("\\t"),
            0x20..=0x7e => output.push(byte as char),
            _ => {
                let _ = write!(output, "\\x{byte:02x}");
            }
        }
    }
    output.push('"');
    output
}
