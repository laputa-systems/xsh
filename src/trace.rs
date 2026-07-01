#![allow(clippy::single_call_fn)]

#[cfg(feature = "tools")]
use crate::modules::json::{
    raw_json_array, raw_json_bool, raw_json_f64, raw_json_i64, raw_json_object, raw_json_string,
    raw_json_u64, raw_json_usize,
};
use crate::source::{SourceMap, Span};
#[cfg(feature = "tools")]
use crate::terminal::table::{
    TableAlign, TextTableColumn, render_text_table, table_width, terminal_table_width_for_stderr,
};
#[cfg(feature = "tools")]
use miniserde::json::Value as JsonValue;
#[cfg(feature = "tools")]
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::fmt::Write as _;
#[cfg(feature = "tools")]
use std::io;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    pub event_id: u64,
    pub parent_event_id: Option<u64>,
    pub depth: u32,
    pub kind: TraceKind,
    pub source_span: Option<Span>,
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

impl TracePayload {
    #[cfg(feature = "tools")]
    fn normalize(&mut self) {
        match self {
            Self::RunEnd { pid, .. }
            | Self::SpawnReady { pid, .. }
            | Self::WaitEnd { pid, .. }
            | Self::SpawnCancel { pid, .. }
            | Self::PipelineSegmentEnd { pid, .. } => {
                *pid = None;
            }
            _ => {}
        }
    }
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
pub struct SyscallTraceRecord {
    pub pid: u32,
    pub program: String,
    pub syscall: String,
    pub errored: bool,
    pub elapsed_ns: u64,
    /// Absolute start time of this syscall in Unix microseconds.
    /// Set by the ptrace supervisor using a (Instant, SystemTime) epoch pair.
    /// Zero when not populated (non-Linux builds or pre-epoch records).
    pub start_time_us: u64,
}

impl SyscallTraceRecord {
    pub fn new(
        pid: u32,
        program: impl Into<String>,
        syscall: impl Into<String>,
        errored: bool,
        elapsed_ns: u64,
        start_time_us: u64,
    ) -> Self {
        Self {
            pid,
            program: program.into(),
            syscall: syscall.into(),
            errored,
            elapsed_ns,
            start_time_us,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyscallSummary {
    pub syscall_count: u64,
    pub elapsed_ns: u128,
    /// Total wall-clock time of the traced script execution (set by the ptrace supervisor).
    pub wall_time_ns: Option<u64>,
    pub by_syscall: Vec<SyscallSummaryRow>,
    pub by_program: Vec<SyscallProgramSummary>,
    pub by_process: Vec<SyscallProcessSummary>,
    /// Per-XSH-operation syscall attribution. Populated by `attribute_operations`
    /// when both syscall timestamps and XSH trace JSONL are available.
    pub by_operation: Vec<SyscallOperationSummary>,
}

/// Time spent in syscalls attributed to a single XSH module/method/run call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyscallOperationSummary {
    /// XSH operation kind: "module", "method", "run", "proc", "pure", etc.
    pub kind: String,
    /// Operation name (e.g. "fs.walk", "archive.tar_create", "printf").
    pub name: String,
    /// Total nanoseconds spent inside syscalls during this operation.
    pub elapsed_ns: u64,
    /// Number of distinct syscall invocations during this operation.
    pub syscall_count: u64,
    /// Per-syscall breakdown (top syscalls by count).
    pub by_syscall: Vec<SyscallCountRow>,
}

impl SyscallSummary {
    pub fn from_records(records: &[SyscallTraceRecord]) -> Self {
        let mut by_syscall = BTreeMap::<String, SyscallSummaryRow>::new();
        let mut process_programs = BTreeMap::<u32, String>::new();
        let mut process_rows = BTreeMap::<u32, BTreeMap<String, u64>>::new();
        let mut elapsed_ns = 0u128;

        for record in records {
            elapsed_ns += u128::from(record.elapsed_ns);
            process_programs.insert(record.pid, record.program.clone());
            process_rows
                .entry(record.pid)
                .or_default()
                .entry(record.syscall.clone())
                .and_modify(|calls| *calls += 1)
                .or_insert(1);

            by_syscall
                .entry(record.syscall.clone())
                .or_insert_with(|| SyscallSummaryRow::new(record.syscall.clone()))
                .add(record.errored, record.elapsed_ns);
        }

        let mut by_process = process_rows
            .into_iter()
            .map(|(pid, rows)| {
                let program = process_programs
                    .get(&pid)
                    .cloned()
                    .unwrap_or_else(|| format!("pid:{pid}"));
                SyscallProcessSummary::new(pid, program, finish_syscall_count_rows(rows))
            })
            .collect::<Vec<_>>();

        by_process.sort_unstable_by(|left, right| {
            right
                .calls
                .cmp(&left.calls)
                .then_with(|| left.pid.cmp(&right.pid))
                .then_with(|| left.program.cmp(&right.program))
        });

        let mut program_rows = BTreeMap::<String, BTreeMap<String, u64>>::new();
        for process in &by_process {
            let rows = program_rows.entry(process.program.clone()).or_default();
            for syscall in &process.syscalls {
                rows.entry(syscall.syscall.clone())
                    .and_modify(|calls| *calls += syscall.calls)
                    .or_insert(syscall.calls);
            }
        }

        let mut by_program = program_rows
            .into_iter()
            .map(|(program, rows)| {
                SyscallProgramSummary::new(program, finish_syscall_count_rows(rows))
            })
            .collect::<Vec<_>>();
        by_program.sort_unstable_by(|left, right| {
            right
                .calls
                .cmp(&left.calls)
                .then_with(|| left.program.cmp(&right.program))
        });

        Self {
            syscall_count: records.len() as u64,
            elapsed_ns,
            wall_time_ns: None,
            by_syscall: finish_syscall_summary_rows(by_syscall),
            by_program,
            by_process,
            by_operation: Vec::new(),
        }
    }

    /// Correlate syscall records with XSH trace events from `trace_jsonl`.
    /// Populates `by_operation` in-place. Only processes records that have a
    /// non-zero `start_time_us` and only considers trace events that have both
    /// `start_time_us` and `duration_us` fields.
    pub fn attribute_operations(&mut self, records: &[SyscallTraceRecord], trace_jsonl: &str) {
        use std::collections::BTreeMap as Map;

        // Parse trace events from JSONL; skip malformed lines silently.
        struct OpWindow {
            kind: String,
            name: String,
            start_us: u64,
            end_us: u64,
        }

        let mut windows: Vec<OpWindow> = Vec::new();
        for line in trace_jsonl.lines() {
            let Ok(value) = crate::modules::json::parse_raw_json(line) else {
                continue;
            };
            let kind_value = crate::modules::json::raw_json_get(&value, "kind");
            let kind = match kind_value.and_then(crate::modules::json::raw_json_as_str) {
                Some(kind)
                    if matches!(
                        kind,
                        "module.result"
                            | "method.result"
                            | "run.end"
                            | "proc.exit"
                            | "pure.exit"
                            | "core.result"
                    ) =>
                {
                    kind.trim_end_matches(".result")
                        .trim_end_matches(".exit")
                        .trim_end_matches(".end")
                        .to_string()
                }
                _ => continue,
            };
            let start_us = match crate::modules::json::raw_json_get(&value, "start_time_us")
                .and_then(crate::modules::json::raw_json_as_u64)
            {
                Some(v) if v > 0 => v,
                _ => continue,
            };
            let duration_us = match crate::modules::json::raw_json_get(&value, "duration_us")
                .and_then(crate::modules::json::raw_json_as_u64)
            {
                Some(v) => v,
                _ => continue,
            };
            let name = crate::modules::json::raw_json_get(&value, "name")
                .and_then(crate::modules::json::raw_json_as_str)
                .or_else(|| {
                    crate::modules::json::raw_json_get(&value, "payload")
                        .and_then(|payload| crate::modules::json::raw_json_get(payload, "target"))
                        .and_then(|target| crate::modules::json::raw_json_get(target, "display"))
                        .and_then(crate::modules::json::raw_json_as_str)
                })
                .unwrap_or("?")
                .to_string();
            windows.push(OpWindow {
                kind,
                name,
                start_us,
                end_us: start_us.saturating_add(duration_us),
            });
        }

        if windows.is_empty() {
            return;
        }

        // For each window, sum elapsed_ns of syscall records that fall inside it.
        // Records with start_time_us == 0 are unattributed (skip).
        #[allow(clippy::type_complexity)]
        let mut op_rows: Map<(String, String), (u64, u64, Map<String, u64>)> = Map::new();
        for record in records {
            if record.start_time_us == 0 {
                continue;
            }
            for window in &windows {
                if record.start_time_us >= window.start_us && record.start_time_us < window.end_us {
                    let entry = op_rows
                        .entry((window.kind.clone(), window.name.clone()))
                        .or_default();
                    entry.0 += record.elapsed_ns;
                    entry.1 += 1;
                    *entry.2.entry(record.syscall.clone()).or_default() += 1;
                }
            }
        }

        let mut by_operation: Vec<SyscallOperationSummary> = op_rows
            .into_iter()
            .map(
                |((kind, name), (elapsed_ns, syscall_count, by_sc))| SyscallOperationSummary {
                    kind,
                    name,
                    elapsed_ns,
                    syscall_count,
                    by_syscall: finish_syscall_count_rows(by_sc),
                },
            )
            .collect();
        by_operation.sort_unstable_by_key(|row| std::cmp::Reverse(row.elapsed_ns));
        self.by_operation = by_operation;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyscallSummaryRow {
    pub syscall: String,
    pub calls: u64,
    pub errors: u64,
    pub elapsed_ns: u128,
}

impl SyscallSummaryRow {
    fn new(syscall: String) -> Self {
        Self {
            syscall,
            calls: 0,
            errors: 0,
            elapsed_ns: 0,
        }
    }

    fn add(&mut self, errored: bool, elapsed_ns: u64) {
        self.calls += 1;
        self.errors += u64::from(errored);
        self.elapsed_ns += u128::from(elapsed_ns);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyscallProgramSummary {
    pub program: String,
    pub calls: u64,
    pub syscalls: Vec<SyscallCountRow>,
}

impl SyscallProgramSummary {
    fn new(program: String, syscalls: Vec<SyscallCountRow>) -> Self {
        let calls = syscalls.iter().map(|row| row.calls).sum();
        Self {
            program,
            calls,
            syscalls,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyscallProcessSummary {
    pub pid: u32,
    pub program: String,
    pub calls: u64,
    pub syscalls: Vec<SyscallCountRow>,
}

impl SyscallProcessSummary {
    fn new(pid: u32, program: String, syscalls: Vec<SyscallCountRow>) -> Self {
        let calls = syscalls.iter().map(|row| row.calls).sum();
        Self {
            pid,
            program,
            calls,
            syscalls,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyscallCountRow {
    pub syscall: String,
    pub calls: u64,
}

#[derive(Clone, Debug)]
#[cfg(feature = "tools")]
pub struct SyscallSummaryRenderer {
    top: usize,
}

#[cfg(feature = "tools")]
impl SyscallSummaryRenderer {
    pub fn new(top: usize) -> Self {
        Self { top }
    }

    pub fn render_text(&self, summary: &SyscallSummary) -> String {
        let mut output = String::new();
        output.push_str("syscall summary\n");
        let _ = writeln!(output, "syscall_count={}", summary.syscall_count);
        let _ = writeln!(
            output,
            "syscall_seconds={}",
            format_syscall_seconds(summary.elapsed_ns)
        );
        if let Some(wall_ns) = summary.wall_time_ns {
            let _ = writeln!(
                output,
                "wall_seconds={}",
                format_syscall_seconds(u128::from(wall_ns))
            );
        }
        output.push_str("top_syscalls_by_count:\n");
        for row in summary.by_syscall.iter().take(self.top) {
            let _ = writeln!(
                output,
                "  {} calls={} errors={} seconds={} usecs/call={}",
                row.syscall,
                row.calls,
                row.errors,
                format_syscall_seconds(row.elapsed_ns),
                format_syscall_usecs_per_call(row.elapsed_ns, row.calls)
            );
        }

        output.push_str("per_program_top_syscalls:\n");
        for program in &summary.by_program {
            let _ = writeln!(
                output,
                "  program={} calls={}",
                program.program, program.calls
            );
            for row in program.syscalls.iter().take(self.top) {
                let _ = writeln!(output, "    {} calls={}", row.syscall, row.calls);
            }
        }

        output.push_str("per_process_top_syscalls:\n");
        for process in summary.by_process.iter().take(self.top) {
            let _ = writeln!(
                output,
                "  pid={} program={} calls={}",
                process.pid, process.program, process.calls
            );
            for row in process.syscalls.iter().take(self.top) {
                let _ = writeln!(output, "    {} calls={}", row.syscall, row.calls);
            }
        }

        if !summary.by_operation.is_empty() {
            output.push_str("per_operation_syscalls:\n");
            for op in summary.by_operation.iter().take(self.top) {
                let _ = writeln!(
                    output,
                    "  {}.{} calls={} seconds={}",
                    op.kind,
                    op.name,
                    op.syscall_count,
                    format_syscall_seconds(u128::from(op.elapsed_ns))
                );
                for row in op.by_syscall.iter().take(self.top) {
                    let _ = writeln!(output, "    {} calls={}", row.syscall, row.calls);
                }
            }
        }
        output
    }

    pub fn render_jsonl(&self, summary: &SyscallSummary) -> String {
        let dto = SyscallSummaryJson::from_summary(summary, self.top);
        let mut output = crate::modules::json::compact_raw_json(&syscall_summary_json_value(dto));
        output.push('\n');
        output
    }
}

#[derive(Clone, Debug, Default)]
#[cfg(feature = "tools")]
pub struct TraceTextRenderer;

#[cfg(feature = "tools")]
impl TraceTextRenderer {
    pub fn new() -> Self {
        Self
    }

    pub fn render_events(&self, events: &[TraceEvent], sources: &SourceMap) -> String {
        let mut output = String::new();
        for event in events {
            self.render_event(event, sources, &mut output);
            output.push('\n');
        }
        output
    }

    fn render_event(&self, event: &TraceEvent, sources: &SourceMap, output: &mut String) {
        let _ = write!(
            output,
            "id={} parent={} depth={} kind={}",
            event.event_id,
            event
                .parent_event_id
                .map_or_else(|| "-".to_string(), |id| id.to_string()),
            event.depth,
            event.kind.as_str()
        );

        if let Some(name) = &event.name {
            let _ = write!(output, " name={}", quote_string_text(name));
        }

        if let Some(api_id) = &event.api_id {
            let _ = write!(output, " api_id={}", quote_string_text(api_id));
        }

        if let Some(span) = event.source_span {
            output.push_str(" span=");
            render_span_text(span, sources, output);
        }

        if let Some(start_time_us) = event.timing.start_time_us {
            let _ = write!(output, " start_us={start_time_us}");
        }

        if let Some(duration_us) = event.timing.duration_us {
            let _ = write!(output, " duration_us={duration_us}");
        }

        render_payload_text(&event.payload, output);
    }
}

#[derive(Clone, Debug, Default)]
#[cfg(feature = "tools")]
pub struct TraceJsonlRenderer;

#[cfg(feature = "tools")]
impl TraceJsonlRenderer {
    pub fn new() -> Self {
        Self
    }

    pub fn render_events(&self, events: &[TraceEvent], sources: &SourceMap) -> String {
        let mut output = String::new();
        for event in events {
            self.render_event(event, sources, &mut output);
            output.push('\n');
        }
        output
    }

    fn render_event(&self, event: &TraceEvent, sources: &SourceMap, output: &mut String) {
        let dto = TraceEventJson::from_event(event, sources);
        output.push_str(&crate::modules::json::compact_raw_json(
            &trace_event_json_value(dto),
        ));
    }
}

#[derive(Clone, Debug, Default)]
#[cfg(feature = "tools")]
pub struct TraceSummaryRenderer;

#[cfg(feature = "tools")]
impl TraceSummaryRenderer {
    pub fn new() -> Self {
        Self
    }

    pub fn render_events(&self, events: &[TraceEvent], sources: &SourceMap) -> String {
        self.render_events_with_width(events, sources, trace_summary_terminal_width())
    }

    pub fn render_events_with_width(
        &self,
        events: &[TraceEvent],
        sources: &SourceMap,
        terminal_width: usize,
    ) -> String {
        let summary = TraceSummary::from_events(events);
        let mut output = String::new();
        let terminal_width = terminal_width.max(MIN_TRACE_SUMMARY_WIDTH);
        output.push_str("trace summary\n");
        render_overview_table(&summary, terminal_width, &mut output);
        output.push('\n');
        render_summary_table(
            "function calls (duration µs)",
            &summary.functions,
            None,
            sources,
            terminal_width,
            &mut output,
        );
        output.push('\n');
        render_summary_table(
            "hot commands (top 10 by total ms)",
            &summary.operations,
            Some(10),
            sources,
            terminal_width,
            &mut output,
        );
        output
    }

    pub fn render_jsonl(&self, events: &[TraceEvent], sources: &SourceMap) -> String {
        let summary = TraceSummary::from_events(events);
        let dto = TraceSummaryJson::from_summary(&summary, sources);
        let mut output = crate::modules::json::compact_raw_json(&trace_summary_json_value(dto));
        output.push('\n');
        output
    }
}

#[cfg(feature = "tools")]
struct TraceEventJson {
    event_id: u64,
    parent_event_id: Option<u64>,
    depth: u32,
    kind: &'static str,
    source_span: Option<TraceSpanJson>,
    name: Option<String>,
    api_id: Option<String>,
    start_time_us: Option<u64>,
    duration_us: Option<u64>,
    payload: TracePayloadJson,
}

#[cfg(feature = "tools")]
impl TraceEventJson {
    fn from_event(event: &TraceEvent, sources: &SourceMap) -> Self {
        Self {
            event_id: event.event_id,
            parent_event_id: event.parent_event_id,
            depth: event.depth,
            kind: event.kind.as_str(),
            source_span: TraceSpanJson::from_span(event.source_span, sources),
            name: event.name.clone(),
            api_id: event.api_id.clone(),
            start_time_us: event.timing.start_time_us,
            duration_us: event.timing.duration_us,
            payload: TracePayloadJson::from_payload(&event.payload),
        }
    }
}

#[cfg(feature = "tools")]
struct TraceSpanJson {
    file: String,
    start_offset: usize,
    end_offset: usize,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

#[cfg(feature = "tools")]
impl TraceSpanJson {
    fn from_span(span: Option<Span>, sources: &SourceMap) -> Option<Self> {
        let span = span?;
        let start = sources.location(span.source_id, span.start())?;
        let end = sources.location(span.source_id, span.end())?;
        Some(Self {
            file: start.file,
            start_offset: span.start(),
            end_offset: span.end(),
            start_line: start.line,
            start_column: start.column,
            end_line: end.line,
            end_column: end.column,
        })
    }
}

#[cfg(feature = "tools")]
enum TracePayloadJson {
    None,
    Core {
        argv: Vec<TraceArgJson>,
    },
    RunStart {
        target: TraceArgJson,
        argv: Vec<TraceArgJson>,
        cwd: TraceArgJson,
        env: Vec<TraceEnvJson>,
    },
    RunEnd {
        pid: Option<u32>,
        status: Option<TraceStatusJson>,
        error: Option<TraceErrorJson>,
    },
    SpawnStart {
        handle_id: Option<u64>,
        target: TraceArgJson,
        argv: Vec<TraceArgJson>,
        cwd: TraceArgJson,
        env: Vec<TraceEnvJson>,
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
        status: Option<TraceStatusJson>,
        error: Option<TraceErrorJson>,
    },
    SpawnCancel {
        handle_id: u64,
        pid: Option<u32>,
        signal: String,
        kill_after_ms: u64,
        error: Option<TraceErrorJson>,
    },
    PipelineEnd {
        status: Option<TraceStatusJson>,
        error: Option<TraceErrorJson>,
    },
    PipelineSegmentStart {
        index: usize,
        target: TraceArgJson,
        argv: Vec<TraceArgJson>,
        cwd: TraceArgJson,
        env: Vec<TraceEnvJson>,
    },
    PipelineSegmentEnd {
        index: usize,
        pid: Option<u32>,
        status: Option<TraceStatusJson>,
        error: Option<TraceErrorJson>,
    },
    Redirection {
        op: String,
        target: Option<TraceArgJson>,
        fd: Option<i32>,
        error: Option<TraceErrorJson>,
    },
    StreamStage {
        stage: String,
        item_count: Option<usize>,
        error: Option<TraceErrorJson>,
    },
    StreamItem {
        stage: String,
        item_index: usize,
        error: Option<TraceErrorJson>,
    },
    ParallelJob {
        stage: String,
        item_index: usize,
        error: Option<TraceErrorJson>,
    },
    RetryAttempt {
        attempt: usize,
        max_attempts: usize,
        next_delay_ms: Option<u64>,
        error: Option<TraceErrorJson>,
    },
    Cwd {
        previous: TraceArgJson,
        current: TraceArgJson,
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
        hook_error: Option<TraceErrorJson>,
    },
    ResultPropagate {
        error_kind: String,
    },
    RuntimeError {
        error: TraceErrorJson,
    },
}

#[cfg(feature = "tools")]
impl TracePayloadJson {
    fn from_payload(payload: &TracePayload) -> Self {
        match payload {
            TracePayload::None => Self::None,
            TracePayload::Core { argv } => Self::Core {
                argv: args_json(argv),
            },
            TracePayload::RunStart {
                target,
                argv,
                cwd,
                env,
            } => Self::RunStart {
                target: TraceArgJson::from_arg(target),
                argv: args_json(argv),
                cwd: TraceArgJson::from_arg(cwd),
                env: env_json(env),
            },
            TracePayload::RunEnd { pid, status, error } => Self::RunEnd {
                pid: *pid,
                status: status.as_ref().map(TraceStatusJson::from_status),
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::SpawnStart {
                handle_id,
                target,
                argv,
                cwd,
                env,
                detached,
            } => Self::SpawnStart {
                handle_id: *handle_id,
                target: TraceArgJson::from_arg(target),
                argv: args_json(argv),
                cwd: TraceArgJson::from_arg(cwd),
                env: env_json(env),
                detached: *detached,
            },
            TracePayload::SpawnReady { handle_id, pid } => Self::SpawnReady {
                handle_id: *handle_id,
                pid: *pid,
            },
            TracePayload::WaitStart { handle_ids } => Self::WaitStart {
                handle_ids: handle_ids.clone(),
            },
            TracePayload::WaitEnd {
                handle_id,
                pid,
                status,
                error,
            } => Self::WaitEnd {
                handle_id: *handle_id,
                pid: *pid,
                status: status.as_ref().map(TraceStatusJson::from_status),
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::SpawnCancel {
                handle_id,
                pid,
                signal,
                kill_after_ms,
                error,
            } => Self::SpawnCancel {
                handle_id: *handle_id,
                pid: *pid,
                signal: signal.clone(),
                kill_after_ms: *kill_after_ms,
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::PipelineEnd { status, error } => Self::PipelineEnd {
                status: status.as_ref().map(TraceStatusJson::from_status),
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::PipelineSegmentStart {
                index,
                target,
                argv,
                cwd,
                env,
            } => Self::PipelineSegmentStart {
                index: *index,
                target: TraceArgJson::from_arg(target),
                argv: args_json(argv),
                cwd: TraceArgJson::from_arg(cwd),
                env: env_json(env),
            },
            TracePayload::PipelineSegmentEnd {
                index,
                pid,
                status,
                error,
            } => Self::PipelineSegmentEnd {
                index: *index,
                pid: *pid,
                status: status.as_ref().map(TraceStatusJson::from_status),
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::Redirection {
                op,
                target,
                fd,
                error,
            } => Self::Redirection {
                op: op.clone(),
                target: target.as_ref().map(TraceArgJson::from_arg),
                fd: *fd,
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::StreamStage {
                stage,
                item_count,
                error,
            } => Self::StreamStage {
                stage: stage.clone(),
                item_count: *item_count,
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::StreamItem {
                stage,
                item_index,
                error,
            } => Self::StreamItem {
                stage: stage.clone(),
                item_index: *item_index,
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::ParallelJob {
                stage,
                item_index,
                error,
            } => Self::ParallelJob {
                stage: stage.clone(),
                item_index: *item_index,
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::RetryAttempt {
                attempt,
                max_attempts,
                next_delay_ms,
                error,
            } => Self::RetryAttempt {
                attempt: *attempt,
                max_attempts: *max_attempts,
                next_delay_ms: *next_delay_ms,
                error: error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::Cwd { previous, current } => Self::Cwd {
                previous: TraceArgJson::from_arg(previous),
                current: TraceArgJson::from_arg(current),
            },
            TracePayload::Signal {
                signal_name,
                signal_number,
                phase,
                matching_hook,
                forwarded,
                pre_cancel_ms,
                escalation_signal_name,
                escalation_signal_number,
                hook_error,
            } => Self::Signal {
                signal_name: signal_name.clone(),
                signal_number: *signal_number,
                phase: phase.clone(),
                matching_hook: *matching_hook,
                forwarded: *forwarded,
                pre_cancel_ms: *pre_cancel_ms,
                escalation_signal_name: escalation_signal_name.clone(),
                escalation_signal_number: *escalation_signal_number,
                hook_error: hook_error.as_ref().map(TraceErrorJson::from_error),
            },
            TracePayload::ResultPropagate { error_kind } => Self::ResultPropagate {
                error_kind: error_kind.clone(),
            },
            TracePayload::RuntimeError { error } => Self::RuntimeError {
                error: TraceErrorJson::from_error(error),
            },
        }
    }
}

#[cfg(feature = "tools")]
struct TraceArgJson {
    hex: String,
    display: String,
}

#[cfg(feature = "tools")]
impl TraceArgJson {
    fn from_arg(arg: &TraceArg) -> Self {
        Self {
            hex: hex_string(&arg.bytes),
            display: String::from_utf8_lossy(&arg.bytes).into_owned(),
        }
    }
}

#[cfg(feature = "tools")]
struct TraceEnvJson {
    name: TraceArgJson,
    value: TraceArgJson,
}

#[cfg(feature = "tools")]
struct TraceStatusJson {
    success: bool,
    kind: &'static str,
    code: Option<i32>,
}

#[cfg(feature = "tools")]
impl TraceStatusJson {
    fn from_status(status: &TraceStatus) -> Self {
        Self {
            success: status.success,
            kind: status.kind.as_str(),
            code: status.code,
        }
    }
}

#[cfg(feature = "tools")]
struct TraceErrorJson {
    kind: String,
    message: String,
}

#[cfg(feature = "tools")]
impl TraceErrorJson {
    fn from_error(error: &TraceError) -> Self {
        Self {
            kind: error.kind.clone(),
            message: error.message.clone(),
        }
    }
}

#[cfg(feature = "tools")]
struct TraceSummaryJson {
    ty: &'static str,
    event_count: usize,
    timed_event_count: usize,
    script_duration_us: Option<u64>,
    function_calls: Vec<SummaryRowJson>,
    hot_commands: Vec<SummaryRowJson>,
}

#[cfg(feature = "tools")]
impl TraceSummaryJson {
    fn from_summary(summary: &TraceSummary, sources: &SourceMap) -> Self {
        Self {
            ty: "trace.summary",
            event_count: summary.event_count,
            timed_event_count: summary.timed_event_count,
            script_duration_us: summary.script_duration_us,
            function_calls: summary_rows_json(&summary.functions, None, sources),
            hot_commands: summary_rows_json(&summary.operations, Some(10), sources),
        }
    }
}

#[cfg(feature = "tools")]
struct SummaryRowJson {
    kind: String,
    name: String,
    count: u64,
    total_us: u64,
    avg_us: f64,
    p50_us: u64,
    p75_us: u64,
    p90_us: u64,
    p99_us: u64,
    max_us: u64,
    slowest_span: Option<TraceSpanJson>,
}

#[cfg(feature = "tools")]
struct SyscallSummaryJson {
    ty: &'static str,
    syscall_count: u64,
    syscall_seconds: f64,
    wall_seconds: Option<f64>,
    top_syscalls_by_count: Vec<SyscallSummaryRowJson>,
    per_program_top_syscalls: Vec<SyscallProgramSummaryJson>,
    per_process_top_syscalls: Vec<SyscallProcessSummaryJson>,
    per_operation_syscalls: Vec<SyscallOperationSummaryJson>,
}

#[cfg(feature = "tools")]
impl SyscallSummaryJson {
    fn from_summary(summary: &SyscallSummary, top: usize) -> Self {
        Self {
            ty: "syscall.summary",
            syscall_count: summary.syscall_count,
            syscall_seconds: syscall_seconds_value(summary.elapsed_ns),
            wall_seconds: summary
                .wall_time_ns
                .map(|ns| syscall_seconds_value(u128::from(ns))),
            top_syscalls_by_count: summary
                .by_syscall
                .iter()
                .take(top)
                .map(SyscallSummaryRowJson::from_row)
                .collect(),
            per_program_top_syscalls: summary
                .by_program
                .iter()
                .map(|program| SyscallProgramSummaryJson::from_summary(program, top))
                .collect(),
            per_process_top_syscalls: summary
                .by_process
                .iter()
                .take(top)
                .map(|process| SyscallProcessSummaryJson::from_summary(process, top))
                .collect(),
            per_operation_syscalls: summary
                .by_operation
                .iter()
                .take(top)
                .map(|op| SyscallOperationSummaryJson::from_op(op, top))
                .collect(),
        }
    }
}

#[cfg(feature = "tools")]
struct SyscallOperationSummaryJson {
    kind: String,
    name: String,
    syscall_count: u64,
    seconds: f64,
    syscalls: Vec<SyscallCountRowJson>,
}

#[cfg(feature = "tools")]
impl SyscallOperationSummaryJson {
    fn from_op(op: &SyscallOperationSummary, top: usize) -> Self {
        Self {
            kind: op.kind.clone(),
            name: op.name.clone(),
            syscall_count: op.syscall_count,
            seconds: syscall_seconds_value(u128::from(op.elapsed_ns)),
            syscalls: op
                .by_syscall
                .iter()
                .take(top)
                .map(SyscallCountRowJson::from_row)
                .collect(),
        }
    }
}

#[cfg(feature = "tools")]
struct SyscallSummaryRowJson {
    syscall: String,
    calls: u64,
    errors: u64,
    seconds: f64,
    usecs_per_call: u64,
}

#[cfg(feature = "tools")]
impl SyscallSummaryRowJson {
    fn from_row(row: &SyscallSummaryRow) -> Self {
        Self {
            syscall: row.syscall.clone(),
            calls: row.calls,
            errors: row.errors,
            seconds: syscall_seconds_value(row.elapsed_ns),
            usecs_per_call: syscall_usecs_per_call(row.elapsed_ns, row.calls),
        }
    }
}

#[cfg(feature = "tools")]
struct SyscallProgramSummaryJson {
    program: String,
    calls: u64,
    syscalls: Vec<SyscallCountRowJson>,
}

#[cfg(feature = "tools")]
impl SyscallProgramSummaryJson {
    fn from_summary(summary: &SyscallProgramSummary, top: usize) -> Self {
        Self {
            program: summary.program.clone(),
            calls: summary.calls,
            syscalls: summary
                .syscalls
                .iter()
                .take(top)
                .map(SyscallCountRowJson::from_row)
                .collect(),
        }
    }
}

#[cfg(feature = "tools")]
struct SyscallProcessSummaryJson {
    pid: u32,
    program: String,
    calls: u64,
    syscalls: Vec<SyscallCountRowJson>,
}

#[cfg(feature = "tools")]
impl SyscallProcessSummaryJson {
    fn from_summary(summary: &SyscallProcessSummary, top: usize) -> Self {
        Self {
            pid: summary.pid,
            program: summary.program.clone(),
            calls: summary.calls,
            syscalls: summary
                .syscalls
                .iter()
                .take(top)
                .map(SyscallCountRowJson::from_row)
                .collect(),
        }
    }
}

#[cfg(feature = "tools")]
struct SyscallCountRowJson {
    syscall: String,
    calls: u64,
}

#[cfg(feature = "tools")]
impl SyscallCountRowJson {
    fn from_row(row: &SyscallCountRow) -> Self {
        Self {
            syscall: row.syscall.clone(),
            calls: row.calls,
        }
    }
}

#[cfg(feature = "tools")]
fn trace_event_json_value(data: TraceEventJson) -> JsonValue {
    raw_json_object([
        ("event_id".to_string(), raw_json_u64(data.event_id)),
        (
            "parent_event_id".to_string(),
            option_u64_json_value(data.parent_event_id),
        ),
        ("depth".to_string(), raw_json_u64(u64::from(data.depth))),
        ("kind".to_string(), raw_json_string(data.kind)),
        (
            "source_span".to_string(),
            option_json_value(data.source_span.map(trace_span_json_value)),
        ),
        ("name".to_string(), option_string_json_value(data.name)),
        ("api_id".to_string(), option_string_json_value(data.api_id)),
        (
            "start_time_us".to_string(),
            option_u64_json_value(data.start_time_us),
        ),
        (
            "duration_us".to_string(),
            option_u64_json_value(data.duration_us),
        ),
        (
            "payload".to_string(),
            trace_payload_json_value(data.payload),
        ),
    ])
}

#[cfg(feature = "tools")]
fn trace_span_json_value(data: TraceSpanJson) -> JsonValue {
    raw_json_object([
        ("file".to_string(), raw_json_string(data.file)),
        (
            "start_offset".to_string(),
            raw_json_usize(data.start_offset),
        ),
        ("end_offset".to_string(), raw_json_usize(data.end_offset)),
        ("start_line".to_string(), raw_json_usize(data.start_line)),
        (
            "start_column".to_string(),
            raw_json_usize(data.start_column),
        ),
        ("end_line".to_string(), raw_json_usize(data.end_line)),
        ("end_column".to_string(), raw_json_usize(data.end_column)),
    ])
}

#[cfg(feature = "tools")]
fn trace_payload_json_value(data: TracePayloadJson) -> JsonValue {
    match data {
        TracePayloadJson::None => typed_payload_json_value("none", vec![]),
        TracePayloadJson::Core { argv } => typed_payload_json_value(
            "core",
            vec![("argv".to_string(), trace_args_json_value(argv))],
        ),
        TracePayloadJson::RunStart {
            target,
            argv,
            cwd,
            env,
        } => typed_payload_json_value(
            "run.start",
            vec![
                ("target".to_string(), trace_arg_json_value(target)),
                ("argv".to_string(), trace_args_json_value(argv)),
                ("cwd".to_string(), trace_arg_json_value(cwd)),
                ("env".to_string(), trace_env_json_value(env)),
            ],
        ),
        TracePayloadJson::RunEnd { pid, status, error } => typed_payload_json_value(
            "run.end",
            vec![
                ("pid".to_string(), option_u32_json_value(pid)),
                (
                    "status".to_string(),
                    option_json_value(status.map(trace_status_json_value)),
                ),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::SpawnStart {
            handle_id,
            target,
            argv,
            cwd,
            env,
            detached,
        } => typed_payload_json_value(
            "spawn.start",
            vec![
                ("handle_id".to_string(), option_u64_json_value(handle_id)),
                ("target".to_string(), trace_arg_json_value(target)),
                ("argv".to_string(), trace_args_json_value(argv)),
                ("cwd".to_string(), trace_arg_json_value(cwd)),
                ("env".to_string(), trace_env_json_value(env)),
                ("detached".to_string(), raw_json_bool(detached)),
            ],
        ),
        TracePayloadJson::SpawnReady { handle_id, pid } => typed_payload_json_value(
            "spawn.ready",
            vec![
                ("handle_id".to_string(), raw_json_u64(handle_id)),
                ("pid".to_string(), option_u32_json_value(pid)),
            ],
        ),
        TracePayloadJson::WaitStart { handle_ids } => typed_payload_json_value(
            "wait.start",
            vec![(
                "handle_ids".to_string(),
                raw_json_array(handle_ids.into_iter().map(raw_json_u64)),
            )],
        ),
        TracePayloadJson::WaitEnd {
            handle_id,
            pid,
            status,
            error,
        } => typed_payload_json_value(
            "wait.end",
            vec![
                ("handle_id".to_string(), option_u64_json_value(handle_id)),
                ("pid".to_string(), option_u32_json_value(pid)),
                (
                    "status".to_string(),
                    option_json_value(status.map(trace_status_json_value)),
                ),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::SpawnCancel {
            handle_id,
            pid,
            signal,
            kill_after_ms,
            error,
        } => typed_payload_json_value(
            "spawn.cancel",
            vec![
                ("handle_id".to_string(), raw_json_u64(handle_id)),
                ("pid".to_string(), option_u32_json_value(pid)),
                ("signal".to_string(), raw_json_string(signal)),
                ("kill_after_ms".to_string(), raw_json_u64(kill_after_ms)),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::PipelineEnd { status, error } => typed_payload_json_value(
            "pipeline.end",
            vec![
                (
                    "status".to_string(),
                    option_json_value(status.map(trace_status_json_value)),
                ),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::PipelineSegmentStart {
            index,
            target,
            argv,
            cwd,
            env,
        } => typed_payload_json_value(
            "pipeline.segment.start",
            vec![
                ("index".to_string(), raw_json_usize(index)),
                ("target".to_string(), trace_arg_json_value(target)),
                ("argv".to_string(), trace_args_json_value(argv)),
                ("cwd".to_string(), trace_arg_json_value(cwd)),
                ("env".to_string(), trace_env_json_value(env)),
            ],
        ),
        TracePayloadJson::PipelineSegmentEnd {
            index,
            pid,
            status,
            error,
        } => typed_payload_json_value(
            "pipeline.segment.end",
            vec![
                ("index".to_string(), raw_json_usize(index)),
                ("pid".to_string(), option_u32_json_value(pid)),
                (
                    "status".to_string(),
                    option_json_value(status.map(trace_status_json_value)),
                ),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::Redirection {
            op,
            target,
            fd,
            error,
        } => typed_payload_json_value(
            "redirection",
            vec![
                ("op".to_string(), raw_json_string(op)),
                (
                    "target".to_string(),
                    option_json_value(target.map(trace_arg_json_value)),
                ),
                ("fd".to_string(), option_i32_json_value(fd)),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::StreamStage {
            stage,
            item_count,
            error,
        } => typed_payload_json_value(
            "stream.stage",
            vec![
                ("stage".to_string(), raw_json_string(stage)),
                (
                    "item_count".to_string(),
                    option_usize_json_value(item_count),
                ),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::StreamItem {
            stage,
            item_index,
            error,
        } => typed_payload_json_value(
            "stream.item",
            vec![
                ("stage".to_string(), raw_json_string(stage)),
                ("item_index".to_string(), raw_json_usize(item_index)),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::ParallelJob {
            stage,
            item_index,
            error,
        } => typed_payload_json_value(
            "parallel.job",
            vec![
                ("stage".to_string(), raw_json_string(stage)),
                ("item_index".to_string(), raw_json_usize(item_index)),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::RetryAttempt {
            attempt,
            max_attempts,
            next_delay_ms,
            error,
        } => typed_payload_json_value(
            "retry.attempt",
            vec![
                ("attempt".to_string(), raw_json_usize(attempt)),
                ("max_attempts".to_string(), raw_json_usize(max_attempts)),
                (
                    "next_delay_ms".to_string(),
                    option_u64_json_value(next_delay_ms),
                ),
                (
                    "error".to_string(),
                    option_json_value(error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::Cwd { previous, current } => typed_payload_json_value(
            "cwd",
            vec![
                ("previous".to_string(), trace_arg_json_value(previous)),
                ("current".to_string(), trace_arg_json_value(current)),
            ],
        ),
        TracePayloadJson::Signal {
            signal_name,
            signal_number,
            phase,
            matching_hook,
            forwarded,
            pre_cancel_ms,
            escalation_signal_name,
            escalation_signal_number,
            hook_error,
        } => typed_payload_json_value(
            "signal",
            vec![
                ("signal_name".to_string(), raw_json_string(signal_name)),
                (
                    "signal_number".to_string(),
                    raw_json_i64(i64::from(signal_number)),
                ),
                ("phase".to_string(), raw_json_string(phase)),
                ("matching_hook".to_string(), raw_json_bool(matching_hook)),
                ("forwarded".to_string(), raw_json_bool(forwarded)),
                (
                    "pre_cancel_ms".to_string(),
                    option_u64_json_value(pre_cancel_ms),
                ),
                (
                    "escalation_signal_name".to_string(),
                    option_string_json_value(escalation_signal_name),
                ),
                (
                    "escalation_signal_number".to_string(),
                    option_i32_json_value(escalation_signal_number),
                ),
                (
                    "hook_error".to_string(),
                    option_json_value(hook_error.map(trace_error_json_value)),
                ),
            ],
        ),
        TracePayloadJson::ResultPropagate { error_kind } => typed_payload_json_value(
            "result.propagate",
            vec![("error_kind".to_string(), raw_json_string(error_kind))],
        ),
        TracePayloadJson::RuntimeError { error } => typed_payload_json_value(
            "runtime.error",
            vec![("error".to_string(), trace_error_json_value(error))],
        ),
    }
}

#[cfg(feature = "tools")]
fn typed_payload_json_value(ty: &'static str, fields: Vec<(String, JsonValue)>) -> JsonValue {
    let mut entries = Vec::with_capacity(fields.len() + 1);
    entries.push(("type".to_string(), raw_json_string(ty)));
    entries.extend(fields);
    raw_json_object(entries)
}

#[cfg(feature = "tools")]
fn trace_arg_json_value(data: TraceArgJson) -> JsonValue {
    raw_json_object([
        ("hex".to_string(), raw_json_string(data.hex)),
        ("display".to_string(), raw_json_string(data.display)),
    ])
}

#[cfg(feature = "tools")]
fn trace_args_json_value(args: Vec<TraceArgJson>) -> JsonValue {
    raw_json_array(args.into_iter().map(trace_arg_json_value))
}

#[cfg(feature = "tools")]
fn trace_env_json_value(env: Vec<TraceEnvJson>) -> JsonValue {
    raw_json_array(env.into_iter().map(|item| {
        raw_json_object([
            ("name".to_string(), trace_arg_json_value(item.name)),
            ("value".to_string(), trace_arg_json_value(item.value)),
        ])
    }))
}

#[cfg(feature = "tools")]
fn trace_status_json_value(data: TraceStatusJson) -> JsonValue {
    raw_json_object([
        ("success".to_string(), raw_json_bool(data.success)),
        ("kind".to_string(), raw_json_string(data.kind)),
        ("code".to_string(), option_i32_json_value(data.code)),
    ])
}

#[cfg(feature = "tools")]
fn trace_error_json_value(data: TraceErrorJson) -> JsonValue {
    raw_json_object([
        ("kind".to_string(), raw_json_string(data.kind)),
        ("message".to_string(), raw_json_string(data.message)),
    ])
}

#[cfg(feature = "tools")]
fn trace_summary_json_value(data: TraceSummaryJson) -> JsonValue {
    raw_json_object([
        ("type".to_string(), raw_json_string(data.ty)),
        ("event_count".to_string(), raw_json_usize(data.event_count)),
        (
            "timed_event_count".to_string(),
            raw_json_usize(data.timed_event_count),
        ),
        (
            "script_duration_us".to_string(),
            option_u64_json_value(data.script_duration_us),
        ),
        (
            "function_calls".to_string(),
            raw_json_array(data.function_calls.into_iter().map(summary_row_json_value)),
        ),
        (
            "hot_commands".to_string(),
            raw_json_array(data.hot_commands.into_iter().map(summary_row_json_value)),
        ),
    ])
}

#[cfg(feature = "tools")]
fn summary_row_json_value(data: SummaryRowJson) -> JsonValue {
    raw_json_object([
        ("kind".to_string(), raw_json_string(data.kind)),
        ("name".to_string(), raw_json_string(data.name)),
        ("count".to_string(), raw_json_u64(data.count)),
        ("total_us".to_string(), raw_json_u64(data.total_us)),
        ("avg_us".to_string(), raw_json_f64(data.avg_us)),
        ("p50_us".to_string(), raw_json_u64(data.p50_us)),
        ("p75_us".to_string(), raw_json_u64(data.p75_us)),
        ("p90_us".to_string(), raw_json_u64(data.p90_us)),
        ("p99_us".to_string(), raw_json_u64(data.p99_us)),
        ("max_us".to_string(), raw_json_u64(data.max_us)),
        (
            "slowest_span".to_string(),
            option_json_value(data.slowest_span.map(trace_span_json_value)),
        ),
    ])
}

#[cfg(feature = "tools")]
fn syscall_summary_json_value(data: SyscallSummaryJson) -> JsonValue {
    let mut fields = vec![
        ("type".to_string(), raw_json_string(data.ty)),
        (
            "syscall_count".to_string(),
            raw_json_u64(data.syscall_count),
        ),
        (
            "syscall_seconds".to_string(),
            raw_json_f64(data.syscall_seconds),
        ),
        (
            "top_syscalls_by_count".to_string(),
            raw_json_array(
                data.top_syscalls_by_count
                    .into_iter()
                    .map(syscall_summary_row_json_value),
            ),
        ),
        (
            "per_program_top_syscalls".to_string(),
            raw_json_array(
                data.per_program_top_syscalls
                    .into_iter()
                    .map(syscall_program_summary_json_value),
            ),
        ),
        (
            "per_process_top_syscalls".to_string(),
            raw_json_array(
                data.per_process_top_syscalls
                    .into_iter()
                    .map(syscall_process_summary_json_value),
            ),
        ),
    ];
    if let Some(wall_seconds) = data.wall_seconds {
        fields.insert(3, ("wall_seconds".to_string(), raw_json_f64(wall_seconds)));
    }
    if !data.per_operation_syscalls.is_empty() {
        fields.push((
            "per_operation_syscalls".to_string(),
            raw_json_array(
                data.per_operation_syscalls
                    .into_iter()
                    .map(syscall_operation_summary_json_value),
            ),
        ));
    }
    raw_json_object(fields)
}

#[cfg(feature = "tools")]
fn syscall_operation_summary_json_value(data: SyscallOperationSummaryJson) -> JsonValue {
    raw_json_object([
        ("kind".to_string(), raw_json_string(data.kind)),
        ("name".to_string(), raw_json_string(data.name)),
        (
            "syscall_count".to_string(),
            raw_json_u64(data.syscall_count),
        ),
        ("seconds".to_string(), raw_json_f64(data.seconds)),
        (
            "syscalls".to_string(),
            raw_json_array(data.syscalls.into_iter().map(syscall_count_row_json_value)),
        ),
    ])
}

#[cfg(feature = "tools")]
fn syscall_summary_row_json_value(data: SyscallSummaryRowJson) -> JsonValue {
    raw_json_object([
        ("syscall".to_string(), raw_json_string(data.syscall)),
        ("calls".to_string(), raw_json_u64(data.calls)),
        ("errors".to_string(), raw_json_u64(data.errors)),
        ("seconds".to_string(), raw_json_f64(data.seconds)),
        (
            "usecs_per_call".to_string(),
            raw_json_u64(data.usecs_per_call),
        ),
    ])
}

#[cfg(feature = "tools")]
fn syscall_program_summary_json_value(data: SyscallProgramSummaryJson) -> JsonValue {
    raw_json_object([
        ("program".to_string(), raw_json_string(data.program)),
        ("calls".to_string(), raw_json_u64(data.calls)),
        (
            "syscalls".to_string(),
            raw_json_array(data.syscalls.into_iter().map(syscall_count_row_json_value)),
        ),
    ])
}

#[cfg(feature = "tools")]
fn syscall_process_summary_json_value(data: SyscallProcessSummaryJson) -> JsonValue {
    raw_json_object([
        ("pid".to_string(), raw_json_u64(u64::from(data.pid))),
        ("program".to_string(), raw_json_string(data.program)),
        ("calls".to_string(), raw_json_u64(data.calls)),
        (
            "syscalls".to_string(),
            raw_json_array(data.syscalls.into_iter().map(syscall_count_row_json_value)),
        ),
    ])
}

#[cfg(feature = "tools")]
fn syscall_count_row_json_value(data: SyscallCountRowJson) -> JsonValue {
    raw_json_object([
        ("syscall".to_string(), raw_json_string(data.syscall)),
        ("calls".to_string(), raw_json_u64(data.calls)),
    ])
}

#[cfg(feature = "tools")]
fn option_json_value(value: Option<JsonValue>) -> JsonValue {
    value.unwrap_or(JsonValue::Null)
}

#[cfg(feature = "tools")]
fn option_string_json_value(value: Option<String>) -> JsonValue {
    option_json_value(value.map(raw_json_string))
}

#[cfg(feature = "tools")]
fn option_u64_json_value(value: Option<u64>) -> JsonValue {
    option_json_value(value.map(raw_json_u64))
}

#[cfg(feature = "tools")]
fn option_u32_json_value(value: Option<u32>) -> JsonValue {
    option_json_value(value.map(|value| raw_json_u64(u64::from(value))))
}

#[cfg(feature = "tools")]
fn option_usize_json_value(value: Option<usize>) -> JsonValue {
    option_json_value(value.map(raw_json_usize))
}

#[cfg(feature = "tools")]
fn option_i32_json_value(value: Option<i32>) -> JsonValue {
    option_json_value(value.map(|value| raw_json_i64(i64::from(value))))
}

#[cfg(feature = "tools")]
fn args_json(args: &[TraceArg]) -> Vec<TraceArgJson> {
    args.iter().map(TraceArgJson::from_arg).collect()
}

#[cfg(feature = "tools")]
fn env_json(env: &[TraceEnv]) -> Vec<TraceEnvJson> {
    env.iter()
        .map(|item| TraceEnvJson {
            name: TraceArgJson::from_arg(&item.name),
            value: TraceArgJson::from_arg(&item.value),
        })
        .collect()
}

#[cfg(feature = "tools")]
fn summary_rows_json(
    rows: &[SummaryRow],
    limit: Option<usize>,
    sources: &SourceMap,
) -> Vec<SummaryRowJson> {
    rows.iter()
        .take(limit.unwrap_or(rows.len()))
        .map(|row| SummaryRowJson {
            kind: row.kind.clone(),
            name: row.name.clone(),
            count: row.count,
            total_us: row.total_us,
            avg_us: (row.avg_us() * 10.0).round() / 10.0,
            p50_us: row.percentile_us(50),
            p75_us: row.percentile_us(75),
            p90_us: row.percentile_us(90),
            p99_us: row.percentile_us(99),
            max_us: row.max_us,
            slowest_span: TraceSpanJson::from_span(row.slowest_span, sources),
        })
        .collect()
}

fn finish_syscall_summary_rows(
    rows: BTreeMap<String, SyscallSummaryRow>,
) -> Vec<SyscallSummaryRow> {
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_unstable_by(|left, right| {
        right
            .calls
            .cmp(&left.calls)
            .then_with(|| right.elapsed_ns.cmp(&left.elapsed_ns))
            .then_with(|| right.errors.cmp(&left.errors))
            .then_with(|| left.syscall.cmp(&right.syscall))
    });
    rows
}

fn finish_syscall_count_rows(rows: BTreeMap<String, u64>) -> Vec<SyscallCountRow> {
    let mut rows = rows
        .into_iter()
        .map(|(syscall, calls)| SyscallCountRow { syscall, calls })
        .collect::<Vec<_>>();
    rows.sort_unstable_by(|left, right| {
        right
            .calls
            .cmp(&left.calls)
            .then_with(|| left.syscall.cmp(&right.syscall))
    });
    rows
}

#[cfg(feature = "tools")]
fn format_syscall_seconds(elapsed_ns: u128) -> String {
    format!("{:.6}", syscall_seconds_value(elapsed_ns))
}

#[cfg(feature = "tools")]
fn syscall_seconds_value(elapsed_ns: u128) -> f64 {
    (elapsed_ns as f64 / 1_000_000_000.0 * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(feature = "tools")]
fn format_syscall_usecs_per_call(elapsed_ns: u128, calls: u64) -> String {
    syscall_usecs_per_call(elapsed_ns, calls).to_string()
}

#[cfg(feature = "tools")]
fn syscall_usecs_per_call(elapsed_ns: u128, calls: u64) -> u64 {
    if calls == 0 {
        0
    } else {
        (elapsed_ns / u128::from(calls) / 1_000) as u64
    }
}

#[derive(Clone, Debug, Default)]
#[cfg(feature = "tools")]
struct TraceSummary {
    event_count: usize,
    timed_event_count: usize,
    script_duration_us: Option<u64>,
    functions: Vec<SummaryRow>,
    operations: Vec<SummaryRow>,
}

#[cfg(feature = "tools")]
impl TraceSummary {
    fn from_events(events: &[TraceEvent]) -> Self {
        let names_by_id: FxHashMap<u64, String> = events
            .iter()
            .filter_map(|event| {
                event
                    .name
                    .as_ref()
                    .map(|name| (event.event_id, name.clone()))
            })
            .collect();
        let mut functions = BTreeMap::new();
        let mut operations = BTreeMap::new();
        let mut timed_event_count = 0;
        let mut script_duration_us = None;

        for event in events {
            let Some(duration_us) = event.timing.duration_us else {
                continue;
            };
            timed_event_count += 1;
            match event.kind {
                TraceKind::ScriptExit => script_duration_us = Some(duration_us),
                TraceKind::ProcExit => add_summary_row(
                    &mut functions,
                    "proc",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                TraceKind::PureExit => add_summary_row(
                    &mut functions,
                    "pure",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                TraceKind::CoreResult => add_summary_row(
                    &mut operations,
                    "core",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                TraceKind::ModuleResult => add_summary_row(
                    &mut operations,
                    "module",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                TraceKind::MethodResult => add_summary_row(
                    &mut operations,
                    "method",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                TraceKind::RunEnd => add_summary_row(
                    &mut operations,
                    "run",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                TraceKind::PipelineExit => add_summary_row(
                    &mut operations,
                    "pipeline",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                TraceKind::StreamStageExit => add_summary_row(
                    &mut operations,
                    "stream-stage",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                TraceKind::CwdExit => add_summary_row(
                    &mut operations,
                    "cwd",
                    summary_event_name(event, &names_by_id),
                    duration_us,
                    event.source_span,
                ),
                _ => {}
            }
        }

        Self {
            event_count: events.len(),
            timed_event_count,
            script_duration_us,
            functions: finish_summary_rows(functions),
            operations: finish_summary_rows(operations),
        }
    }
}

#[derive(Clone, Debug)]
#[cfg(feature = "tools")]
struct SummaryRow {
    kind: String,
    name: String,
    count: u64,
    total_us: u64,
    durations_us: Vec<u64>,
    max_us: u64,
    slowest_span: Option<Span>,
}

#[cfg(feature = "tools")]
const DEFAULT_TRACE_SUMMARY_WIDTH: usize = 120;
#[cfg(feature = "tools")]
const MIN_TRACE_SUMMARY_WIDTH: usize = 60;

#[derive(Clone, Copy, Debug)]
#[cfg(feature = "tools")]
enum SummaryField {
    Kind,
    Name,
    Calls,
    Total,
    P50,
    P75,
    P90,
    P99,
    Slowest,
    Max,
    Avg,
}

#[derive(Clone, Debug)]
#[cfg(feature = "tools")]
struct SummaryTableColumn {
    field: SummaryField,
    table: TextTableColumn,
}

#[cfg(feature = "tools")]
impl SummaryRow {
    fn new(kind: &str, name: String) -> Self {
        Self {
            kind: kind.to_string(),
            name,
            count: 0,
            total_us: 0,
            durations_us: Vec::new(),
            max_us: 0,
            slowest_span: None,
        }
    }

    fn add(&mut self, duration_us: u64, span: Option<Span>) {
        self.count += 1;
        self.total_us += duration_us;
        self.durations_us.push(duration_us);
        if self.count == 1 || duration_us > self.max_us {
            self.max_us = duration_us;
            self.slowest_span = span;
        }
    }

    fn avg_us(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_us as f64 / self.count as f64
        }
    }

    fn percentile_us(&self, percentile: usize) -> u64 {
        if self.durations_us.is_empty() {
            return 0;
        }
        let rank = (percentile * self.durations_us.len()).div_ceil(100);
        self.durations_us[rank.saturating_sub(1)]
    }
}

#[cfg(feature = "tools")]
fn add_summary_row(
    rows: &mut BTreeMap<(String, String), SummaryRow>,
    kind: &str,
    name: String,
    duration_us: u64,
    span: Option<Span>,
) {
    rows.entry((kind.to_string(), name.clone()))
        .or_insert_with(|| SummaryRow::new(kind, name))
        .add(duration_us, span);
}

#[cfg(feature = "tools")]
fn finish_summary_rows(rows: BTreeMap<(String, String), SummaryRow>) -> Vec<SummaryRow> {
    let mut rows: Vec<_> = rows
        .into_values()
        .map(|mut row| {
            row.durations_us.sort_unstable();
            row
        })
        .collect();
    rows.sort_unstable_by(|left, right| {
        right
            .total_us
            .cmp(&left.total_us)
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.name.cmp(&right.name))
    });
    rows
}

#[cfg(feature = "tools")]
fn summary_event_name(event: &TraceEvent, names_by_id: &FxHashMap<u64, String>) -> String {
    if let Some(name) = &event.name {
        return name.clone();
    }
    if let Some(parent_name) = event
        .parent_event_id
        .and_then(|event_id| names_by_id.get(&event_id))
    {
        return parent_name.clone();
    }
    match &event.payload {
        TracePayload::StreamStage { stage, .. } => stage.clone(),
        TracePayload::PipelineSegmentEnd { index, .. } => format!("segment {index}"),
        TracePayload::Redirection { op, .. } => op.clone(),
        TracePayload::RetryAttempt { attempt, .. } => format!("attempt {attempt}"),
        TracePayload::ResultPropagate { error_kind } => error_kind.clone(),
        TracePayload::RuntimeError { error } => error.kind.clone(),
        _ => event.kind.as_str().to_string(),
    }
}

#[cfg(feature = "tools")]
fn render_overview_table(summary: &TraceSummary, terminal_width: usize, output: &mut String) {
    let columns = [
        TextTableColumn::new("metric", 15, 24, TableAlign::Left),
        TextTableColumn::new("value", 7, 16, TableAlign::Right),
    ];
    let rows = vec![
        vec!["events".to_string(), summary.event_count.to_string()],
        vec![
            "timed events".to_string(),
            summary.timed_event_count.to_string(),
        ],
        vec![
            "script duration".to_string(),
            summary
                .script_duration_us
                .map_or_else(|| "-".to_string(), |duration| format!("{duration} µs")),
        ],
    ];
    render_text_table(&columns, &rows, terminal_width, output);
}

#[cfg(feature = "tools")]
fn render_summary_table(
    title: &str,
    rows: &[SummaryRow],
    limit: Option<usize>,
    sources: &SourceMap,
    terminal_width: usize,
    output: &mut String,
) {
    output.push_str(title);
    output.push('\n');
    if rows.is_empty() {
        render_text_table(
            &[TextTableColumn::new("status", 6, 12, TableAlign::Left)],
            &[vec!["none".to_string()]],
            terminal_width,
            output,
        );
        return;
    }

    let columns = select_summary_columns(terminal_width);
    let table_columns: Vec<_> = columns.iter().map(|column| column.table.clone()).collect();
    let table_rows: Vec<_> = rows
        .iter()
        .take(limit.unwrap_or(rows.len()))
        .map(|row| summary_table_row(row, &columns, sources))
        .collect();
    render_text_table(&table_columns, &table_rows, terminal_width, output);
}

#[cfg(feature = "tools")]
fn select_summary_columns(terminal_width: usize) -> Vec<SummaryTableColumn> {
    let mut columns = vec![
        summary_column(SummaryField::Kind, "kind", 4, 12, TableAlign::Left),
        summary_column(SummaryField::Name, "name", 8, 32, TableAlign::Left),
        summary_column(SummaryField::Calls, "calls", 5, 8, TableAlign::Right),
        summary_column(SummaryField::Total, "total", 5, 8, TableAlign::Right),
        summary_column(SummaryField::P50, "p50", 3, 6, TableAlign::Right),
        summary_column(SummaryField::P75, "p75", 3, 6, TableAlign::Right),
        summary_column(SummaryField::P90, "p90", 3, 6, TableAlign::Right),
        summary_column(SummaryField::P99, "p99", 3, 6, TableAlign::Right),
    ];
    for optional in [
        summary_column(
            SummaryField::Slowest,
            "slowest span",
            12,
            48,
            TableAlign::Left,
        ),
        summary_column(SummaryField::Max, "max", 3, 6, TableAlign::Right),
        summary_column(SummaryField::Avg, "avg", 3, 6, TableAlign::Right),
    ] {
        let mut candidate = columns.clone();
        candidate.push(optional);
        let min_widths: Vec<_> = candidate
            .iter()
            .map(|column| column.table.min_width)
            .collect();
        if table_width(&min_widths) <= terminal_width {
            columns = candidate;
        }
    }
    columns
}

#[cfg(feature = "tools")]
fn summary_column(
    field: SummaryField,
    header: &'static str,
    min_width: usize,
    max_width: usize,
    align: TableAlign,
) -> SummaryTableColumn {
    SummaryTableColumn {
        field,
        table: TextTableColumn::new(header, min_width, max_width, align),
    }
}

#[cfg(feature = "tools")]
fn summary_table_row(
    row: &SummaryRow,
    columns: &[SummaryTableColumn],
    sources: &SourceMap,
) -> Vec<String> {
    columns
        .iter()
        .map(|column| match column.field {
            SummaryField::Kind => row.kind.clone(),
            SummaryField::Name => row.name.clone(),
            SummaryField::Calls => row.count.to_string(),
            SummaryField::Total => row.total_us.to_string(),
            SummaryField::P50 => row.percentile_us(50).to_string(),
            SummaryField::P75 => row.percentile_us(75).to_string(),
            SummaryField::P90 => row.percentile_us(90).to_string(),
            SummaryField::P99 => row.percentile_us(99).to_string(),
            SummaryField::Slowest => render_optional_span_text(row.slowest_span, sources),
            SummaryField::Max => row.max_us.to_string(),
            SummaryField::Avg => format!("{:.2}", row.avg_us()),
        })
        .collect()
}

#[cfg(feature = "tools")]
fn render_optional_span_text(span: Option<Span>, sources: &SourceMap) -> String {
    let Some(span) = span else {
        return "-".to_string();
    };
    let mut output = String::new();
    render_span_text(span, sources, &mut output);
    output
}

#[cfg(feature = "tools")]
fn trace_summary_terminal_width() -> usize {
    terminal_table_width_for_stderr(MIN_TRACE_SUMMARY_WIDTH, DEFAULT_TRACE_SUMMARY_WIDTH)
}

#[derive(Clone, Debug, Default)]
#[cfg(feature = "tools")]
pub struct TraceFlamegraphRenderer;

#[cfg(feature = "tools")]
impl TraceFlamegraphRenderer {
    pub fn new() -> Self {
        Self
    }

    /// Render events as inferno-compatible folded-stack text.
    ///
    /// Each leaf operation (module call, method call, external process) is emitted
    /// as one line: `context;...;leaf DURATION_US`, where the context frames come
    /// from walking the parent-event chain up to the script root.
    pub fn render_events(&self, events: &[TraceEvent]) -> String {
        let names_by_id: FxHashMap<u64, String> = events
            .iter()
            .filter_map(|e| e.name.as_ref().map(|n| (e.event_id, n.clone())))
            .collect();
        let by_id: FxHashMap<u64, &TraceEvent> = events.iter().map(|e| (e.event_id, e)).collect();

        let mut output = String::new();

        for event in events {
            let Some(duration_us) = event.timing.duration_us else {
                continue;
            };
            if duration_us == 0 {
                continue;
            }

            let leaf = match event.kind {
                TraceKind::ModuleResult => {
                    format!("module.{}", flamegraph_leaf_name(event, &names_by_id))
                }
                TraceKind::MethodResult => {
                    format!("method.{}", flamegraph_leaf_name(event, &names_by_id))
                }
                TraceKind::RunEnd => {
                    format!("run:{}", flamegraph_leaf_name(event, &names_by_id))
                }
                TraceKind::CoreResult => {
                    format!("core.{}", flamegraph_leaf_name(event, &names_by_id))
                }
                _ => continue,
            };

            let mut frames: Vec<String> = Vec::new();
            let mut cursor = event.parent_event_id;
            while let Some(pid) = cursor {
                let Some(parent) = by_id.get(&pid) else {
                    break;
                };
                if let Some(label) = flamegraph_context_label(parent) {
                    frames.push(label);
                }
                cursor = parent.parent_event_id;
            }
            frames.reverse();
            frames.push(leaf);

            let _ = writeln!(output, "{} {duration_us}", frames.join(";"));
        }
        output
    }
}

#[cfg(feature = "tools")]
fn flamegraph_leaf_name(event: &TraceEvent, names_by_id: &FxHashMap<u64, String>) -> String {
    if let Some(name) = &event.name {
        return name.clone();
    }
    if let Some(parent_id) = event.parent_event_id
        && let Some(name) = names_by_id.get(&parent_id)
    {
        return name.clone();
    }
    match &event.payload {
        TracePayload::StreamStage { stage, .. } => stage.clone(),
        _ => "?".to_string(),
    }
}

#[cfg(feature = "tools")]
fn flamegraph_context_label(event: &TraceEvent) -> Option<String> {
    match event.kind {
        TraceKind::ScriptEnter => Some("script".to_string()),
        TraceKind::ProcEnter => Some(format!("proc:{}", event.name.as_deref().unwrap_or("?"))),
        TraceKind::PureEnter => Some(format!("pure:{}", event.name.as_deref().unwrap_or("?"))),
        TraceKind::StreamEnter => Some("stream".to_string()),
        TraceKind::StreamStageEnter => {
            let name = if let Some(n) = event.name.as_deref() {
                n.to_string()
            } else if let TracePayload::StreamStage { stage, .. } = &event.payload {
                stage.clone()
            } else {
                "?".to_string()
            };
            Some(format!("|>{name}"))
        }
        TraceKind::PipelineEnter => Some("pipeline".to_string()),
        _ => None,
    }
}

#[derive(Clone, Debug, Default)]
#[cfg(feature = "tools")]
pub struct TraceNormalizer;

#[cfg(feature = "tools")]
impl TraceNormalizer {
    pub fn new() -> Self {
        Self
    }

    pub fn normalize_events(&self, events: &[TraceEvent]) -> Vec<TraceEvent> {
        let mut id_map = FxHashMap::default();
        for (index, event) in events.iter().enumerate() {
            id_map.insert(event.event_id, index as u64 + 1);
        }

        events
            .iter()
            .enumerate()
            .map(|(index, event)| {
                let mut normalized = event.clone();
                normalized.event_id = index as u64 + 1;
                normalized.parent_event_id = normalized
                    .parent_event_id
                    .and_then(|parent| id_map.get(&parent).copied());
                normalized.timing = TraceTiming::default();
                normalized.payload.normalize();
                normalized
            })
            .collect()
    }
}

#[cfg(feature = "tools")]
pub trait TraceSink {
    fn write_event_line(&mut self, line: &str) -> io::Result<()>;
}

#[derive(Clone, Debug, Default)]
#[cfg(feature = "tools")]
pub struct MemoryTraceSink {
    lines: Vec<String>,
}

#[cfg(feature = "tools")]
impl MemoryTraceSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn into_string(self) -> String {
        self.lines.join("")
    }
}

#[cfg(feature = "tools")]
impl TraceSink for MemoryTraceSink {
    fn write_event_line(&mut self, line: &str) -> io::Result<()> {
        self.lines.push(line.to_string());
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Traceback {
    pub failing_span: Option<Span>,
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
        output.push_str("runtime traceback: ");
        output.push_str(&traceback.error.message);
        output.push('\n');

        if let Some(span) = traceback.failing_span {
            output.push_str(" --> ");
            render_span_text(span, sources, &mut output);
            output.push('\n');
        }

        output.push_str("operation: ");
        output.push_str(&traceback.operation_kind);
        output.push('\n');
        output.push_str("error: ");
        output.push_str(&traceback.error.kind);
        output.push_str(": ");
        output.push_str(&traceback.error.message);
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

#[cfg(feature = "tools")]
fn render_payload_text(payload: &TracePayload, output: &mut String) {
    match payload {
        TracePayload::None => {}
        TracePayload::Core { argv } => {
            output.push_str(" argv=");
            render_args_text(argv, output);
        }
        TracePayload::RunStart {
            target,
            argv,
            cwd,
            env,
        } => {
            let _ = write!(output, " target={}", target.quoted());
            output.push_str(" argv=");
            render_args_text(argv, output);
            let _ = write!(output, " cwd={}", cwd.quoted());
            if !env.is_empty() {
                output.push_str(" env=");
                render_env_text(env, output);
            }
        }
        TracePayload::RunEnd { pid, status, error } => {
            if let Some(pid) = pid {
                let _ = write!(output, " pid={pid}");
            }
            if let Some(status) = status {
                let _ = write!(
                    output,
                    " status={{kind:{} success:{} code:{}}}",
                    status.kind.as_str(),
                    status.success,
                    status
                        .code
                        .map_or_else(|| "null".to_string(), |code| code.to_string())
                );
            }
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::SpawnStart {
            handle_id,
            target,
            argv,
            cwd,
            env,
            detached,
        } => {
            if let Some(handle_id) = handle_id {
                let _ = write!(output, " handle_id={handle_id}");
            }
            let _ = write!(output, " target={}", target.quoted());
            output.push_str(" argv=");
            render_args_text(argv, output);
            let _ = write!(output, " cwd={}", cwd.quoted());
            if !env.is_empty() {
                output.push_str(" env=");
                render_env_text(env, output);
            }
            let _ = write!(output, " detached={detached}");
        }
        TracePayload::SpawnReady { handle_id, pid } => {
            let _ = write!(output, " handle_id={handle_id}");
            if let Some(pid) = pid {
                let _ = write!(output, " pid={pid}");
            }
        }
        TracePayload::WaitStart { handle_ids } => {
            output.push_str(" handle_ids=[");
            for (index, handle_id) in handle_ids.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                let _ = write!(output, "{handle_id}");
            }
            output.push(']');
        }
        TracePayload::WaitEnd {
            handle_id,
            pid,
            status,
            error,
        } => {
            if let Some(handle_id) = handle_id {
                let _ = write!(output, " handle_id={handle_id}");
            }
            if let Some(pid) = pid {
                let _ = write!(output, " pid={pid}");
            }
            if let Some(status) = status {
                let _ = write!(
                    output,
                    " status={{kind:{} success:{} code:{}}}",
                    status.kind.as_str(),
                    status.success,
                    status
                        .code
                        .map_or_else(|| "null".to_string(), |code| code.to_string())
                );
            }
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::SpawnCancel {
            handle_id,
            pid,
            signal,
            kill_after_ms,
            error,
        } => {
            let _ = write!(
                output,
                " handle_id={handle_id} signal={} kill_after_ms={kill_after_ms}",
                quote_text(signal)
            );
            if let Some(pid) = pid {
                let _ = write!(output, " pid={pid}");
            }
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::PipelineEnd { status, error } => {
            if let Some(status) = status {
                let _ = write!(
                    output,
                    " status={{kind:{} success:{} code:{}}}",
                    status.kind.as_str(),
                    status.success,
                    status
                        .code
                        .map_or_else(|| "null".to_string(), |code| code.to_string())
                );
            }
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::PipelineSegmentStart {
            index,
            target,
            argv,
            cwd,
            env,
        } => {
            let _ = write!(output, " index={index} target={}", target.quoted());
            output.push_str(" argv=");
            render_args_text(argv, output);
            let _ = write!(output, " cwd={}", cwd.quoted());
            if !env.is_empty() {
                output.push_str(" env=");
                render_env_text(env, output);
            }
        }
        TracePayload::PipelineSegmentEnd {
            index,
            pid,
            status,
            error,
        } => {
            let _ = write!(output, " index={index}");
            if let Some(pid) = pid {
                let _ = write!(output, " pid={pid}");
            }
            if let Some(status) = status {
                let _ = write!(
                    output,
                    " status={{kind:{} success:{} code:{}}}",
                    status.kind.as_str(),
                    status.success,
                    status
                        .code
                        .map_or_else(|| "null".to_string(), |code| code.to_string())
                );
            }
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::Redirection {
            op,
            target,
            fd,
            error,
        } => {
            let _ = write!(output, " op={}", quote_text(op));
            if let Some(target) = target {
                let _ = write!(output, " target={}", target.quoted());
            }
            if let Some(fd) = fd {
                let _ = write!(output, " fd={fd}");
            }
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::StreamStage {
            stage,
            item_count,
            error,
        } => {
            let _ = write!(output, " stage={}", quote_text(stage));
            if let Some(item_count) = item_count {
                let _ = write!(output, " item_count={item_count}");
            }
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::StreamItem {
            stage,
            item_index,
            error,
        }
        | TracePayload::ParallelJob {
            stage,
            item_index,
            error,
        } => {
            let _ = write!(
                output,
                " stage={} item_index={item_index}",
                quote_text(stage)
            );
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::RetryAttempt {
            attempt,
            max_attempts,
            next_delay_ms,
            error,
        } => {
            let _ = write!(output, " attempt={attempt} max_attempts={max_attempts}");
            if let Some(delay) = next_delay_ms {
                let _ = write!(output, " next_delay_ms={delay}");
            }
            if let Some(error) = error {
                let _ = write!(
                    output,
                    " error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::Cwd { previous, current } => {
            let _ = write!(
                output,
                " previous={} current={}",
                previous.quoted(),
                current.quoted()
            );
        }
        TracePayload::Signal {
            signal_name,
            signal_number,
            phase,
            matching_hook,
            forwarded,
            pre_cancel_ms,
            escalation_signal_name,
            escalation_signal_number,
            hook_error,
        } => {
            let _ = write!(
                output,
                " signal={} number={} phase={} matching_hook={} forwarded={}",
                quote_text(signal_name),
                signal_number,
                quote_text(phase),
                matching_hook,
                forwarded
            );
            if let Some(pre_cancel_ms) = pre_cancel_ms {
                let _ = write!(output, " pre_cancel_ms={pre_cancel_ms}");
            }
            if let Some(name) = escalation_signal_name {
                let _ = write!(output, " escalation_signal={}", quote_text(name));
            }
            if let Some(number) = escalation_signal_number {
                let _ = write!(output, " escalation_number={number}");
            }
            if let Some(error) = hook_error {
                let _ = write!(
                    output,
                    " hook_error={{kind:{} message:{}}}",
                    quote_text(&error.kind),
                    quote_text(&error.message)
                );
            }
        }
        TracePayload::ResultPropagate { error_kind } => {
            let _ = write!(output, " error_kind={}", quote_text(error_kind));
        }
        TracePayload::RuntimeError { error } => {
            let _ = write!(
                output,
                " error={{kind:{} message:{}}}",
                quote_text(&error.kind),
                quote_text(&error.message)
            );
        }
    }
}

#[cfg(feature = "tools")]
fn render_args_text(args: &[TraceArg], output: &mut String) {
    output.push('[');
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str(&arg.quoted());
    }
    output.push(']');
}

#[cfg(feature = "tools")]
fn render_env_text(env: &[TraceEnv], output: &mut String) {
    output.push('{');
    for (index, item) in env.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str(&item.name.quoted());
        output.push(':');
        output.push_str(&item.value.quoted());
    }
    output.push('}');
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

#[cfg(feature = "tools")]
fn render_string(value: &str, output: &mut String) {
    output.push('"');
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            ch if ch <= '\u{1f}' => {
                let _ = write!(output, "\\u{:04x}", ch as u32);
            }
            ch => output.push(ch),
        }
    }
    output.push('"');
}

#[cfg(feature = "tools")]
fn quote_text(value: &str) -> String {
    quote_bytes(value.as_bytes())
}

#[cfg(feature = "tools")]
fn quote_string_text(value: &str) -> String {
    let mut output = String::new();
    render_string(value, &mut output);
    output
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

#[cfg(feature = "tools")]
fn hex_string(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    render_hex(bytes, &mut output);
    output
}

#[cfg(feature = "tools")]
fn render_hex(bytes: &[u8], output: &mut String) {
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SyscallSummary, SyscallSummaryRenderer, SyscallTraceRecord, TraceArg, TraceError,
        TraceEvent, TraceJsonlRenderer, TraceKind, TraceNormalizer, TracePayload, TraceStatus,
        TraceStatusKind, TraceSummaryRenderer, TraceTextRenderer, TraceTiming, Traceback,
        TracebackFrame, TracebackFrameKind, TracebackRenderer,
    };
    use crate::source::{SourceMap, Span};
    use crate::terminal::table::table_text_width;

    fn json_field<'a>(value: &'a miniserde::json::Value, key: &str) -> &'a miniserde::json::Value {
        match value {
            miniserde::json::Value::Object(fields) => fields.get(key).unwrap(),
            _ => panic!("expected object"),
        }
    }

    fn json_index(value: &miniserde::json::Value, index: usize) -> &miniserde::json::Value {
        match value {
            miniserde::json::Value::Array(items) => &items[index],
            _ => panic!("expected array"),
        }
    }

    fn json_array(value: &miniserde::json::Value) -> &miniserde::json::Array {
        match value {
            miniserde::json::Value::Array(items) => items,
            _ => panic!("expected array"),
        }
    }

    fn json_str(value: &miniserde::json::Value) -> &str {
        match value {
            miniserde::json::Value::String(value) => value,
            _ => panic!("expected string"),
        }
    }

    fn json_u64(value: &miniserde::json::Value) -> u64 {
        match value {
            miniserde::json::Value::Number(miniserde::json::Number::U64(value)) => *value,
            miniserde::json::Value::Number(miniserde::json::Number::I64(value)) => {
                u64::try_from(*value).unwrap()
            }
            _ => panic!("expected u64"),
        }
    }

    #[test]
    fn renders_nested_trace_as_stable_text() {
        let (sources, events) = synthetic_trace();
        let events = TraceNormalizer::new().normalize_events(&events);

        let rendered = TraceTextRenderer::new().render_events(&events, &sources);

        assert_eq!(
            rendered,
            "id=1 parent=- depth=0 kind=script.enter name=\"sample\" span=sample.xsh:1:1-1:4\nid=2 parent=1 depth=1 kind=run.start name=\"printf\" span=sample.xsh:1:1-1:4 target=b\"printf\" argv=[b\"printf\", b\"hello world\", b\"line\\nfeed\", b\"\\xff\"] cwd=b\"/tmp/build\"\nid=3 parent=2 depth=1 kind=run.end span=sample.xsh:1:1-1:4 status={kind:exit success:true code:0}\n"
        );
    }

    #[test]
    fn renders_nested_trace_as_json_lines() {
        let (sources, events) = synthetic_trace();

        let rendered = TraceJsonlRenderer::new().render_events(&events, &sources);

        assert!(
            rendered
                .lines()
                .all(|line| line.starts_with('{') && line.ends_with('}'))
        );
        assert!(rendered.contains("\"kind\":\"run.start\""));

        let values = rendered
            .lines()
            .map(|line| {
                miniserde::json::from_str::<miniserde::json::Value>(line).expect("parse trace JSON")
            })
            .collect::<Vec<_>>();
        let run_start = values
            .iter()
            .find(|value| json_str(json_field(value, "kind")) == "run.start")
            .expect("run.start JSON event");
        assert_eq!(json_u64(json_field(run_start, "event_id")), 20);
        assert_eq!(json_u64(json_field(run_start, "parent_event_id")), 10);
        assert_eq!(
            json_str(json_field(json_field(run_start, "source_span"), "file")),
            "sample.xsh"
        );
        let payload = json_field(run_start, "payload");
        assert_eq!(json_str(json_field(payload, "type")), "run.start");
        assert_eq!(
            json_str(json_field(json_field(payload, "target"), "display")),
            "printf"
        );
        let argv = json_array(json_field(payload, "argv"));
        assert!(argv.iter().any(|arg| {
            json_str(json_field(arg, "hex")) == "ff" && json_str(json_field(arg, "display")) == "�"
        }));
    }

    #[test]
    fn wraps_trace_summary_cells_without_ellipsis() {
        let (sources, mut events) = synthetic_trace();
        events[1].name = Some("very-long-command-name-that-keeps-going".to_string());

        let rendered = TraceSummaryRenderer::new().render_events_with_width(&events, &sources, 60);

        assert!(!rendered.contains('…'));
        assert!(rendered.contains("very-long"));
        assert!(rendered.contains("-command-"));
        assert!(
            rendered.lines().all(|line| table_text_width(line) <= 60),
            "{rendered}"
        );
    }

    #[test]
    fn normalizes_unstable_trace_values() {
        let (sources, events) = synthetic_trace();

        let normalized = TraceNormalizer::new().normalize_events(&events);
        let rendered = TraceTextRenderer::new().render_events(&normalized, &sources);

        assert!(!rendered.contains("start_us="));
        assert!(!rendered.contains("duration_us="));
        assert!(!rendered.contains("pid="));
        assert!(rendered.contains("id=2 parent=1 depth=1 kind=run.start"));
    }

    #[test]
    fn aggregates_and_renders_syscall_summary_text() {
        let records = synthetic_syscalls();
        let summary = SyscallSummary::from_records(&records);

        assert_eq!(summary.syscall_count, 5);
        assert_eq!(summary.by_syscall[0].syscall, "read");
        assert_eq!(summary.by_syscall[0].calls, 3);
        assert_eq!(summary.by_syscall[0].errors, 1);
        assert_eq!(summary.by_process[0].pid, 10);
        assert_eq!(summary.by_program[0].program, "xsh");

        let rendered = SyscallSummaryRenderer::new(2).render_text(&summary);

        assert!(rendered.contains("syscall_count=5"));
        assert!(rendered.contains("syscall_seconds=0.000015"));
        assert!(rendered.contains("top_syscalls_by_count:"));
        assert!(rendered.contains("read calls=3 errors=1 seconds=0.000009 usecs/call=3"));
        assert!(rendered.contains("per_program_top_syscalls:"));
        assert!(rendered.contains("program=xsh calls=3"));
        assert!(rendered.contains("per_process_top_syscalls:"));
        assert!(rendered.contains("pid=10 program=xsh calls=3"));
    }

    #[test]
    fn renders_syscall_summary_jsonl() {
        let summary = SyscallSummary::from_records(&synthetic_syscalls());
        let rendered = SyscallSummaryRenderer::new(1).render_jsonl(&summary);
        let value: miniserde::json::Value =
            miniserde::json::from_str(rendered.trim()).expect("parse syscall summary JSON");

        assert_eq!(json_str(json_field(&value, "type")), "syscall.summary");
        assert_eq!(json_u64(json_field(&value, "syscall_count")), 5);
        let top = json_index(json_field(&value, "top_syscalls_by_count"), 0);
        assert_eq!(json_str(json_field(top, "syscall")), "read");
        assert_eq!(json_u64(json_field(top, "calls")), 3);
        assert_eq!(
            json_array(json_field(&value, "per_process_top_syscalls")).len(),
            1
        );
        assert_eq!(
            json_u64(json_field(
                json_index(
                    json_field(
                        json_index(json_field(&value, "per_program_top_syscalls"), 0),
                        "syscalls"
                    ),
                    0
                ),
                "calls"
            )),
            2
        );
    }

    #[test]
    fn renders_traceback_with_failing_span_and_call_path() {
        let mut sources = SourceMap::new();
        let id = sources.add_file(
            "sample.xsh",
            "pure helper() -> Result[Unit] {\n  run false ?\n}\n",
        );
        let traceback = Traceback {
            failing_span: Some(Span::new(id, 34, 43)),
            operation_kind: "run".to_string(),
            error: TraceError::new("nonzero-exit", "process exited with status 1"),
            frames: vec![
                TracebackFrame {
                    kind: TracebackFrameKind::Proc,
                    name: "main".to_string(),
                    definition_span: None,
                    call_span: Some(Span::new(id, 0, 4)),
                },
                TracebackFrame {
                    kind: TracebackFrameKind::Pure,
                    name: "helper".to_string(),
                    definition_span: None,
                    call_span: Some(Span::new(id, 5, 11)),
                },
            ],
        };

        let rendered = TracebackRenderer::new().render(&traceback, &sources);

        assert!(rendered.contains(" --> sample.xsh:2:3-2:12\n"));
        assert!(rendered.contains("  1. proc main at sample.xsh:1:1-1:5\n"));
        assert!(rendered.contains("  2. pure helper at sample.xsh:1:6-1:12\n"));
    }

    fn synthetic_trace() -> (SourceMap, Vec<TraceEvent>) {
        let mut sources = SourceMap::new();
        let id = sources.add_file("sample.xsh", "run printf\n");
        let span = Span::new(id, 0, 3);
        let events = vec![
            TraceEvent::new(10, TraceKind::ScriptEnter)
                .with_name("sample")
                .with_span(span)
                .with_timing(TraceTiming::new(Some(100), None)),
            TraceEvent::new(20, TraceKind::RunStart)
                .with_parent(10, 1)
                .with_name("printf")
                .with_span(span)
                .with_timing(TraceTiming::new(Some(110), None))
                .with_payload(TracePayload::RunStart {
                    target: TraceArg::text("printf"),
                    argv: vec![
                        TraceArg::text("printf"),
                        TraceArg::text("hello world"),
                        TraceArg::text("line\nfeed"),
                        TraceArg::bytes([0xff]),
                    ],
                    cwd: TraceArg::text("/tmp/build"),
                    env: Vec::new(),
                }),
            TraceEvent::new(30, TraceKind::RunEnd)
                .with_parent(20, 1)
                .with_span(span)
                .with_timing(TraceTiming::new(None, Some(7)))
                .with_payload(TracePayload::RunEnd {
                    pid: Some(4242),
                    status: Some(TraceStatus {
                        success: true,
                        kind: TraceStatusKind::Exit,
                        code: Some(0),
                    }),
                    error: None,
                }),
        ];
        (sources, events)
    }

    fn synthetic_syscalls() -> Vec<SyscallTraceRecord> {
        vec![
            SyscallTraceRecord::new(10, "xsh", "read", false, 2_000, 0),
            SyscallTraceRecord::new(10, "xsh", "read", true, 3_000, 0),
            SyscallTraceRecord::new(10, "xsh", "write", false, 4_000, 0),
            SyscallTraceRecord::new(11, "cat", "read", false, 4_000, 0),
            SyscallTraceRecord::new(11, "cat", "close", false, 2_000, 0),
        ]
    }
}
