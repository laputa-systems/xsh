//! `Formatter::desugar_source` prints the program the checker reads. These
//! tests hold a handful of inline programs to that: the desugared program has
//! no sugar, is stable under the formatter and under desugaring again, and
//! checks exactly as the original does. The integration test
//! `the_desugared_corpus_checks_and_tests_like_the_corpus` holds the whole
//! native test corpus to the same contract and also runs it.

use super::{Formatter, fresh_name, hidden_names, is_spellable_name};
use std::collections::BTreeSet;
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

fn desugar(source: &str) -> String {
    let output = Formatter::new().desugar_source(SourceId::new(0), source);
    assert!(
        output.diagnostics.is_empty(),
        "{source}\n{:?}",
        output.diagnostics
    );
    output.formatted
}

/// Every diagnostic of a check as `code: message`, sorted. Positions are left
/// out because desugaring moves them.
fn check(source: &str) -> Vec<String> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{source}\n{:?}",
        parsed.diagnostics
    );
    let mut reported = Checker::check_arena(&parsed.arena, source)
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}: {}",
                diagnostic.code.map_or("", |code| code.name()),
                diagnostic.message
            )
        })
        .collect::<Vec<_>>();
    reported.sort();
    reported
}

fn sugar_count(source: &str) -> usize {
    Parser::parse_source_arena_only(SourceId::new(0), source)
        .arena
        .arena
        .sugar_expansions
        .len()
}

/// Programs that check clean and programs that do not: the two spellings
/// must agree on every diagnostic either way.
const PROGRAMS: &[&str] = &[
    "pure read(raw: Str?) -> Str {\n  return \"none\" when raw == null\n  raw.trim()\n}\n",
    "pure read(raw: Str?) -> Str {\n  return \"none\" unless raw != null\n  raw.trim()\n}\n",
    "pure read(raw: Str?) -> Str {\n  guard raw != null else { return \"none\" }\n  raw.trim()\n}\n",
    "var total = 0\nrepeat 3 times {\n  total += 1\n  continue when total == 2\n  break unless total < 9\n}\nprint $total\n",
    "stream picked(rows: List[Int]) -> Stream[Int] {\n  for row in rows {\n    guard row >= 0 else {\n      guard row > -9 else { break }\n      continue\n    }\n    yield row when row > 0\n  }\n}\n",
    "proc pick(code: Int) -> Int {\n  match code {\n    0 => return 1 when code == 0\n    _ => { return 2 unless code > 5 }\n  }\n  3\n}\n",
    // A guarded statement proves nothing when its payload stays in the block.
    "stream rows(raw: Str?) -> Stream[Str] {\n  yield \"none\" when raw == null\n  yield raw.trim()\n}\n",
    // A condition that is not a Bool or a Status.
    "pure pick() -> Int { return 1 when 2; return 3 }\n",
    "for n in [1] {\n  continue unless n + 1\n}\n",
    "repeat \"twice\" times {\n  print \"tick\"\n}\n",
    // A body that can fall through.
    "pure pick(flag: Bool) -> Int { return 1 when flag }\n",
    "pure pick(flag: Bool) -> Int { guard flag else { return 1 } }\n",
    // Control flow outside its target.
    "return when true\n",
    "break unless false\n",
];

#[test]
fn a_desugared_program_checks_like_the_program() {
    for source in PROGRAMS {
        assert!(sugar_count(source) > 0, "no sugar in:\n{source}");
        let desugared = desugar(source);
        assert_eq!(sugar_count(&desugared), 0, "sugar left in:\n{desugared}");
        assert_eq!(check(source), check(&desugared), "{source}\n{desugared}");
    }
}

#[test]
fn desugared_output_is_formatted_and_desugars_to_itself() {
    for source in PROGRAMS {
        let desugared = desugar(source);
        assert_eq!(desugar(&desugared), desugared, "{source}");
        let formatted = Formatter::new().format_source(SourceId::new(0), &desugared);
        assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
        assert_eq!(formatted.formatted, desugared, "{source}");
    }
}

/// The failure block of a `guard` must leave; the `if` it expands to may
/// fall through. That rule is the one part of a sugar form's meaning that
/// its printed expansion does not carry.
#[test]
fn a_guard_that_falls_through_is_rejected_only_as_written() {
    let source = "proc settle(flag: Bool) {\n  guard flag else { print \"stay\" }\n}\n";
    assert_eq!(check(source).len(), 1, "{:?}", check(source));
    assert!(check(source)[0].starts_with("check.guard-fallthrough: "));
    assert_eq!(check(&desugar(source)), Vec::<String>::new());
}

#[test]
fn comments_stay_on_the_expansion_of_the_statement_they_lead() {
    let source = "for n in [1, 2] {\n  # skip the first\n  continue when n == 1 # cheap\n  # never negative\n  guard n > 0 else {\n    # give up\n    break\n  }\n\n  print $n\n}\n";
    assert_eq!(
        desugar(source),
        "for n in [1, 2] {\n  # skip the first\n  if n == 1 { continue } # cheap\n\n  # never negative\n  if n > 0 {} else {\n    # give up\n    break\n  }\n\n  print $n\n}\n"
    );
}

/// The formatter copies some text from the source instead of printing it: a
/// statement after `# fmt: skip`, and an expression with a comment inside.
/// A copy would keep the sugar it holds, so those are printed instead.
#[test]
fn sugar_inside_text_the_formatter_would_copy_is_expanded() {
    let source = "proc pick(rows: List[Int]) -> Int {\n  # fmt: skip\n  return   0   when rows.len()   ==   0\n  let kept = rows |> map { |row|\n    # negative rows count as one\n    return 1 when row < 0\n    row\n  }\n  kept.len()\n}\n";
    let desugared = desugar(source);
    assert_eq!(sugar_count(&desugared), 0, "{desugared}");
    assert!(desugared.contains("# fmt: skip\n  if rows.len() == 0 { return 0 }"), "{desugared}");
    assert!(desugared.contains("# negative rows count as one\n"), "{desugared}");
    assert_eq!(check(source), check(&desugared));
}

#[test]
fn a_program_without_sugar_is_printed_as_the_formatter_prints_it() {
    let source = "let xs = [1,2]\nfor x in xs { print $x }\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert_eq!(desugar(source), formatted.formatted);
}

#[test]
fn a_name_is_spellable_when_it_is_one_identifier() {
    for name in ["range", "_", "scratch_2", "sort-by"] {
        assert!(is_spellable_name(name), "{name}");
    }
    for name in ["", "%scratch", "scratch dir", "2nd", "a.b", "if"] {
        assert!(!is_spellable_name(name), "{name}");
    }
}

#[test]
fn a_hidden_local_gets_a_fresh_spellable_name() {
    let mut taken = ["tempdir_1", "tempdir_2", "local_1"]
        .map(str::to_string)
        .into_iter()
        .collect::<BTreeSet<_>>();
    // The name keeps what can be spelled of the hidden one and skips every
    // spelling already in use, including the ones it hands out.
    assert_eq!(fresh_name("%tempdir", &mut taken), "tempdir_3");
    assert_eq!(fresh_name("%tempdir", &mut taken), "tempdir_4");
    assert_eq!(fresh_name("<if>", &mut taken), "if_1");
    assert_eq!(fresh_name("%%", &mut taken), "local_2");
    assert_eq!(fresh_name("%9lives", &mut taken), "lives_1");
    for name in ["tempdir_3", "tempdir_4", "if_1", "local_2", "lives_1"] {
        assert!(is_spellable_name(name), "{name}");
    }
}

/// No form binds a hidden local yet, so a program the parser builds has none
/// to rename and every written name is printed as written.
#[test]
fn written_names_are_never_renamed() {
    for source in PROGRAMS {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert_eq!(hidden_names(&parsed.arena.arena, source), Vec::new(), "{source}");
    }
}
