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
        "f(H, 1)", "f(1, H)", "f(H?.f, 1)", "f(H?.f(), 1)", "[H, 1]", "{k: H}", "if H {\n  w()\n}", "while H {\n  w()\n}", "for x in H {\n  w()\n}",
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

/// Applies every redundant-parenthesis fix until none remain.
fn remove_redundant_parens(source: &str) -> String {
    let mut text = source.to_string();
    for _ in 0..16 {
        let mut fixes: Vec<(usize, usize, String)> = grouping(&text)
            .iter()
            .filter(|diagnostic| diagnostic.code.as_deref() == Some("check.redundant-parens"))
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .map(|hint| (hint.span.unwrap().start(), hint.span.unwrap().end(), hint.replacement.clone().unwrap()))
            .collect();
        if fixes.is_empty() {
            return text;
        }
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.0));
        let mut limit = usize::MAX;
        for (start, end, replacement) in fixes {
            if end <= limit {
                text.replace_range(start..end, &replacement);
                limit = start;
            }
        }
    }
    panic!("redundant-parenthesis fixes do not converge for {source}");
}

/// Every parent slot with every child form, the child written in
/// parentheses: the printer's output parses to the same tree and is a
/// fixpoint, and `check.redundant-parens` reports the parentheses exactly when
/// the source without them parses to the same tree with no grouping error.
#[test]
fn needs_parens_is_exact_for_every_slot_and_form() {
    let (parents, children) = (parents(), children());
    let (mut cases, mut required, mut redundant, mut kept, mut ambiguous) = (0, 0, 0, 0, 0);
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
            let same_tree = canonical(&bare).as_ref() == Some(&tree);
            let bare_grouping = grouping(&bare);
            let removable = same_tree && bare_grouping.iter().all(|d| !matches!(d.code.as_deref(), Some("check.mixed-logical" | "check.ambiguous-grouping")));
            // The ambiguous-grouping fix restores exactly these parentheses.
            if same_tree && let Some(fix) = bare_grouping.iter().find(|d| d.code.as_deref() == Some("check.ambiguous-grouping")) {
                let hint = &fix.fix_hints[0];
                let mut fixed = bare.clone();
                fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
                assert_eq!(fixed, grouped, "ambiguous-grouping fix of\n{bare}");
                ambiguous += 1;
            }
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
    assert_eq!((parents.len(), children.len()), (75, 65));
    assert_eq!((cases, required, redundant, kept, ambiguous), (4875, 1385, 3480, 10, 156));
}

struct Generator(u64);

impl Generator {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound
    }

    /// A random expression whose every operand is parenthesized, so it parses
    /// to the tree it was built as.
    fn expr(&mut self, depth: usize) -> String {
        const LEAVES: [&str; 8] = ["a", "b", "1", "\"s\"", "[a, 2]", "{k: a}", "/tmp/x", "."];
        if depth == 0 || self.next(4) == 0 {
            return LEAVES[self.next(LEAVES.len())].to_string();
        }
        let operand = |generator: &mut Self| format!("({})", generator.expr(depth - 1));
        match self.next(15) {
            0..=3 => {
                let op = OPERATORS[self.next(OPERATORS.len())];
                format!("{} {op} {}", operand(self), operand(self))
            }
            4 => format!("-{}", operand(self)),
            5 => format!("! {}", operand(self)),
            6 => format!("{}.f", operand(self)),
            7 => format!("{}?", operand(self)),
            8 => format!("{}[{}]", operand(self), operand(self)),
            9 => format!("f({}, {})", operand(self), operand(self)),
            10 => format!("{} is Int", operand(self)),
            11 => format!("if {} {{ {} }} else {{ {} }}", operand(self), operand(self), operand(self)),
            12 => format!("[{} for x in {}]", operand(self), operand(self)),
            13 => format!("match {} {{ _ => {} }}", operand(self), operand(self)),
            _ => ["run foo", "spawn f()", "b |> f(.)", "b?.f", "b |> sort", "b |> take(1)"][self.next(6)].to_string(),
        }
    }
}

/// Random fully parenthesized expressions in every slot template: the
/// printer's output parses to the same tree, is a fixpoint, and has no
/// redundant parentheses; removing every redundant parenthesis from the
/// source keeps the tree and formats identically.
#[test]
fn generated_trees_round_trip_through_the_printer_and_redundancy_fixes() {
    // Line breaking is not under test; one line keeps every tree comparable.
    let wide = |source: &str| format_wide(source, usize::MAX / 2);
    let parents = parents();
    let mut generator = Generator(0x5eed);
    let mut checked = 0;
    for _ in 0..5000 {
        let parent = &parents[generator.next(parents.len())];
        let expr = generator.expr(4);
        let source = parent.replacen('H', &format!("({expr})"), 1);
        let Some(tree) = canonical(&source) else { continue };
        checked += 1;
        let formatted = wide(&source);
        assert_eq!(canonical(&formatted).as_ref(), Some(&tree), "{source}\nprinted as\n{formatted}");
        assert_eq!(wide(&formatted), formatted, "{source}");
        assert!(grouping(&formatted).is_empty(), "{formatted}\n{:?}", grouping(&formatted));
        let minimal = remove_redundant_parens(&source);
        assert_eq!(canonical(&minimal).as_ref(), Some(&tree), "{source}\nfixed as\n{minimal}");
        assert_eq!(wide(&minimal), formatted, "{source}\nfixed as\n{minimal}");
    }
    assert_eq!(checked, 5000);
}
