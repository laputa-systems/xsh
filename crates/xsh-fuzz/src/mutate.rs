//! Mutation of well-formed programs into likely ill-typed or ill-formed ones,
//! and the formatter/lint invariants checked on well-formed programs.
//!
//! Mutants only go through parsing, checking, and preparation: the frontend
//! must reject them with ordinary diagnostics, never panic, hang, or report an
//! internal error, and every diagnostic span must address the source. Every
//! mutant that parses without diagnostics must be a grammar sentence.

use crate::harness::check_text;
use crate::rng::Rng;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::grammar::earley::Recognizer;
use xsh::frontend::syntax::grammar::{grammar, grammar_tokens};
use xsh::frontend::syntax::parser::Parser;

/// Corpus directories, relative to the repository root. They are only read.
pub const CORPUS_DIRS: &[&str] = &["tests", "core", "dev", "showcase", "examples"];

/// Corpus files larger than this are skipped: checking a mutant holds the
/// whole program's facts in memory.
pub const MAX_CORPUS_BYTES: u64 = 32 << 10;

/// Path under which mutants are checked. It has no sibling modules, so a
/// mutant's `use` fails fast instead of loading (and retaining) a large
/// module graph from the repository.
pub const MUTANT_FILE: &str = "mutant.xsh";

/// Every `.xsh` file of at most [`MAX_CORPUS_BYTES`] under the corpus
/// directories of `root`, sorted. Directory symlinks are not traversed.
pub fn corpus_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in CORPUS_DIRS {
        collect(&root.join(dir), &mut files);
    }
    files.retain(|path| {
        std::fs::metadata(path).is_ok_and(|metadata| metadata.len() <= MAX_CORPUS_BYTES)
    });
    files.sort();
    files
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.symlink_metadata().is_ok_and(|metadata| metadata.is_symlink()) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            if !matches!(entry.file_name().to_str(), Some("target" | ".git" | ".claude" | ".work")) {
                collect(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "xsh") && path.is_file() {
            out.push(path);
        }
    }
}

const TOKENS: &[&str] = &[
    "?",
    "??",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    ",",
    ".",
    ":",
    "=",
    "->",
    "|",
    "@",
    "...",
    "..",
    ";",
    "\n",
    "+",
    "-",
    "*",
    "/",
    "%",
    "==",
    "!=",
    "<",
    ">=",
    "!",
    "and",
    "or",
    "in",
    "not in",
    "is",
    "let",
    "var",
    "if",
    "else",
    "match",
    "for",
    "while",
    "return",
    "break",
    "continue",
    "try",
    "pure",
    "proc",
    "stream",
    "type",
    "enum",
    "error",
    "yield",
    "defer",
    "assert",
    "guard",
    "with",
    "ctx",
    "cd",
    "env",
    "run",
    "spawn",
    "wait",
    "null",
    "true",
    "0",
    "-1",
    "9223372036854775807",
    "1.5",
    "\"s\"",
    "p\"x\"",
    "b\"\\xff\"",
    "rx\"(\"",
    "f\"{",
    "f\"}",
    "\"\"\"",
    "Ok(",
    "Err(",
    "Int",
    "Str",
    "List[",
    "Map[",
    "Result[",
    "?.",
    "?[",
    "=>",
    "_",
    "{ |x| ",
    "#",
    "`",
];

const TYPES: &[&str] = &[
    "Int",
    "Str",
    "Bool",
    "Float",
    "Path",
    "Bytes",
    "List[Int]",
    "Map[Str]",
    "Int?",
    "Result[Int]",
    "Any",
    "Unit",
];

fn char_boundary_floor(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn identifiers(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        let word = ch.is_ascii_alphanumeric() || ch == '_';
        match (start, word) {
            (None, true) if !ch.is_ascii_digit() => start = Some(index),
            (Some(begin), false) => {
                out.push(&text[begin..index]);
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// One random mutation of `text`.
pub fn mutate(text: &str, rng: &mut Rng) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return rng.pick(TOKENS).to_string();
    }
    let mut out: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
    match rng.below(12) {
        0 => {
            out.remove(rng.below(out.len()));
        }
        1 => {
            let index = rng.below(out.len());
            let line = out[index].clone();
            out.insert(index, line);
        }
        2 => {
            let a = rng.below(out.len());
            let b = rng.below(out.len());
            out.swap(a, b);
        }
        3 => {
            let joined = out.join("\n");
            let cut = char_boundary_floor(&joined, rng.below(joined.len() + 1));
            return joined[..cut].to_string();
        }
        4 | 5 => {
            // Insert a token at a random character position.
            let index = rng.below(out.len());
            let line = &out[index];
            let at = char_boundary_floor(line, rng.below(line.len() + 1));
            out[index] = format!("{}{}{}", &line[..at], rng.pick(TOKENS), &line[at..]);
        }
        6 | 7 => {
            // Delete a short character range.
            let index = rng.below(out.len());
            let line = &out[index];
            if !line.is_empty() {
                let start = char_boundary_floor(line, rng.below(line.len()));
                let end = char_boundary_floor(line, start + 1 + rng.below(6));
                out[index] = format!("{}{}", &line[..start], &line[end.max(start)..]);
            }
        }
        8 => {
            // Replace one identifier with another from the same file.
            let joined = out.join("\n");
            let names = identifiers(&joined);
            if names.len() >= 2 {
                let from = *rng.pick(&names);
                let to = *rng.pick(&names);
                let occurrences: Vec<usize> =
                    joined.match_indices(from).map(|(at, _)| at).collect();
                let at = *rng.pick(&occurrences);
                return format!("{}{}{}", &joined[..at], to, &joined[at + from.len()..]);
            }
        }
        9 => {
            // Swap a type annotation.
            let joined = out.join("\n");
            let types: Vec<usize> = TYPES
                .iter()
                .flat_map(|ty| joined.match_indices(ty).map(|(at, _)| at))
                .collect();
            if !types.is_empty() {
                let at = *rng.pick(&types);
                let old = TYPES
                    .iter()
                    .find(|ty| joined[at..].starts_with(**ty))
                    .expect("type");
                return format!(
                    "{}{}{}",
                    &joined[..at],
                    rng.pick(TYPES),
                    &joined[at + old.len()..]
                );
            }
        }
        10 => {
            // Remove or add a propagation `?`.
            let joined = out.join("\n");
            let marks: Vec<usize> = joined.match_indices('?').map(|(at, _)| at).collect();
            if !marks.is_empty() && rng.chance(50) {
                let at = *rng.pick(&marks);
                return format!("{}{}", &joined[..at], &joined[at + 1..]);
            }
            let closes: Vec<usize> = joined.match_indices(')').map(|(at, _)| at + 1).collect();
            if !closes.is_empty() {
                let at = *rng.pick(&closes);
                return format!("{}?{}", &joined[..at], &joined[at..]);
            }
        }
        _ => {
            // Move a line into a different nesting position.
            let from = rng.below(out.len());
            let line = out.remove(from);
            let to = rng.below(out.len() + 1);
            out.insert(to, line);
        }
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// Checks a mutant; `Err` describes a frontend defect.
pub fn check_mutant(file: &str, text: &str) -> Result<bool, String> {
    let report = check_text(file, text);
    if let Some(internal) = report.internal_error() {
        return Err(internal);
    }
    // Loading adds import diagnostics to the parse report. Parse separately
    // so unresolved imports and checker errors cannot hide grammar drift.
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), text);
    if parsed.diagnostics.is_empty() {
        static RECOGNIZER: OnceLock<Recognizer> = OnceLock::new();
        let recognizer = RECOGNIZER.get_or_init(|| Recognizer::new(grammar()));
        let tokens = grammar_tokens(text, parsed.cst.token_table());
        if let Err(rejection) = recognizer.recognize(&tokens) {
            let near: Vec<&str> = tokens
                [rejection.token.saturating_sub(4)..(rejection.token + 3).min(tokens.len())]
                .iter()
                .map(|token| token.text)
                .collect();
            return Err(format!(
                "grammar rejected a cleanly parsed mutant near {near:?}; expected {}",
                rejection.expected.join(" ")
            ));
        }
    }
    Ok(report.accepted())
}

/// Formatter and lint invariants for a program that checks clean: the
/// formatter must accept it, its output must check clean, be a fixed point,
/// and leave the lint codes unchanged.
pub fn format_invariants(source: &str, scratch: &Path) -> Result<(), String> {
    let formatter = xsht::format::Formatter::new();
    let first = formatter.format_source(SourceId::new(0), source);
    if !first.diagnostics.is_empty() {
        let messages: Vec<String> = first
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect();
        return Err(format!(
            "formatter refused a checked program: {}",
            messages.join("; ")
        ));
    }
    let second = formatter.format_source(SourceId::new(0), &first.formatted);
    if second.formatted != first.formatted {
        return Err(format!(
            "formatting is not idempotent\n--- first\n{}--- second\n{}",
            first.formatted, second.formatted
        ));
    }
    let report = check_text("formatted.xsh", &first.formatted);
    if !report.accepted() {
        let mut text = String::from("formatted program no longer checks:\n");
        for line in report
            .parse
            .iter()
            .chain(&report.check)
            .chain(&report.lower)
        {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str("--- formatted\n");
        text.push_str(&first.formatted);
        return Err(text);
    }
    let before = lint_codes(source, &scratch.join("lint-original.xsh"))?;
    let after = lint_codes(&first.formatted, &scratch.join("lint-formatted.xsh"))?;
    if before != after {
        let mut only_before = before.clone();
        let mut only_after = Vec::new();
        for code in after {
            match only_before.iter().position(|existing| *existing == code) {
                Some(index) => {
                    only_before.remove(index);
                }
                None => only_after.push(code),
            }
        }
        return Err(format!(
            "lint codes changed by formatting: lost {only_before:?}, gained {only_after:?}\n--- formatted\n{}",
            first.formatted
        ));
    }
    Ok(())
}

/// The sorted lint diagnostic codes `xsht lint` reports for `source`.
pub fn lint_codes(source: &str, path: &Path) -> Result<Vec<String>, String> {
    std::fs::write(path, source).map_err(|error| format!("write {}: {error}", path.display()))?;
    let output = xsht::lint_files(&[path.to_string_lossy().into_owned()], false, false, None);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut codes = Vec::new();
    for line in stderr.lines() {
        // Diagnostic headers read `warn[lint.code]: message`.
        if let Some(start) = line.find('[')
            && start > 0
            && line[..start].bytes().all(|byte| byte.is_ascii_lowercase())
            && let Some(len) = line[start + 1..].find(']')
        {
            codes.push(line[start + 1..start + 1 + len].to_string());
        }
    }
    codes.sort();
    Ok(codes)
}

#[cfg(test)]
mod tests {
    use super::corpus_files;

    // A real host symlink is needed to reproduce traversal through an
    // ancestor, which a native source fixture cannot model in Rust's walker.
    #[test]
    fn corpus_files_skip_directory_symlinks_and_include_nested_sources() {
        let root = tempfile::tempdir().expect("corpus directory");
        let fixture = root.path().join("core/tests/data/grep/fx/dl/real");
        std::fs::create_dir_all(&fixture).expect("create symlink fixture");
        let core = fixture.join("source.xsh");
        std::fs::write(&core, "let value = 1\n").expect("write source fixture");
        std::os::unix::fs::symlink("..", fixture.join("up")).expect("create ancestor symlink");
        std::os::unix::fs::symlink("core", root.path().join("examples"))
            .expect("create corpus directory symlink");
        let nested = root.path().join("dev/compat/smoke/source.xsh");
        std::fs::create_dir_all(nested.parent().unwrap())
            .expect("create nested development directory");
        std::fs::write(&nested, "let value = 2\n").expect("write nested development fixture");
        let dev = root.path().join("dev/tool.xsh");
        std::fs::write(&dev, "let value = 3\n").expect("write development fixture");
        let alias = root.path().join("dev/alias.xsh");
        std::os::unix::fs::symlink("tool.xsh", &alias).expect("create source file symlink");
        assert_eq!(corpus_files(root.path()), vec![core, alias, nested, dev]);
    }

    #[test]
    fn corpus_files_skip_generated_and_working_directories() {
        let root = tempfile::tempdir().expect("corpus directory");
        let source = root.path().join("tests/source.xsh");
        std::fs::create_dir_all(source.parent().unwrap()).expect("create source directory");
        std::fs::write(&source, "let value = 1\n").expect("write source fixture");
        for directory in ["target", ".git", ".claude", ".work"] {
            let ignored = root.path().join("tests").join(directory).join("scratch.xsh");
            std::fs::create_dir_all(ignored.parent().unwrap()).expect("create ignored directory");
            std::fs::write(ignored, "let value = 2\n").expect("write ignored fixture");
        }
        assert_eq!(corpus_files(root.path()), vec![source]);
    }
}
