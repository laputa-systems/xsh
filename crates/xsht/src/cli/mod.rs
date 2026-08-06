#![allow(dead_code, unused_imports)]

use xsh::runner::ScriptOutput;
use xsh::runtime::process::cancellation_requested_signal;
use xsh::trace::SyscallSummary;

/// Result contract shared by `xsht::cli` command adapters.
#[derive(Clone, Debug)]
pub struct CliOutput {
    pub status: u8,
    pub stdout: Vec<u8>,
    /// Script's own stderr (no trace text).
    pub stderr: Vec<u8>,
    /// Rendered trace output (XSH trace + syscall summary). Empty when no trace was requested
    /// or when a trace_file was specified.
    pub trace_text: String,
    /// Structured syscall summary, populated by the ptrace supervisor.
    pub syscall_summary: Option<SyscallSummary>,
}

impl CliOutput {
    pub fn success() -> Self {
        Self {
            status: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            trace_text: String::new(),
            syscall_summary: None,
        }
    }
}

impl From<ScriptOutput> for CliOutput {
    fn from(output: ScriptOutput) -> Self {
        Self {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
            trace_text: String::new(),
            syscall_summary: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceOptions {
    pub script: String,
    pub args: Vec<String>,
    pub raw: bool,
    pub format: TraceFormat,
    pub file: Option<String>,
    pub syscalls: bool,
    pub top_syscalls: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceFormat {
    Text,
    Jsonl,
    Flamegraph,
}

fn text_bytes(text: impl Into<String>) -> Vec<u8> {
    text.into().into_bytes()
}

fn push_text(buf: &mut Vec<u8>, text: &str) {
    buf.extend_from_slice(text.as_bytes());
}

pub fn cancellation_output() -> Option<CliOutput> {
    let signal = cancellation_requested_signal()?;
    Some(CliOutput {
        status: cancellation_status(signal),
        stdout: Vec::new(),
        stderr: text_bytes(format!(
            "xsht: interrupted by {}\n",
            cancellation_signal_name(signal)
        )),
        trace_text: String::new(),
        syscall_summary: None,
    })
}

#[allow(clippy::single_call_fn)]
fn cancellation_status(signal: i32) -> u8 {
    (128 + signal).clamp(1, 255) as u8
}

#[allow(clippy::single_call_fn)]
fn cancellation_signal_name(signal: i32) -> String {
    match signal {
        libc::SIGINT => "SIGINT".to_string(),
        libc::SIGTERM => "SIGTERM".to_string(),
        _ => format!("signal {signal}"),
    }
}

mod api;
mod check;
mod coverage;
mod files;
mod fmt;
mod grep;
mod lint;
mod refactor;
mod syntax_tree;
mod trace;

pub use api::api_command;
pub use check::{
    AnnotationPolicy, AnnotationSelection, check_paths_with_options,
    check_paths_with_summary_options, check_script, check_script_with_options,
};
pub use coverage::CoverageCollector;
pub use files::{
    CONFIG_FILE_NAME, CoverageConfig, FormatConfig, XshConfig, collect_configured_xsh_files,
    collect_xsh_files,
    load_config,
};
pub(crate) use files::{
    collect_configured_or_explicit_xsh_files, is_path_excluded, load_config_from,
    nearest_config_for_file, resolve_config_path,
};
pub use fmt::format_files;
pub use grep::grep_scripts;
pub use lint::lint_files;
pub use refactor::refactor_scripts;
pub use syntax_tree::ast_script;
pub use trace::trace_script;

use xsh::loader::{parse_script, parse_script_with_module_roots};
