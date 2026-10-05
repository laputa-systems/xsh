use super::lint_size_products;
use xsh::diagnostic::Diagnostic;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

fn lint(source: &str) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    parsed
        .arena
        .symbol_owner()
        .clone()
        .with_current(|| lint_size_products(&parsed.arena, source))
}

/// The source after applying every fix the lint offers.
fn fixed(source: &str) -> String {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = lint(source)
        .iter()
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .filter_map(|hint| Some((hint.span?.range(), hint.replacement.clone()?)))
        .collect();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut text = source.to_owned();
    for (range, replacement) in edits {
        text.replace_range(range, &replacement);
    }
    text
}

#[test]
fn products_of_1024_under_operators_and_typed_bindings_are_rewritten() {
    for (source, expected) in [
        (
            "assert size == 64 * 1024 * 1024\n",
            "assert size == 64MiB\n",
        ),
        ("let small = size < 4 * 1024\n", "let small = size < 4KiB\n"),
        (
            "let raw = used + used / 4 + 1024 * 1024 * 64\n",
            "let raw = used + used / 4 + 64MiB\n",
        ),
        (
            "const limit: UInt = 5 * 1024 * 1024 * 1024\n",
            "const limit: UInt = 5GiB\n",
        ),
        ("var chunk: Int = 1024 * 1024\n", "var chunk: Int = 1MiB\n"),
        (
            "let blocks = total / (1024 * 1024)\n",
            "let blocks = total / 1MiB\n",
        ),
        (
            "if ( 1024 * 1024 ) <= size { print big }\n",
            "if 1MiB <= size { print big }\n",
        ),
        (
            "let padded = align(4 * 1024 + size)\n",
            "let padded = align(4KiB + size)\n",
        ),
        (
            "let big: Int = 2 * 1024 * 1024 * 1024 * 1024\n",
            "let big: Int = 2048GiB\n",
        ),
    ] {
        assert_eq!(fixed(source), expected, "{source}");
        assert!(lint(expected).is_empty(), "{expected}");
    }
}

#[test]
fn products_whose_type_is_inferred_are_reported_without_a_fix() {
    for source in [
        "let chunk = 1024 * 1024\n",
        "var remaining = 8 * 1024\n",
        "image.truncate(128 * 1024 * 1024)?\n",
        "let sizes = [4 * 1024, 8 * 1024]\n",
        "run head -c (64 * 1024) file\n",
        "proc reserve() { return 16 * 1024 * 1024 }\n",
    ] {
        let diagnostics = lint(source);
        assert!(!diagnostics.is_empty(), "{source}");
        for diagnostic in &diagnostics {
            assert_eq!(
                diagnostic.code.map(|code| code.name()),
                Some("lint.prefer-size-literal")
            );
            assert!(diagnostic.fix_hints.is_empty(), "{source}");
            assert!(
                diagnostic.notes.iter().any(|note| note.contains("`UInt`")),
                "{source}"
            );
        }
        assert_eq!(fixed(source), source);
    }
}

#[test]
fn other_products_are_left_alone() {
    for source in [
        // A factor that is not a literal.
        "let bytes = megabytes * 1024 * 1024\n",
        "let bytes = 1024 * 1024 * megabytes\n",
        "let bytes = 4 * page_size\n",
        // No factor of 1024: milliseconds, counts, and geometry.
        "let millis = 60 * 1000\n",
        "let pixels = 1920 * 1080\n",
        // Several counts: the author's grouping says more than one number.
        "let total = 2 * 8 * 1024\n",
        // Not a plain decimal product.
        "let mode = 0o2000 * 1024\n",
        "let scaled = 1.5 * 1024.0\n",
        "let sized = 4KiB * 1024\n",
        "let offset = -4 * 1024\n",
        // Already a literal.
        "let chunk = 64KiB\n",
    ] {
        assert!(lint(source).is_empty(), "{source}: {:?}", lint(source));
    }
}

#[test]
fn a_product_broken_across_lines_keeps_its_layout() {
    let source = "let ok = size == 64 *\n  1024 * 1024\n";
    let diagnostics = lint(source);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].fix_hints.is_empty());
}
