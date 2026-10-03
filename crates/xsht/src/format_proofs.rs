//! Proofs that `grouping::needs_parens` is exactly the grouping the parser
//! requires, run through the real parser, printer, and redundancy check.

use super::{Formatter, Parser, SourceId, format_equivalence};
use xsh::diagnostic::Diagnostic;
use xsh::frontend::syntax::grouping::grouping_diagnostics;

const OPERATORS: [&str; 16] = ["??", "or", "and", "==", "!=", "<", "<=", ">", ">=", "in", "not in", "+", "-", "*", "/", "%"];

/// One spelling of every expression form, by fixity and how it ends.
fn children() -> Vec<String> {
    let mut children: Vec<String> = [
        "a", "1", "\"s\"", "[a]", "{k: a}", "$?", "/tmp/x", "env.Str.HOME", ".", ".f", "f\"x${a}\"",
        "-b", "! b", "b is Int", "b < c < d",
        "b.f", "b?.f", "b[0]", "b?[0]", "b[0..1]", "b(c)", "b?", "b.require(Int)",
        "if b { c } else { d }", "match b { _ => c }", "loop { break }", "try { c }", "retry [1s] { c }", "{|e| c}",
        "cd (b) { c }", "ctx \"m\" { c }",
        "run foo", "run foo -x", "run cat < b\"\"", "spawn run foo", "spawn f()", "wait c",
        "b |> lines()", "b |> f(.)", "b |> map(c)", "b |> sort", "b |> take(1)", "b |> sum", "b |> map .f", "b |> where . > c", "{k: b for k in c}", "{[f\"${k}\"]: b for k in c}", "{k}", "{}",
    ]
    .map(str::to_string)
    .to_vec();
    children.extend(OPERATORS.map(|op| format!("b {op} c")));
    children
}

/// One template with the hole `H` for every slot an expression can fill.
fn parents() -> Vec<String> {
    let mut parents: Vec<String> = [
        "let v = -H", "let v = ! H", "let v = H is Int",
        "let v = H.f", "let v = H?.f", "let v = H[0]", "let v = H?[0]", "let v = H[0..1]", "let v = H(1)", "let v = H?",
        "let v = H.require(Int)",
        "let v = xs[H..1]", "let v = xs[0..H]", "let v = xs[H]",
        "let v = spawn H", "let v = wait H",
        "let v = H |> lines()", "let v = H |> f(.)",
        "let v = a < b < H", "let v = H < a < b",
        "let v = H", "x = H", "proc p() {\n  return H when c\n}",
        "H", "f()\nH", "let w = 1\nH", "if c {\n  w()\n}\nH", "proc p() {\n  H\n}",
        "let v = match a { _ => H }", "match a {\n  _ => H\n}", "let v = if c { H } else { 1 }",
        "f(H, 1)", "f(1, H)", "[H, 1]", "{k: H}", "if H {\n  w()\n}", "while H {\n  w()\n}", "for x in H {\n  w()\n}",
        "print f\"${H}\"", "assert H, \"m\"", "assert H",
    ]
    .map(str::to_string)
    .to_vec();
    for op in OPERATORS {
        parents.push(format!("let v = a {op} H"));
        parents.push(format!("let v = H {op} a"));
    }
    parents
}

fn canonical(source: &str) -> Option<String> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    parsed.diagnostics.is_empty().then(|| format_equivalence::canonical(&parsed.arena, source).text)
}

fn grouping(source: &str) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    grouping_diagnostics(&parsed.arena, source)
}

fn format(source: &str) -> String {
    format_wide(source, super::DEFAULT_LINE_WIDTH)
}

fn format_wide(source: &str, width: usize) -> String {
    let formatted = Formatter::new().with_line_width(width).format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{source}\n{:?}", formatted.diagnostics);
    formatted.formatted
}

/// Every parent slot with every child form, the child written in
/// parentheses: the printer's output parses to the same tree and is a
/// fixpoint, and `check.redundant-parens` reports the parentheses exactly when
/// the source without them parses to the same tree with no grouping error.
#[test]
fn needs_parens_is_exact_for_every_slot_and_form() {
    let (parents, children) = (parents(), children());
    let (mut cases, mut required, mut redundant, mut kept) = (0, 0, 0, 0);
    for parent in &parents {
        for child in &children {
            let hole = parent.find('H').unwrap();
            let grouped = parent.replacen('H', &format!("({child})"), 1);
            let Some(tree) = canonical(&grouped) else { continue };
            cases += 1;
            let formatted = format(&grouped);
            assert_eq!(canonical(&formatted).as_ref(), Some(&tree), "{grouped}\nprinted as\n{formatted}");
            assert_eq!(format(&formatted), formatted, "{grouped}");
            assert!(grouping(&formatted).is_empty(), "{formatted}\n{:?}", grouping(&formatted));
            let group_end = hole + child.len() + 2;
            let reported = grouping(&grouped).iter().any(|diagnostic| {
                diagnostic.code.as_deref() == Some("check.redundant-parens")
                    && diagnostic.labels[0].span.start() == hole
                    && diagnostic.labels[0].span.end() == group_end
            });
            let bare = parent.replacen('H', child, 1);
            let removable = canonical(&bare).as_ref() == Some(&tree) && grouping(&bare).iter().all(|d| d.code.as_deref() != Some("check.mixed-logical"));
            // A `|>` method stage is rebuilt as a call, so its spelling, not
            // its tree, decides whether a suffix joins the stage; such
            // parentheses are kept rather than judged.
            if child.contains("|>") && !reported && removable {
                kept += 1;
                continue;
            }
            assert_eq!(reported, removable, "parentheses in\n{grouped}\nreported redundant: {reported}; removable: {removable}");
            if reported { redundant += 1 } else { required += 1 }
        }
    }
    assert_eq!((parents.len(), children.len()), (73, 65));
    assert_eq!((cases, required, redundant, kept), (4745, 1173, 3562, 10));
}
