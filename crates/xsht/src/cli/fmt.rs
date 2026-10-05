use crate::xsht::cli::{
    CliOutput, ConfigCache, DiscoveryFor, XshConfig, cancellation_output, discover_scripts,
    load_config, text_bytes,
};
use crate::xsht::config::config_for_file;
use crate::xsht::format::Formatter;
use rustc_hash::FxHashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticRenderer};
use xsh::frontend::check::CheckOptions;
use xsh::frontend::load::parse_load_check_file;
use xsh::frontend::source::SourceMap;

pub fn format_files(files: &[String], check: bool) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut status = 0;
    let mut seen_diagnostics = FxHashSet::default();

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
    let discovered = match discover_format_files(files, &config) {
        Ok(paths) => paths,
        Err(message) => {
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
    };

    let Some(mut results) = format_files_parallel(discovered) else {
        return cancellation_output().expect("formatting stops early only for a signal");
    };
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
            FormatResultKind::Diagnostics(diagnostics) => {
                status = 2;
                for diagnostic in diagnostics {
                    if seen_diagnostics.insert(diagnostic.key) {
                        stderr.push_str(&diagnostic.text);
                    }
                }
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

fn discover_format_files(files: &[String], cwd_config: &XshConfig) -> Result<Vec<String>, String> {
    // Discovery also honors `[format] exclude`; a file named on the command
    // line is always formatted.
    let discovered = discover_scripts(
        files,
        cwd_config,
        &ConfigCache::default(),
        DiscoveryFor::Formatting,
    )?;
    Ok(discovered
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
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
    Diagnostics(Vec<RenderedDiagnostic>),
}

struct RenderedDiagnostic {
    key: String,
    text: String,
}

/// How often the command looks for a cancellation request while its workers
/// format.
const CANCELLATION_POLL: Duration = Duration::from_millis(20);

/// Formats `files` on worker threads. `None` means a cancellation request
/// arrived first.
///
/// A file is one unit of work for a worker, and a large one takes seconds to
/// load, check, and format, so a worker cannot answer a signal promptly. The
/// command does instead: it waits for results with a look at the signal
/// between them and returns as soon as one is requested, leaving the workers
/// to end with the process. Nothing is lost by that, because a worker only
/// computes text and the command alone writes files, after every result is
/// in.
#[allow(clippy::single_call_fn)]
fn format_files_parallel(files: Vec<String>) -> Option<Vec<FormatResult>> {
    if files.is_empty() {
        return Some(Vec::new());
    }
    let file_count = files.len();
    let files: Arc<[String]> = files.into();
    let next = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut workers = Vec::new();
    for _ in 0..worker_count(file_count) {
        let (files, next, tx) = (Arc::clone(&files), Arc::clone(&next), tx.clone());
        // The writer recurses once per level of source nesting, like the
        // passes that prepare a script, and the platform's default worker
        // stack does not hold the deepest source the parser accepts.
        let worker = thread::Builder::new()
            .name("xsht-fmt".to_string())
            .stack_size(super::FRONTEND_WORKER_STACK_BYTES)
            .spawn(move || {
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
            })
            .expect("spawn a formatting worker");
        workers.push(worker);
    }
    drop(tx);

    let mut results = Vec::with_capacity(file_count);
    while results.len() < file_count {
        match rx.recv_timeout(CANCELLATION_POLL) {
            Ok(result) => results.push(result),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if cancellation_output().is_some() {
                    return None;
                }
            }
            // Every worker is gone with files left: one saw the signal, or
            // one panicked.
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
    for worker in workers {
        if let Err(panic) = worker.join() {
            std::panic::resume_unwind(panic);
        }
    }
    (results.len() == file_count).then_some(results)
}

#[allow(clippy::single_call_fn)]
fn format_one_file(index: usize, file: &str) -> FormatResult {
    let config = match config_for_file(file) {
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
            kind: FormatResultKind::Diagnostics(render_diagnostics_with_keys(
                &checked_program.parsed.diagnostics,
                &checked_program.sources,
            )),
        };
    }
    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    // Formatting removes exactly the parentheses `check.redundant-parens` rejects.
    let blocking = checked
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code != Some(DiagnosticCode::CheckRedundantParens))
        .cloned()
        .collect::<Vec<_>>();
    if !blocking.is_empty() {
        return FormatResult {
            index,
            file: file.to_string(),
            kind: FormatResultKind::Diagnostics(render_diagnostics_with_keys(
                &blocking,
                &checked_program.sources,
            )),
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
        .format_parsed_source(text, &checked_program.parsed);
    if !formatted.diagnostics.is_empty() {
        return FormatResult {
            index,
            file: file.to_string(),
            kind: FormatResultKind::Diagnostics(render_diagnostics_with_keys(
                &formatted.diagnostics,
                &checked_program.sources,
            )),
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

fn render_diagnostics_with_keys(
    diagnostics: &[Diagnostic],
    sources: &SourceMap,
) -> Vec<RenderedDiagnostic> {
    diagnostics
        .iter()
        .map(|diagnostic| RenderedDiagnostic {
            key: diagnostic_key(diagnostic, sources),
            text: DiagnosticRenderer::new().render(std::slice::from_ref(diagnostic), sources),
        })
        .collect()
}

fn diagnostic_key(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let span = diagnostic
        .labels
        .first()
        .map(|label| label.span)
        .or(diagnostic.span);
    let location = span.and_then(|span| {
        sources
            .location(span.source_id, span.start())
            .map(|loc| (span, loc))
    });
    match location {
        Some((span, loc)) => format!(
            "{:?}:{}:{}:{}:{}:{}",
            diagnostic.severity,
            diagnostic.code.map_or("", DiagnosticCode::name),
            diagnostic.message,
            loc.file,
            span.start(),
            span.end(),
        ),
        None => format!(
            "{:?}:{}:{}",
            diagnostic.severity,
            diagnostic.code.map_or("", DiagnosticCode::name),
            diagnostic.message,
        ),
    }
}

#[allow(clippy::single_call_fn)]
fn worker_count(file_count: usize) -> usize {
    thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, file_count)
}

#[cfg(test)]
mod tests {
    use super::discover_format_files;
    use crate::xsht::cli::XshConfig;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn explicit_directory_discovery_does_not_expand_parent_includes() {
        let root = TempDir::new().expect("create temp root");
        let project = root.path().join("project");
        fs::create_dir(&project).expect("create project directory");
        fs::write(root.path().join("xsht-config.ini"), "include = extra\n")
            .expect("write parent config");
        let script = project.join("main.xsh");
        fs::write(&script, "let value = 1\n").expect("write script");

        let files = discover_format_files(
            &[project.to_string_lossy().into_owned()],
            &XshConfig::default(),
        )
        .expect("discover explicit directory");
        assert_eq!(files, vec![script.to_string_lossy()]);
    }
}
