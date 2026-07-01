use crate::xsht::cli::{CliOutput, TraceFormat, TraceOptions, push_text, text_bytes};
use std::fs;
use xsh::diagnostic::DiagnosticRenderer;
use xsh::loader::{entry_source_from_bytes, parse_load_entry_source_arena_only};
use xsh::runtime::eval::Evaluator;
use xsh::trace::{
    TraceFlamegraphRenderer, TraceJsonlRenderer, TraceSummaryRenderer, TraceTextRenderer,
    TracebackRenderer,
};

mod syscall_trace;

pub fn trace_script(options: TraceOptions) -> CliOutput {
    if options.syscalls {
        return syscall_trace::run(options);
    }

    trace_script_without_syscalls(options)
}

#[allow(clippy::single_call_fn)]
pub(crate) fn trace_script_without_syscalls(options: TraceOptions) -> CliOutput {
    let bytes = match fs::read(&options.script) {
        Ok(bytes) => bytes,
        Err(err) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!(
                    "xsht trace: failed to read '{}': {err}\n",
                    options.script
                )),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };
    let entry_source = entry_source_from_bytes(&options.script, bytes);
    let entry_source_id = entry_source.source_id;
    if !entry_source.diagnostics.is_empty() {
        let sources = entry_source.sources;
        return CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(
                DiagnosticRenderer::new().render(&entry_source.diagnostics, &sources),
            ),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    let (sources, parsed) =
        parse_load_entry_source_arena_only(&options.script, entry_source, Vec::new());
    if !parsed.diagnostics.is_empty() {
        return CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources)),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    let output = Evaluator::new_with_sources(options.args, sources)
        .with_tracing()
        .eval(&parsed.arena, entry_source_id);
    let sources = output.sources.clone();
    let mut stderr = output.stderr;

    let rendered = match options.format {
        TraceFormat::Text if options.raw => {
            TraceTextRenderer::new().render_events(&output.trace_events, &sources)
        }
        TraceFormat::Jsonl if options.raw => {
            TraceJsonlRenderer::new().render_events(&output.trace_events, &sources)
        }
        TraceFormat::Text => {
            TraceSummaryRenderer::new().render_events(&output.trace_events, &sources)
        }
        TraceFormat::Jsonl => {
            TraceSummaryRenderer::new().render_jsonl(&output.trace_events, &sources)
        }
        TraceFormat::Flamegraph => {
            TraceFlamegraphRenderer::new().render_events(&output.trace_events)
        }
    };
    let trace_text = if let Some(path) = options.file {
        if let Err(err) = fs::write(&path, rendered) {
            return CliOutput {
                status: 4,
                stdout: output.stdout,
                stderr: text_bytes(format!(
                    "xsht trace: failed to write trace file '{path}': {err}\n"
                )),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
        String::new()
    } else {
        rendered
    };

    if !output.diagnostics.is_empty() {
        push_text(
            &mut stderr,
            &DiagnosticRenderer::new().render(&output.diagnostics, &sources),
        );
    }

    let status = if let Some(traceback) = output.traceback {
        push_text(
            &mut stderr,
            &TracebackRenderer::new().render(&traceback, &sources),
        );
        3
    } else {
        output.status
    };

    CliOutput {
        status,
        stdout: output.stdout,
        stderr,
        trace_text,
        syscall_summary: None,
    }
}
