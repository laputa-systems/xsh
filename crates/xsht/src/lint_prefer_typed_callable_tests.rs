use super::super::{LintOptions, Linter};
use xsh::diagnostic::{Diagnostic, DiagnosticCode};
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

/// Runs the whole linter with the rule switched on, as `xsht lint` does with
/// `[lint] prefer-typed-callables = true`, and keeps this rule's reports.
fn lint_with(source: &str, enabled: bool) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let options = LintOptions {
        expr_types: checked.expr_types,
        function_return_types: checked.function_return_types,
        function_effect_facts: checked.function_effect_facts,
        function_effect_facts_checked: true,
        prefer_typed_callables: enabled,
        ..LintOptions::default()
    };
    Linter::lint(&parsed.arena, source, options)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferTypedCallable))
        .collect()
}

fn lint(source: &str) -> Vec<Diagnostic> {
    lint_with(source, true)
}

const BUILDERS: &str = "proc debug_build(root: Path) [fs, error] -> Result[Unit] {\n  root.mkdir()\n}\n\nproc release_build(root: Path) {\n  root.mkdir()\n  run make -C $root\n}\n\n";

#[test]
fn proc_parameter_passed_one_signature_names_its_callable_type() {
    let source = format!(
        "{BUILDERS}proc build_with(build: Proc, root: Path) -> Result[Unit, Error] {{\n  let _ = build.call(root)?\n}}\n\nbuild_with(debug_build, p\"/a\")\nbuild_with(root: p\"/b\", build: release_build)\n"
    );
    let diagnostics = lint(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(
        diagnostic.message,
        "every call of `build_with` passes a function with one signature as `build`"
    );
    let label = &diagnostic.labels[0];
    assert_eq!(&source[label.span.range()], "Proc");
    // The clause is the union of what the passed functions need, and an
    // inferred return type is spelled as checked.
    assert_eq!(
        label.message.as_deref(),
        Some("the callable type is `proc(root: Path) [fs, process, error] -> Result[Unit]`")
    );
    assert!(diagnostic.fix_hints.is_empty(), "{:?}", diagnostic.fix_hints);
    assert!(lint_with(&source, false).is_empty());
}

#[test]
fn pure_parameter_passed_one_signature_names_its_callable_type() {
    let source = "pure double(n: Int) -> Int {\n  n * 2\n}\n\npure triple(n: Int) -> Int {\n  n * 3\n}\n\npure apply(scale: Pure, n: Int) -> Result[Int] {\n  scale.call(n).require(Int)\n}\n\nprint ${apply(double, 1)? + apply(triple, 1)?}\n";
    let diagnostics = lint(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].labels[0].message.as_deref(),
        Some("the callable type is `pure(n: Int) -> Int`")
    );
}

#[test]
fn calls_that_pass_different_signatures_are_not_reported() {
    let source = format!(
        "{BUILDERS}proc other(dir: Path) [fs, error] -> Result[Unit] {{\n  dir.mkdir()\n}}\n\nproc build_with(build: Proc, root: Path) -> Result[Unit, Error] {{\n  let _ = build.call(root)?\n}}\n\nbuild_with(debug_build, p\"/a\")\nbuild_with(other, p\"/b\")\n"
    );
    assert!(lint(&source).is_empty());
}

#[test]
fn a_caller_the_traversal_cannot_see_keeps_the_dynamic_type() {
    // Exported: other modules call it.
    let exported = format!(
        "{BUILDERS}## Builds.\nexport proc build_with(build: Proc, root: Path) -> Result[Unit, Error] {{\n  let _ = build.call(root)?\n}}\n\nbuild_with(debug_build, p\"/a\")\n"
    );
    assert!(lint(&exported).is_empty());

    // Used as a value: whoever holds the value calls it with anything.
    let escaped = format!(
        "{BUILDERS}proc build_with(build: Proc, root: Path) -> Result[Unit, Error] {{\n  let _ = build.call(root)?\n}}\n\nlet held = build_with\nheld(debug_build, p\"/a\")\nbuild_with(debug_build, p\"/b\")\n"
    );
    assert!(lint(&escaped).is_empty());

    // Passed a dynamic handle: its signature is not known.
    let dynamic = format!(
        "{BUILDERS}proc build_with(build: Proc, root: Path) -> Result[Unit, Error] {{\n  let _ = build.call(root)?\n}}\n\nproc forward(build: Proc, root: Path) {{\n  build_with(build, root)\n}}\n\nforward(debug_build, p\"/a\")\nbuild_with(debug_build, p\"/b\")\n"
    );
    let diagnostics = lint(&dynamic);
    // `forward` itself is always passed `debug_build`; `build_with` is not.
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "every call of `forward` passes a function with one signature as `build`"
    );

    // Never called here: nothing shows what it is passed.
    let uncalled = format!(
        "{BUILDERS}proc build_with(build: Proc, root: Path) -> Result[Unit, Error] {{\n  let _ = build.call(root)?\n}}\n\ndebug_build(p\"/a\")\nrelease_build(p\"/b\")\n"
    );
    assert!(lint(&uncalled).is_empty());
}
