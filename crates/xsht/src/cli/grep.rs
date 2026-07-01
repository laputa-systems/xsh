use crate::xsht::cli::{
    CliOutput, cancellation_output, collect_configured_or_explicit_xsh_files, load_config,
    text_bytes,
};
use crate::xsht::grep::{
    find_matches_in_program, line_at_offset, offset_to_line, parse_pattern_expr,
};
use std::fs;
use std::path::Path;
use xsh::source::SourceMap;
use xsh::syntax::parser::Parser;

pub fn grep_scripts(pattern_str: &str, paths: &[String]) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }

    let pattern_expr = match parse_pattern_expr(pattern_str) {
        Ok(expr) => expr,
        Err(message) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht grep: {message}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };

    let config = match load_config() {
        Ok(config) => config,
        Err(message) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht grep: {message}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };
    let files = match collect_configured_or_explicit_xsh_files(Path::new("."), &config, paths) {
        Ok(files) => files,
        Err(message) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht grep: {message}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    }
    .into_iter()
    .map(|path| path.to_string_lossy().into_owned())
    .collect::<Vec<_>>();

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut total_matches = 0usize;
    let mut status = 0u8;

    for file in &files {
        if let Some(output) = cancellation_output() {
            return output;
        }
        let text = match fs::read_to_string(file) {
            Ok(t) => t,
            Err(err) => {
                stderr.push_str(&format!("xsht grep: failed to read '{file}': {err}\n"));
                status = 2;
                continue;
            }
        };

        let mut sources = SourceMap::new();
        let source_id = sources.add_file(file, text.clone());
        let parsed = Parser::parse_source_arena_only(source_id, &text);
        if !parsed.diagnostics.is_empty() {
            continue;
        }

        let mut matches = Vec::new();
        find_matches_in_program(&pattern_expr, &parsed.arena, &text, &mut matches);

        for m in &matches {
            let line = offset_to_line(&text, m.span.start());
            let line_text = line_at_offset(&text, m.span.start());
            stdout.push_str(&format!("{file}:{line}:{line_text}\n"));
            total_matches += 1;
        }
    }

    stdout.push_str(&format!(
        "{total_matches} match{}\n",
        if total_matches == 1 { "" } else { "es" }
    ));

    if status == 0 && total_matches == 0 {
        status = 1;
    }

    CliOutput {
        status,
        stdout: stdout.into_bytes(),
        stderr: stderr.into_bytes(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}
