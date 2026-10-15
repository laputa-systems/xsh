use super::{LintOptions, Linter};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use xsh::frontend::check::{CheckBundleView, Checker};
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaProgramBuilder};
use xsh::frontend::syntax::parser::Parser;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn lint_source_only(source: &str, only: Vec<DiagnosticCode>) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            only: Some(only),
            ..LintOptions::default()
        },
    )
    .diagnostics
}

fn lint_checked(source: &str, only: Vec<DiagnosticCode>) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            checked_output: Some(std::sync::Arc::new(checked)),
            only: Some(only),
            ..LintOptions::default()
        },
    )
    .diagnostics
}

fn assignment_sensitive_codes() -> Vec<DiagnosticCode> {
    vec![
        DiagnosticCode::LintPreferComparisonChain,
        DiagnosticCode::LintDollarInExpressionString,
        DiagnosticCode::LintMissingFPrefix,
    ]
}

fn pipeline_expression_types(source: &str) -> BTreeMap<xsh::frontend::source::Span, super::Type> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "{:?}",
        checked.diagnostics
    );
    let statements = parsed
        .arena
        .arena
        .stmt_ids(parsed.arena.statements)
        .collect::<Vec<_>>();
    let call_initializers = statements
        .iter()
        .filter_map(|&statement| match parsed.arena.arena.stmt(statement).kind {
            xsh::frontend::syntax::arena::ArenaStmtKind::Let {
                initializer: xsh::frontend::syntax::arena::ArenaExprOrRun::Expr(expression),
                ..
            } if matches!(
                parsed.arena.arena.expr(expression).kind,
                xsh::frontend::syntax::arena::ArenaExprKind::Call { .. }
            ) => Some(expression),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [input, output, ..] = call_initializers.as_slice() else {
        panic!("expected call initializers, got {call_initializers:?}");
    };
    BTreeMap::from([
        (
            parsed.arena.arena.expr(*input).span,
            checked.expr_types[&parsed.arena.arena.expr(*input).span].clone(),
        ),
        (
            parsed.arena.arena.expr(*output).span,
            checked.expr_types[&parsed.arena.arena.expr(*output).span].clone(),
        ),
    ])
}

fn lint_pair_and_comparison(
    source: &str,
    expression_types: &BTreeMap<xsh::frontend::source::Span, super::Type>,
) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let statements = parsed
        .arena
        .arena
        .stmt_ids(parsed.arena.statements)
        .collect::<Vec<_>>();
    let call_initializers = statements
        .iter()
        .filter_map(|&statement| match parsed.arena.arena.stmt(statement).kind {
            xsh::frontend::syntax::arena::ArenaStmtKind::Let {
                initializer: xsh::frontend::syntax::arena::ArenaExprOrRun::Expr(expression),
                ..
            } if matches!(
                parsed.arena.arena.expr(expression).kind,
                xsh::frontend::syntax::arena::ArenaExprKind::Call { .. }
            ) => Some(expression),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [input, output, ..] = call_initializers.as_slice() else {
        panic!("expected call initializers, got {call_initializers:?}");
    };
    let input_span = parsed.arena.arena.expr(*input).span;
    let output_span = parsed.arena.arena.expr(*output).span;
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            only: Some(vec![
                DiagnosticCode::LintPreferValuePipeline,
                DiagnosticCode::LintPreferComparisonChain,
            ]),
            expr_types: BTreeMap::from([
                (input_span, expression_types[&input_span].clone()),
                (output_span, expression_types[&output_span].clone()),
            ]),
            ..LintOptions::default()
        },
    )
    .diagnostics
}

#[test]
fn late_assignments_resolve_candidates_in_original_diagnostic_order() {
    let source = "let low = 1\nlet middle = 2\nlet high = 3\nlet comparison = low < middle and middle < high\nlet dollar = \"$later\"\nlet formatted = \"{later}\"\nlater = 4\n";
    let diagnostics = lint_source_only(source, assignment_sensitive_codes());

    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.unwrap())
            .collect::<Vec<_>>(),
        [
            DiagnosticCode::LintPreferComparisonChain,
            DiagnosticCode::LintDollarInExpressionString,
            DiagnosticCode::LintMissingFPrefix,
        ],
        "{diagnostics:?}"
    );
    assert_eq!(diagnostics[0].fix_hints.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[1].fix_hints.is_empty(), "{diagnostics:?}");
    assert_eq!(diagnostics[2].fix_hints.len(), 1, "{diagnostics:?}");
}

#[test]
fn a_dropped_outer_chain_does_not_suppress_a_safe_inner_chain() {
    let source = "let a = 1\nlet b = 2\nlet c = 3\nlet d = 4\nlet comparison = (a < b and b < c) and c < d\nproc mutate() {\n  c = 5\n}\n";
    let diagnostics = lint_source_only(
        source,
        vec![DiagnosticCode::LintPreferComparisonChain],
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        &source[diagnostics[0].labels[0].span.range()],
        "a < b and b < c"
    );
    assert_eq!(diagnostics[0].fix_hints.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].fix_hints[0].replacement.as_deref(),
        Some("a < b < c")
    );
}

#[test]
fn statement_pair_reordering_keeps_candidate_offsets_aligned() {
    let source = "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet input = inner(2)\nlet output = outer(10, value: input)\nlet low = 1\nlet middle = 2\nlet high = 3\nlet comparison = low < middle and middle < high\n";
    let expression_types = pipeline_expression_types(source);
    let diagnostics = lint_pair_and_comparison(source, &expression_types);

    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.unwrap())
            .collect::<Vec<_>>(),
        [
            DiagnosticCode::LintPreferValuePipeline,
            DiagnosticCode::LintPreferComparisonChain,
        ],
        "{diagnostics:?}"
    );
    assert_eq!(diagnostics[0].fix_hints.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[1].fix_hints.len(), 1, "{diagnostics:?}");

    let assigned = lint_pair_and_comparison(
        &format!("{source}proc mutate() {{\n  var middle = 2\n  middle = 5\n}}\n"),
        &expression_types,
    );
    assert_eq!(
        assigned
            .iter()
            .map(|diagnostic| diagnostic.code.unwrap())
            .collect::<Vec<_>>(),
        [DiagnosticCode::LintPreferValuePipeline],
        "{assigned:?}"
    );
    assert_eq!(assigned[0].fix_hints.len(), 1, "{assigned:?}");
}

#[test]
fn only_root_names_from_statement_regions_suppress_name_warnings() {
    let source = "root.field = 1\nitems[0] = 2\ne\"XSH_ASSIGNMENT_FACT\" = 3\nlet result = try {\n  nested = 4\n  0\n}\natomically replace output as temporary {\n  repeat 1 times { atomic_written = 6 }\n}\nwait until ready within 5s backoff 1s..4s\nlet root_text = \"$root\"\nlet items_text = \"$items\"\nlet environment_text = \"$XSH_ASSIGNMENT_FACT\"\nlet nested_text = \"$nested\"\nlet atomic_text = \"$atomic_written\"\nlet delay_text = \"$delay\"\nproc show() {\n  defer { deferred = 5 }\n  let deferred_text = \"$deferred\"\n}\n";
    let diagnostics = lint_source_only(
        source,
        vec![DiagnosticCode::LintDollarInExpressionString],
    );
    let checked_diagnostics = lint_checked(
        source,
        vec![DiagnosticCode::LintDollarInExpressionString],
    );

    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
    assert_eq!(checked_diagnostics.len(), 3, "{checked_diagnostics:?}");
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.labels[0].span.range()])
            .collect::<Vec<_>>(),
        ["$root", "$items", "$atomic_written"]
    );
    assert_eq!(
        checked_diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.labels[0].span.range()])
            .collect::<Vec<_>>(),
        ["$root", "$items", "$atomic_written"]
    );
}

#[test]
fn scope_visibility_is_observed_at_the_original_expression_hook() {
    let source = "proc show(local: Str) {\n  let dollar = \"$local\"\n  let formatted = \"{local}\"\n}\n";
    let diagnostics = lint_source_only(
        source,
        vec![
            DiagnosticCode::LintDollarInExpressionString,
            DiagnosticCode::LintMissingFPrefix,
        ],
    );

    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.unwrap())
            .collect::<Vec<_>>(),
        [
            DiagnosticCode::LintDollarInExpressionString,
            DiagnosticCode::LintMissingFPrefix,
        ]
    );
}

#[test]
fn checked_assignment_facts_survive_an_error_output() {
    let source = "name = 2\nlet text = \"$name\"\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error),
        "the fixture must exercise partial checked output"
    );
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            checked_output: Some(std::sync::Arc::new(checked)),
            only: Some(vec![DiagnosticCode::LintDollarInExpressionString]),
            ..LintOptions::default()
        },
    )
    .diagnostics;

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        &source[diagnostics[0].labels[0].span.range()],
        "$name"
    );
    selected_statement_ranges_keep_assignment_facts_separate();
    imported_same_spelling_assignments_do_not_train_the_root();
}

#[test]
fn checked_empty_program_without_source_identity_remains_lintable() {
    let program = ArenaProgram::default();
    let checked = Checker::check_arena(&program, "");
    let output = Linter::lint(
        &program,
        "",
        LintOptions {
            checked_output: Some(Arc::new(checked)),
            ..LintOptions::default()
        },
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

fn selected_statement_ranges_keep_assignment_facts_separate() {
    let source = "name = 1\nlet text = \"$name\"\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let first = xsh::frontend::syntax::arena::ArenaRange {
        start: parsed.arena.statements.start,
        len: 1,
    };
    let second = xsh::frontend::syntax::arena::ArenaRange {
        start: first.start + first.len,
        len: 1,
    };
    let first_view = CheckBundleView::for_root(
        &parsed.arena,
        first,
        None,
        Some(SourceId::new(0)),
    );
    let first_checked = Checker::check_bundle_view_with_type_program(
        &first_view,
        source,
        xsh::frontend::check::CheckOptions::default(),
        Arc::new(parsed.arena.clone()),
    );
    assert_eq!(
        first_checked
            .assignment_names(SourceId::new(0), first)
            .map(|names| {
                names
                    .iter()
                    .map(|name| name.as_str().as_str().to_owned())
                    .collect::<Vec<_>>()
            }),
        Some(vec!["name".to_owned()]),
    );
    assert_eq!(
        first_checked.assignment_names(SourceId::new(0), second),
        None,
    );

    let second_view = CheckBundleView::for_root(
        &parsed.arena,
        second,
        None,
        Some(SourceId::new(0)),
    );
    let second_checked = Checker::check_bundle_view_with_type_program(
        &second_view,
        source,
        xsh::frontend::check::CheckOptions::default(),
        Arc::new(parsed.arena.clone()),
    );
    assert_eq!(
        second_checked
            .assignment_names(SourceId::new(0), second)
            .map(BTreeSet::is_empty),
        Some(true),
    );
    assert_eq!(
        second_checked.assignment_names(SourceId::new(0), first),
        None,
    );
    let diagnostics = Linter::lint_view(
        &second_view,
        source,
        LintOptions {
            checked_output: Some(Arc::new(second_checked)),
            only: Some(vec![DiagnosticCode::LintDollarInExpressionString]),
            ..LintOptions::default()
        },
    )
    .diagnostics;

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn imported_same_spelling_assignments_do_not_train_the_root() {
    let root_source = "use helper\nlet text = \"$shared\"\n";
    let module_source = "shared = 1\n";
    let root_id = SourceId::new(0);
    let module_id = SourceId::new(1);
    let mut builder = ArenaProgramBuilder::with_token_capacity(64);
    let root = Parser::parse_source_into_arena_builder(root_id, root_source, &mut builder);
    let use_statement = builder
        .ast_arena()
        .stmt_ids(root.statements)
        .next()
        .expect("root import statement");
    let xsh::frontend::syntax::arena::ArenaStmtKind::Use(use_id) =
        builder.ast_arena().stmt(use_statement).kind
    else {
        panic!("first root statement is the helper import");
    };
    builder.set_use_resolved(use_id, Arc::from("helper-key"));
    let helper_name = builder.name("helper");
    let module = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
    builder.push_arena_module(
        "helper-key".to_string(),
        helper_name,
        module.statements,
    );
    let program = builder.finish();
    let view = CheckBundleView::for_root(&program, root.statements, None, Some(root_id));
    let checked = Checker::check_bundle_view_with_type_program(
        &view,
        root_source,
        xsh::frontend::check::CheckOptions::default(),
        Arc::new(program.clone()),
    );
    let diagnostics = Linter::lint_view(
        &view,
        root_source,
        LintOptions {
            checked_output: Some(Arc::new(checked)),
            only: Some(vec![DiagnosticCode::LintDollarInExpressionString]),
            ..LintOptions::default()
        },
    )
    .diagnostics;

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
