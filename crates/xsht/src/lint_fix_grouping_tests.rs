use super::minimize_fix_grouping;
use xsh::diagnostic::{Diagnostic, FixHint};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::syntax::parser::Parser;

fn fix(source: &str, target: &str, occurrence: usize, replacement: &str) -> Diagnostic {
    fix_in_source(SourceId::new(0), source, target, occurrence, replacement)
}

fn fix_in_source(
    source_id: SourceId,
    source: &str,
    target: &str,
    occurrence: usize,
    replacement: &str,
) -> Diagnostic {
    let (start, _) = source
        .match_indices(target)
        .nth(occurrence)
        .expect("fix target");
    Diagnostic::warning("fix").with_fix_hint(FixHint::replacement(
        Span::new(source_id, start, start + target.len()),
        "rewrite",
        replacement,
    ))
}

fn minimized(source: &str, mut diagnostics: Vec<Diagnostic>) -> Vec<String> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = parsed
        .arena
        .statement_ids()
        .next()
        .map(|statement| parsed.arena.arena.stmt(statement).span.source_id);
    minimize_fix_grouping(source_id, source, &mut diagnostics);
    diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.fix_hints[0].replacement.clone().unwrap())
        .collect()
}

const SOURCE: &str = "let a = 1\nlet b = 2\nlet c = a + b\nlet d = c * 2\n";

// Fixes to separate text share one parse; each still loses exactly the
// parentheses that are redundant where it lands.
#[test]
fn separate_fixes_are_each_minimized() {
    assert_eq!(
        minimized(
            SOURCE,
            vec![
                fix(SOURCE, "1", 0, "(3)"),
                fix(SOURCE, "a + b", 0, "(a) * (b + 1)"),
                fix(SOURCE, "c", 1, "(c + 1)"),
            ],
        ),
        ["3", "a * (b + 1)", "(c + 1)"]
    );
}

#[test]
fn selected_module_groupings_are_minimized_with_an_empty_global_root() {
    let source = "proc compare(actual: Int) {\n    test.eq(actual, 3)?\n    match actual {\n        3 => test.ok(actual > 2)?,\n        _ => {},\n    }\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(7), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let module_statements = parsed.arena.statements;
    let mut program = parsed.arena;
    program.statements = Default::default();
    let view = xsh::frontend::check::CheckBundleView::for_module(
        &program,
        module_statements,
        None,
        Some(SourceId::new(7)),
    );
    let source_id = view
        .statement_ids()
        .next()
        .map(|statement| program.arena.stmt(statement).span.source_id);
    let mut diagnostics = vec![
        fix_in_source(
            SourceId::new(7),
            source,
            "test.eq(actual, 3)?",
            0,
            "assert (actual) == (3)",
        ),
        fix_in_source(
            SourceId::new(7),
            source,
            "test.ok(actual > 2)?",
            0,
            "{ assert (actual > 2) }",
        ),
    ];

    minimize_fix_grouping(source_id, source, &mut diagnostics);

    assert_eq!(
        diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.fix_hints[0].replacement.clone().unwrap())
            .collect::<Vec<_>>(),
        ["assert actual == 3", "{ assert actual > 2 }"]
    );
}

// A fix inside another's text cannot be applied beside it, so it is judged
// in a later pass, against the source without the outer fix.
#[test]
fn a_fix_inside_another_is_minimized_on_its_own() {
    assert_eq!(
        minimized(
            SOURCE,
            vec![
                fix(SOURCE, "a + b", 0, "(a) * (b + 1)"),
                fix(SOURCE, "a", 1, "(a)"),
                fix(SOURCE, "b", 1, "(b - 1)"),
            ],
        ),
        ["a * (b + 1)", "a", "(b - 1)"]
    );
}

// One fix that leaves text the parser rejects keeps its own replacement and
// does not stop the others from being minimized.
#[test]
fn a_fix_that_does_not_parse_leaves_the_others_minimized() {
    assert_eq!(
        minimized(
            SOURCE,
            vec![
                fix(SOURCE, "1", 0, "(3)"),
                fix(SOURCE, "2", 0, "(2"),
                fix(SOURCE, "c", 1, "(c)"),
            ],
        ),
        ["3", "(2", "c"]
    );
}
