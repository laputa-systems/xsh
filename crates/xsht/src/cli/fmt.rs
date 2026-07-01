use crate::xsht::cli::{
    CliOutput, XshConfig, cancellation_output, collect_configured_xsh_files, load_config,
    text_bytes,
};
use crate::xsht::config::config_for_file;
use crate::xsht::format::Formatter;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use xsh::diagnostic::DiagnosticRenderer;
use xsh::loader::parse_load_check_file;
use xsh::sema::check::CheckOptions;

pub fn format_files(files: &[String], check: bool) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut status = 0;

    let discovered;
    let files: &[String] = if files.is_empty() {
        let config = match load_config() {
            Ok(config) => config,
            Err(message) => {
                return CliOutput {
                    status: 2,
                    stdout: stdout.into_bytes(),
                    stderr: text_bytes(format!("xsht: {message}\n")),
                    trace_text: String::new(),
                    syscall_summary: None,
                };
            }
        };
        let mut paths = Vec::new();
        if let Err(message) = collect_configured_xsh_files(Path::new("."), &config, &mut paths) {
            if let Some(output) = cancellation_output() {
                return output;
            }
            return CliOutput {
                status: 2,
                stdout: stdout.into_bytes(),
                stderr: text_bytes(format!("xsht: {message}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
        discovered = paths
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        &discovered
    } else {
        files
    };

    let mut results = format_files_parallel(files);
    if let Some(output) = cancellation_output() {
        return output;
    }
    results.sort_unstable_by_key(|result| result.index);

    for result in results {
        match result.kind {
            FormatResultKind::Clean => {}
            FormatResultKind::ConfigError(message) => {
                status = 2;
                stderr.push_str(&message);
            }
            FormatResultKind::ReadError(message) => {
                status = 2;
                stderr.push_str(&message);
            }
            FormatResultKind::ParseError(message) => {
                status = 2;
                stderr.push_str(&message);
            }
            FormatResultKind::NeedsFormat(formatted) => {
                if check {
                    if status == 0 {
                        status = 1;
                    }
                    stdout.push_str(&result.file);
                    stdout.push_str(": needs formatting\n");
                } else if let Err(err) = fs::write(&result.file, formatted) {
                    status = 4;
                    stderr.push_str(&format!("xsht: failed to write '{}': {err}\n", result.file));
                }
            }
        }
    }

    CliOutput {
        status,
        stdout: stdout.into_bytes(),
        stderr: stderr.into_bytes(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

struct FormatResult {
    index: usize,
    file: String,
    kind: FormatResultKind,
}

enum FormatResultKind {
    Clean,
    NeedsFormat(String),
    ConfigError(String),
    ReadError(String),
    ParseError(String),
}

#[allow(clippy::single_call_fn)]
fn format_files_parallel(files: &[String]) -> Vec<FormatResult> {
    if files.is_empty() {
        return Vec::new();
    }
    let next = AtomicUsize::new(0);
    let (tx, rx) = crossbeam_channel::unbounded();
    let workers = worker_count(files.len());

    thread::scope(|scope| {
        for _ in 0..workers {
            let next = &next;
            let tx = tx.clone();
            scope.spawn(move || {
                loop {
                    if cancellation_output().is_some() {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = files.get(index) else {
                        break;
                    };
                    if tx.send(format_one_file(index, file)).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(tx);
    rx.into_iter().collect()
}

#[allow(clippy::single_call_fn)]
fn format_one_file(index: usize, file: &str) -> FormatResult {
    let config = match config_for_file(file, &XshConfig::default()) {
        Ok(config) => config,
        Err(message) => {
            return FormatResult {
                index,
                file: file.to_string(),
                kind: FormatResultKind::ConfigError(format!("xsht: {message}\n")),
            };
        }
    };
    let checked_program =
        match parse_load_check_file(file, config.module_roots(), CheckOptions::default()) {
            Ok(program) => program,
            Err(err) => {
                return FormatResult {
                    index,
                    file: file.to_string(),
                    kind: FormatResultKind::ReadError(format!(
                        "xsht: failed to read '{file}': {err}\n"
                    )),
                };
            }
        };
    if !checked_program.parsed.diagnostics.is_empty() {
        return FormatResult {
            index,
            file: file.to_string(),
            kind: FormatResultKind::ParseError(checked_program.render_parse_diagnostics()),
        };
    }
    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    if !checked.diagnostics.is_empty() {
        return FormatResult {
            index,
            file: file.to_string(),
            kind: FormatResultKind::ParseError(checked_program.render_check_diagnostics()),
        };
    }
    let Some(text) = checked_program.entry_source_text() else {
        return FormatResult {
            index,
            file: file.to_string(),
            kind: FormatResultKind::ParseError("xsht: missing script source\n".to_string()),
        };
    };
    let formatted = Formatter::new()
        .with_line_width(config.line_width())
        .format_source(checked_program.entry_source_id, text);
    if !formatted.diagnostics.is_empty() {
        return FormatResult {
            index,
            file: file.to_string(),
            kind: FormatResultKind::ParseError(
                DiagnosticRenderer::new().render(&formatted.diagnostics, &checked_program.sources),
            ),
        };
    }

    let kind = if formatted.formatted == text {
        FormatResultKind::Clean
    } else {
        FormatResultKind::NeedsFormat(formatted.formatted)
    };
    FormatResult {
        index,
        file: file.to_string(),
        kind,
    }
}

#[allow(clippy::single_call_fn)]
fn worker_count(file_count: usize) -> usize {
    thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, file_count)
}
