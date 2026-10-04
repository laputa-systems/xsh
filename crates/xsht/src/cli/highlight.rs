use crate::xsht::cli::{CliOutput, text_bytes};
use xsh::frontend::syntax::highlight::highlight;

/// Prints the highlight runs of one script as JSON Lines, one
/// `{"kind":...,"text":...}` object per run, so the lines concatenate back to
/// the source. The file is read as text and never parsed or checked: a
/// half-written script highlights like any other.
pub fn highlight_script(script: &str) -> CliOutput {
    let source = match std::fs::read_to_string(script) {
        Ok(source) => source,
        Err(err) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht: failed to read '{script}': {err}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };

    let mut stdout = String::new();
    for run in highlight(&source) {
        stdout.push_str(&format!(
            "{{\"kind\":{},\"text\":{}}}\n",
            miniserde::json::to_string(run.kind.name()),
            miniserde::json::to_string(&source[run.start..run.end])
        ));
    }
    CliOutput {
        status: 0,
        stdout: text_bytes(stdout),
        stderr: Vec::new(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}
