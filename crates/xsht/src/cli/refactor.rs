use crate::xsht::cli::{
    CliOutput, cancellation_output, collect_configured_or_explicit_xsh_files, load_config,
    text_bytes,
};
use crate::xsht::grep::{
    apply_replacement, find_matches_in_program, line_at_offset, offset_to_line, parse_pattern_expr,
};
use std::cmp::Reverse;
use std::fs;
use std::path::Path;
use xsh::source::SourceMap;
use xsh::syntax::parser::Parser;

pub fn refactor_scripts(
    pattern_str: &str,
    replacement_str: &str,
    paths: &[String],
    dry_run: bool,
) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }

    let pattern_expr = match parse_pattern_expr(pattern_str) {
        Ok(expr) => expr,
        Err(message) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht refactor: {message}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };

    let replacement_expr = match parse_pattern_expr(replacement_str) {
        Ok(expr) => expr,
        Err(message) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht refactor: {message}\n")),
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
                stderr: text_bytes(format!("xsht refactor: {message}\n")),
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
                stderr: text_bytes(format!("xsht refactor: {message}\n")),
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
    let mut total_changes = 0usize;
    let mut status = 0u8;

    for file in &files {
        if let Some(output) = cancellation_output() {
            return output;
        }
        let text = match fs::read_to_string(file) {
            Ok(t) => t,
            Err(err) => {
                stderr.push_str(&format!("xsht refactor: failed to read '{file}': {err}\n"));
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

        if matches.is_empty() {
            continue;
        }

        // Build replacements: (start, end, new_text), sorted in reverse order to preserve offsets.
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();
        for m in &matches {
            if let Some(new_text) = apply_replacement(&replacement_expr, m, &text) {
                replacements.push((m.span.start(), m.span.end(), new_text));
            }
        }

        // Remove overlapping/duplicate spans (keep first occurrence per span start).
        replacements.sort_unstable_by_key(|(start, _, _)| *start);
        replacements.dedup_by_key(|(start, _, _)| *start);

        for (start, _, new_text) in &replacements {
            let line = offset_to_line(&text, *start);
            let old_line = line_at_offset(&text, *start);
            stdout.push_str(&format!("{file}:{line}: {old_line}\n"));
            stdout.push_str(&format!("      -> {new_text}\n"));
            total_changes += 1;
        }

        if !dry_run {
            // Apply in reverse order so offsets remain valid.
            let mut new_text = text.clone();
            let mut rev = replacements.clone();
            rev.sort_unstable_by_key(|(start, _, _)| Reverse(*start));
            for (start, end, new) in rev {
                new_text.replace_range(start..end, &new);
            }
            if let Err(err) = fs::write(file, &new_text) {
                stderr.push_str(&format!("xsht refactor: failed to write '{file}': {err}\n"));
                status = 4;
            }
        }
    }

    stdout.push_str(&format!(
        "{total_changes} change{}{}\n",
        if total_changes == 1 { "" } else { "s" },
        if dry_run { " (dry run)" } else { "" }
    ));

    if status == 0 && total_changes == 0 {
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
