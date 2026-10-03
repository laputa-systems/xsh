//! Formatting must not change what the linter reports.
//!
//! Each corpus is linted as written and after `xsht fmt` (or after layout-only
//! perturbations). Diagnostics are compared as multisets keyed by file, code,
//! and the ordinal of the first significant token at the reported location.
//! Grouping delimiters, separators, comments, and newlines are not significant, so the
//! anchor survives the layout changes the formatter is allowed to make.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::arena::{ArenaExprKind, ExprId};
use xsh::frontend::syntax::lexer::Lexer;
use xsh::frontend::syntax::parser::Parser;
use xsh::frontend::syntax::token::TokenTag;
use xsht::format::Formatter;

struct Anchor {
    offset: usize,
    class: String,
    location: String,
}

type DiagnosticSet = BTreeMap<(String, String), Vec<Anchor>>;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("workspace root")
}

fn copy_corpus(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("read corpus directory") {
        let entry = entry.expect("read corpus entry");
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let target = to.join(&*name);
        if entry.file_type().expect("corpus entry type").is_dir() {
            if matches!(&*name, "target" | ".git" | ".claude") {
                continue;
            }
            copy_corpus(&path, &target);
        } else if name.ends_with(".xsh") || name == "xsht-config.ini" {
            fs::create_dir_all(to).expect("create corpus copy directory");
            fs::copy(&path, &target).expect("copy corpus file");
        }
    }
}

fn xsh_files(root: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("read corpus directory") {
        let path = entry.expect("read corpus entry").path();
        if path.is_dir() {
            xsh_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "xsh") {
            output.push(path);
        }
    }
}

fn xsht(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(args)
        .current_dir(root)
        .env_remove("XSH_MODULE_PATH")
        .output()
        .expect("run xsht")
}

fn significant(tag: TokenTag) -> bool {
    !matches!(
        tag,
        TokenTag::Comment | TokenTag::Newline | TokenTag::LParen | TokenTag::RParen | TokenTag::LBrace | TokenTag::RBrace | TokenTag::Comma | TokenTag::Semicolon | TokenTag::Eof
    )
}

/// Byte offset of the first significant token at or after a 1-based line and
/// character column, plus a spelling-independent class for that token: names
/// keep their text, while literals collapse to one class because the formatter
/// owns their quoting, escapes, and block layout.
fn token_anchor(source: &str, line: usize, column: usize) -> (usize, String) {
    let line_start = source.split_inclusive('\n').take(line.saturating_sub(1)).map(str::len).sum::<usize>();
    let offset = source[line_start..].char_indices().nth(column.saturating_sub(1)).map_or(source.len(), |(index, _)| line_start + index);
    let tokens = Lexer::new(SourceId::new(0), source).lex_compact().token_table;
    let text = |index: usize| {
        let start = tokens.start_at(index).expect("token start");
        source[start..tokens.end_at(index, source).unwrap_or(start)].trim_start_matches('$').to_owned()
    };
    for index in 0..tokens.len() {
        let tag = tokens.tag_at(index).expect("token tag");
        let start = tokens.start_at(index).expect("token start");
        if !significant(tag) || start < offset {
            continue;
        }
        let class = match tag {
            TokenTag::String | TokenTag::PathString | TokenTag::GlobString | TokenTag::FmtString | TokenTag::PathFmtString
            | TokenTag::Bytes | TokenTag::Regex | TokenTag::Int | TokenTag::Float | TokenTag::Duration => "literal".to_owned(),
            TokenTag::DollarLBrace if index + 1 < tokens.len() => text(index + 1),
            _ => text(index),
        };
        return (start, class);
    }
    (source.len(), String::new())
}

fn lint_tree(root: &Path) -> DiagnosticSet {
    let output = xsht(root, &["lint"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.code().is_some_and(|code| code <= 1), "xsht lint failed in {}:\n{stderr}", root.display());
    let prefix = format!("{}/", root.canonicalize().expect("canonical corpus root").display());
    let mut diagnostics = DiagnosticSet::new();
    let mut lines = stderr.lines();
    while let Some(line) = lines.next() {
        let Some((head, _)) = line.split_once("]: ") else { continue };
        let Some((severity, code)) = head.split_once('[') else { continue };
        if severity.is_empty() || !severity.chars().all(|ch| ch.is_ascii_lowercase()) {
            continue;
        }
        let Some(location) = lines.next() else { break };
        let mut parts = location.trim().rsplitn(3, ':');
        let (Some(column), Some(line_number), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let (Ok(line_number), Ok(column)) = (line_number.parse::<usize>(), column.parse::<usize>()) else { continue };
        let path = Path::new(path).canonicalize().map_or_else(|_| path.to_owned(), |path| path.display().to_string());
        let path = path.strip_prefix(&prefix).unwrap_or(&path).to_owned();
        let source = fs::read_to_string(root.join(&path)).unwrap_or_default();
        let (offset, class) = token_anchor(&source, line_number, column);
        diagnostics.entry((path.clone(), format!("{severity}[{code}]"))).or_default().push(Anchor { offset, class, location: format!("{path}:{line_number}:{column}") });
    }
    for anchors in diagnostics.values_mut() {
        anchors.sort_by_key(|anchor| anchor.offset);
    }
    diagnostics
}

/// Formatting keeps nodes in source order, so per file and code the ordered
/// anchor classes identify the reported nodes without depending on positions.
fn assert_same_diagnostics(corpus: &str, expected: &DiagnosticSet, actual: &DiagnosticSet) {
    let mut report = Vec::new();
    for key in expected.keys().chain(actual.keys().filter(|key| !expected.contains_key(*key))) {
        let before = expected.get(key).map_or(&[][..], Vec::as_slice);
        let after = actual.get(key).map_or(&[][..], Vec::as_slice);
        let classes = |anchors: &[Anchor]| anchors.iter().map(|anchor| anchor.class.clone()).collect::<Vec<_>>();
        if classes(before) != classes(after) {
            let describe = |anchors: &[Anchor]| anchors.iter().map(|anchor| format!("{} `{}`", anchor.location, anchor.class)).collect::<Vec<_>>();
            report.push(format!("{}: before {:?}, after {:?}", key.1, describe(before), describe(after)));
        }
    }
    assert!(report.is_empty(), "{corpus}: lint diagnostics depend on layout in {} file/code group(s):\n{}", report.len(), report.join("\n"));
}

/// Lints `corpus` as written and after `xsht fmt`, requiring equal diagnostics.
fn assert_formatting_preserves_lints(name: &str, corpus: &Path) -> usize {
    let scratch = TempDir::new().expect("scratch directory");
    let original = scratch.path().join("original");
    let formatted = scratch.path().join("formatted");
    copy_corpus(corpus, &original);
    copy_corpus(corpus, &formatted);
    let fmt = xsht(&formatted, &["fmt"]);
    assert!(fmt.status.success(), "xsht fmt failed for {name}:\n{}", String::from_utf8_lossy(&fmt.stderr));
    let before = lint_tree(&original);
    assert_same_diagnostics(name, &before, &lint_tree(&formatted));
    before.values().map(Vec::len).sum()
}

#[derive(Clone)]
struct Insertion {
    offset: usize,
    // Closers sort before openers at one offset so nested groups stay balanced.
    order: u8,
    text: &'static str,
    // Source bytes replaced by `text`; equal to `offset` for a pure insertion.
    end: usize,
}

fn apply(source: &str, edits: &[Insertion]) -> String {
    let mut edits = edits.to_vec();
    edits.sort_by_key(|edit| (edit.offset, edit.order));
    let mut output = String::with_capacity(source.len() + edits.len() * 2);
    let mut cursor = 0;
    for edit in edits {
        output.push_str(&source[cursor..edit.offset]);
        output.push_str(edit.text);
        cursor = edit.end;
    }
    output.push_str(&source[cursor..]);
    output
}

/// Adds redundant parentheses around operands, breaks lines after binary
/// operators, doubles existing blank lines, and joins bracketed lines. Only edits that the formatter
/// erases are kept, so the perturbed file is the same program modulo layout.
fn perturb_layout(source: &str) -> Option<String> {
    let canonical = Formatter::new().format_source(SourceId::new(0), source);
    if !canonical.diagnostics.is_empty() {
        return None;
    }
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    if !parsed.diagnostics.is_empty() {
        return None;
    }
    let arena = &parsed.arena.arena;
    let in_source = |span: xsh::frontend::source::Span| span.source_id == SourceId::new(0) && span.start() < span.end() && span.end() <= source.len();
    let mut parens = Vec::new();
    let mut breaks = Vec::new();
    for index in 0..arena.expr_tags.len() {
        let ArenaExprKind::Binary { left, right, .. } = arena.expr(ExprId::from_index(index)).kind else { continue };
        for operand in [left, right] {
            let span = arena.expr(operand).span;
            // Implicit-receiver shorthand such as `.name` must lead its stage.
            if in_source(span) && !source[span.range()].starts_with('.') {
                parens.push(vec![
                    Insertion { offset: span.start(), order: 1, text: "(", end: span.start() },
                    Insertion { offset: span.end(), order: 0, text: ")", end: span.end() },
                ]);
            }
        }
        let span = arena.expr(right).span;
        if in_source(span) && in_source(arena.expr(left).span) {
            breaks.push(vec![Insertion { offset: span.start(), order: 2, text: "\n", end: span.start() }]);
        }
    }
    let tokens = Lexer::new(SourceId::new(0), source).lex_compact().token_table;
    let mut blank_lines = Vec::new();
    for index in 1..tokens.len() {
        if tokens.tag_at(index - 1) == Some(TokenTag::Newline) && tokens.tag_at(index) == Some(TokenTag::Newline) {
            let offset = tokens.start_at(index).expect("token start");
            blank_lines.push(vec![Insertion { offset, order: 3, text: "\n", end: offset }]);
        }
    }
    // Join lines inside parentheses and brackets, where breaks are layout only.
    let mut joins = Vec::new();
    let mut depth = 0usize;
    for index in 0..tokens.len() {
        match tokens.tag_at(index) {
            Some(TokenTag::LParen | TokenTag::LBracket) => depth += 1,
            Some(TokenTag::RParen | TokenTag::RBracket) => depth = depth.saturating_sub(1),
            Some(TokenTag::Newline) if depth > 0 && index > 0 && tokens.tag_at(index - 1) != Some(TokenTag::Comment) => {
                let offset = tokens.start_at(index).expect("token start");
                let end = offset + source[offset..].find(|ch: char| !ch.is_whitespace()).unwrap_or(source.len() - offset);
                joins.push(vec![Insertion { offset, order: 4, text: " ", end }]);
            }
            _ => {}
        }
    }
    let mut accepted = Vec::new();
    for groups in [parens, breaks, blank_lines, joins] {
        accept_layout_edits(source, &canonical.formatted, &mut accepted, &groups, &mut 48);
    }
    (!accepted.is_empty()).then(|| apply(source, &accepted))
}

/// Keeps the edit groups whose application still formats to `canonical`,
/// bisecting rejected groups within a fixed formatter budget. A group, such as
/// a pair of parentheses, is accepted or rejected as a whole.
fn accept_layout_edits(source: &str, canonical: &str, accepted: &mut Vec<Insertion>, groups: &[Vec<Insertion>], budget: &mut usize) {
    if groups.is_empty() || *budget == 0 {
        return;
    }
    *budget -= 1;
    let mut trial = accepted.clone();
    trial.extend(groups.iter().flatten().cloned());
    let output = Formatter::new().format_source(SourceId::new(0), &apply(source, &trial));
    if output.diagnostics.is_empty() && output.formatted == canonical {
        *accepted = trial;
        return;
    }
    if groups.len() == 1 {
        return;
    }
    let (left, right) = groups.split_at(groups.len() / 2);
    accept_layout_edits(source, canonical, accepted, left, budget);
    accept_layout_edits(source, canonical, accepted, right, budget);
}

#[test]
fn formatting_preserves_lints_on_the_repository_corpus() {
    let count = assert_formatting_preserves_lints("repository", &workspace_root());
    eprintln!("repository corpus: {count} diagnostic(s), layout-independent");
}

#[test]
fn layout_perturbation_preserves_lints_on_the_repository_corpus() {
    let scratch = TempDir::new().expect("scratch directory");
    let original = scratch.path().join("original");
    let perturbed = scratch.path().join("perturbed");
    copy_corpus(&workspace_root(), &original);
    copy_corpus(&workspace_root(), &perturbed);
    let mut files = Vec::new();
    xsh_files(&perturbed, &mut files);
    let mut changed = 0;
    for file in files {
        let source = fs::read_to_string(&file).expect("read corpus file");
        if let Some(text) = perturb_layout(&source) {
            fs::write(&file, text).expect("write perturbed file");
            changed += 1;
        }
    }
    assert!(changed > 0, "no corpus file accepted a layout perturbation");
    assert_same_diagnostics("perturbed repository", &lint_tree(&original), &lint_tree(&perturbed));
    eprintln!("perturbed {changed} repository file(s)");
}

/// `xsht fmt` then `xsht lint --fix` applies the same rewrites as the reverse
/// order: after a final format both orders agree up to blank lines.
#[test]
fn lint_fix_commutes_with_formatting_on_corpus_files() {
    // The fixture reproduces the two shapes that once reported only after
    // formatting (grouped guard operand, grouped literal chain), taken from
    // dev/system_report_check.xsh and tests/xsh/system-report.xsh.
    const FIXTURE: &str = "lint-fix-order.xsh";
    const FILES: &[&str] = &[FIXTURE, "tests/xsh/block-strings.xsh", "tests/xsh/stdlib/text.xsh", "showcase/tokei.xsh"];
    let scratch = TempDir::new().expect("scratch directory");
    let format_first = scratch.path().join("format-first");
    let fix_first = scratch.path().join("fix-first");
    copy_corpus(&workspace_root(), &format_first);
    copy_corpus(&workspace_root(), &fix_first);
    fs::write(
        format_first.join(FIXTURE),
        "pure has_id(id: Str?) -> Bool {\n  if id == null or (id) == \"\" {\n    return false\n  }\n  true\n}\nlet value = \"  root=private  quiet  \" + \"\\n\"\nassert value == (\"  root=private  quiet  \" + \"\\n\")\nassert has_id(\"linux\")\n",
    )
    .expect("write fixture");
    let mut fixed_files = 0;
    for file in FILES {
        let source = fs::read_to_string(format_first.join(file)).expect("read corpus file");
        let perturbed = perturb_layout(&source).unwrap_or(source);
        for root in [&format_first, &fix_first] {
            fs::write(root.join(file), &perturbed).expect("write perturbed file");
        }
        for (root, steps) in [(&format_first, &[&["fmt", file][..], &["lint", "--fix", file], &["fmt", file]][..]), (&fix_first, &[&["lint", "--fix", file][..], &["fmt", file]])] {
            for args in steps {
                let output = xsht(root, args);
                assert!(output.status.code().is_some_and(|code| code <= 1), "xsht {args:?} failed:\n{}", String::from_utf8_lossy(&output.stderr));
            }
        }
        // The formatter keeps an author's blank lines, so a blank line it added
        // after a multiline block survives that block becoming one line.
        let without_blank_lines = |text: &str| text.lines().filter(|line| !line.trim().is_empty()).collect::<Vec<_>>().join("\n");
        let formatted = fs::read_to_string(format_first.join(file)).expect("read format-first result");
        let fixed = fs::read_to_string(fix_first.join(file)).expect("read fix-first result");
        assert_eq!(without_blank_lines(&formatted), without_blank_lines(&fixed), "{file}: fix order changed the result");
        fixed_files += usize::from(without_blank_lines(&formatted) != without_blank_lines(&Formatter::new().format_source(SourceId::new(0), &perturbed).formatted));
    }
    assert!(fixed_files > 0, "no corpus file received a lint fix");
}

/// The corpus as it stood before the formatter rewrote it, when history is available.
#[test]
fn formatting_preserves_lints_on_the_pre_format_corpus() {
    let root = workspace_root();
    let archive = Command::new("git").args(["archive", "b1f984c8^"]).current_dir(&root).output();
    let Ok(archive) = archive else { return };
    if !archive.status.success() {
        return;
    }
    let scratch = TempDir::new().expect("scratch directory");
    let mut tar = Command::new("tar")
        .args(["-x", "-C"])
        .arg(scratch.path())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("start tar");
    std::io::Write::write_all(tar.stdin.as_mut().expect("tar stdin"), &archive.stdout).expect("write archive");
    assert!(tar.wait().expect("wait for tar").success());
    // The checker has since rejected these unsound sites; migrate them as the
    // live tree was, so the historical corpus still checks.
    for (file, from, to) in [
        ("tests/xsh/collections.xsh", "item.path.display\n", "item.path.display()\n"),
        ("tests/xsh/stdlib/args.xsh", "parsed.target.name()", "parsed.target?.name()"),
    ] {
        let path = scratch.path().join(file);
        if let Ok(text) = fs::read_to_string(&path) {
            fs::write(&path, text.replace(from, to)).expect("migrate historical corpus file");
        }
    }
    let count = assert_formatting_preserves_lints("pre-format", scratch.path());
    eprintln!("pre-format corpus: {count} diagnostic(s), layout-independent");
}

#[test]
fn formatting_preserves_lints_on_package_corpus() {
    let root = std::env::var_os("XSH_PACKAGE_CORPUS").map_or_else(|| workspace_root().join("../packages"), PathBuf::from);
    if !root.is_dir() {
        return;
    }
    let count = assert_formatting_preserves_lints("packages", &root);
    eprintln!("package corpus: {count} diagnostic(s), layout-independent");
}

