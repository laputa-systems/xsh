//! The grammar in `src/syntax/grammar.rs` and the parser accept the same
//! language: sentences generated from the productions parse without
//! diagnostics, and every checked-in source the parser accepts is a sentence
//! of the productions.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use xsh::diagnostic::DiagnosticCode;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::grammar::earley::{Recognizer, top_level_parts};
use xsh::frontend::syntax::grammar::generate::{Generator, NAMES};
use xsh::frontend::syntax::grammar::{
    Class, Grammar, Item, STATEMENT_KEYWORDS, STREAM_STAGES, grammar, lex_grammar_tokens,
    line_continuation_spellings, stream_stage,
};
use xsh::frontend::syntax::parser::Parser;

/// Fixed seeds and depths: shallow sentences cover every production's
/// shortest forms, deeper ones nest them.
const SENTENCE_SEEDS: u64 = 1500;
const SENTENCE_DEPTHS: [u32; 3] = [4, 7, 10];

fn workers() -> usize {
    std::thread::available_parallelism()
        .map_or(2, std::num::NonZeroUsize::get)
        .clamp(1, 4)
}

/// Runs `body` over `items` on a few threads, collecting failure reports.
fn for_each_parallel<T: Sync>(
    items: &[T],
    body: impl Fn(&T) -> Option<String> + Sync,
) -> Vec<String> {
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers() {
            scope.spawn(|| {
                while let Some(item) = items.get(next.fetch_add(1, Ordering::Relaxed)) {
                    if let Some(failure) = body(item) {
                        failures.lock().unwrap().push(failure);
                    }
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    failures.sort();
    failures
}

fn rule_references(item: &Item, out: &mut Vec<&'static str>) {
    match item {
        Item::Rule(name) => out.push(name),
        Item::Seq(items) | Item::Alt(items) => {
            items.iter().for_each(|item| rule_references(item, out))
        }
        Item::Opt(inner) | Item::Star(inner) | Item::Plus(inner) | Item::Line(inner) | Item::Group(inner) => {
            rule_references(inner, out)
        }
        Item::List { item, .. } => rule_references(item, out),
        Item::Term(_) | Item::Not(_) | Item::Peek(_) | Item::NameStatement(_) | Item::WordStart | Item::NotContextScope | Item::ConditionBody { .. } => {}
    }
}

fn words(item: &Item, out: &mut Vec<&'static str>) {
    let mut terms = Vec::new();
    match item {
        Item::Term(term) => terms.push(*term),
        Item::Not(sequences) | Item::Peek(sequences) => {
            terms.extend(sequences.iter().flatten().copied())
        }
        Item::Seq(items) | Item::Alt(items) => items.iter().for_each(|item| words(item, out)),
        Item::Opt(inner) | Item::Star(inner) | Item::Plus(inner) | Item::Line(inner) | Item::Group(inner) => {
            words(inner, out)
        }
        Item::List { item, .. } => words(item, out),
        Item::Rule(_) | Item::NameStatement(_) | Item::WordStart | Item::NotContextScope | Item::ConditionBody { .. } => {}
    }
    out.extend(terms.into_iter().filter_map(|term| match term.class {
        Class::Word(word) => Some(word),
        _ => None,
    }));
}

#[test]
fn grammar_rules_are_defined_reachable_and_derivable() {
    let grammar = grammar();
    let mut reachable = vec![Grammar::START];
    let mut index = 0;
    while let Some(name) = reachable.get(index).copied() {
        let rule = grammar
            .rule(name)
            .unwrap_or_else(|| panic!("rule `{name}` is referenced but not defined"));
        let mut references = Vec::new();
        rule_references(&rule.body, &mut references);
        for reference in references {
            if !reachable.contains(&reference) {
                reachable.push(reference);
            }
        }
        index += 1;
    }
    let unreachable: Vec<&str> = grammar
        .rules
        .iter()
        .map(|rule| rule.name)
        .filter(|name| !reachable.contains(name))
        .collect();
    assert!(
        unreachable.is_empty(),
        "rules unreachable from `program`: {unreachable:?}"
    );
    assert_eq!(
        Generator::new(grammar).underivable_rules(),
        Vec::<&str>::new(),
        "rules with no finite derivation"
    );
    // Generated names must never spell a contextual word, or a sentence
    // could dispatch differently from the production that built it.
    let mut spelled = Vec::new();
    for rule in &grammar.rules {
        words(&rule.body, &mut spelled);
    }
    let clashes: Vec<&str> = NAMES
        .iter()
        .copied()
        .filter(|name| spelled.contains(name))
        .collect();
    assert!(
        clashes.is_empty(),
        "generator names that the grammar spells as words: {clashes:?}"
    );
    // `stream_stage` indexes the stage table by kind.
    for stage in &STREAM_STAGES {
        assert_eq!(
            stream_stage(stage.kind).name,
            stage.name,
            "the stage table is out of `StreamStageKind` order"
        );
    }
}

/// The parser dispatches statements on the grammar's keyword table, and each
/// form's production begins with its keyword.
#[test]
fn statement_keyword_forms_begin_with_their_keywords() {
    let grammar = grammar();
    let mut generator = Generator::new(grammar);
    for (keyword, form) in STATEMENT_KEYWORDS {
        let Some(rule) = form.rule() else { continue };
        let leads: Vec<&str> = STATEMENT_KEYWORDS
            .iter()
            .filter(|(_, other)| *other == form)
            .map(|(keyword, _)| keyword.as_str())
            .collect();
        assert!(leads.contains(&keyword.as_str()));
        for seed in 0..16 {
            let Some(sentence) = generator.sentence(rule, seed, 3) else {
                continue;
            };
            let first = sentence.split_whitespace().next().unwrap_or("");
            let first = first.split(['.', '(']).next().unwrap_or("");
            assert!(
                leads.contains(&first),
                "`{rule}` sentence does not begin with {leads:?}: {sentence}"
            );
        }
    }
}

/// Seeds per rule for candidates that take the shortest way to that rule,
/// so every production is exercised.
const TARGETED_SEEDS: u64 = 6;

#[test]
fn grammar_generated_sentences_parse_without_diagnostics() {
    generated_sentences_parse(&SENTENCE_DEPTHS, SENTENCE_SEEDS, 4, TARGETED_SEEDS);
}

#[test]
fn grammar_deeper_generated_sentences_parse_without_diagnostics() {
    generated_sentences_parse(&[12, 16], 10_000, 12, 16);
}

fn generated_sentences_parse(depths: &[u32], seeds: u64, targeted_depth: u32, targeted_seeds: u64) {
    let started = Instant::now();
    let grammar = grammar();
    let recognizer = Recognizer::new(grammar);
    let mut jobs: Vec<(u32, u64, Option<&'static str>)> = depths
        .iter()
        .flat_map(|depth| (0..seeds).map(move |seed| (*depth, seed, None)))
        .collect();
    jobs.extend(
        grammar
            .rules
            .iter()
            .flat_map(|rule| (0..targeted_seeds).map(move |seed| (targeted_depth, seed, Some(rule.name)))),
    );
    let sentences = AtomicUsize::new(0);
    let covered = Mutex::new(std::collections::BTreeSet::new());
    let failures = for_each_parallel(&jobs, |&(depth, seed, target)| {
        let mut generator = Generator::new(grammar);
        let source = match target {
            Some(target) => {
                // These expression rules have a shorter route through run
                // option values, whose surrounding command context rejects
                // the generated forms.
                let start = match target {
                    "collect_expression"
                    | "builder_call"
                    | "builder_block"
                    | "builder_compound_entry"
                    | "builder_entry"
                    | "run_end" => "expression_statement",
                    _ => Grammar::START,
                };
                generator.targeted_sentence(start, target, seed, depth)?
            }
            None => generator.sentence(Grammar::START, seed, depth)?,
        };
        // A candidate that misses a lookahead or lexes differently is not a
        // sentence of the grammar.
        let tokens = lex_grammar_tokens(&source)?;
        recognizer.recognize(&tokens).ok()?;
        sentences.fetch_add(1, Ordering::Relaxed);
        covered
            .lock()
            .unwrap()
            .extend(generator.expanded_rules().iter().copied());
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        let diagnostic = parsed.diagnostics.first()?;
        Some(format!(
            "depth {depth} seed {seed} target {target:?}: [{}] {} at {:?}\n{source}",
            diagnostic.code.map(DiagnosticCode::name).unwrap_or(""),
            diagnostic.message,
            diagnostic.span
        ))
    });
    let sentences = sentences.into_inner();
    eprintln!(
        "grammar generation: {sentences} sentences of {} candidates in {:?}",
        jobs.len(),
        started.elapsed()
    );
    assert!(
        failures.is_empty(),
        "{} generated sentences failed to parse:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(
        sentences * 10 >= jobs.len() * 8,
        "only {sentences} of {} candidates were sentences",
        jobs.len()
    );
    let covered = covered.into_inner().unwrap();
    let missing: Vec<&str> = grammar
        .rules
        .iter()
        .map(|rule| rule.name)
        .filter(|name| !covered.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "rules no generated sentence used: {missing:?}"
    );
}

fn xsh_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).expect("read corpus directory") {
        let entry = entry.expect("read corpus entry");
        let path = entry.path();
        if entry.file_type().expect("corpus entry type").is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some("target" | ".git" | ".claude" | ".work")
            ) {
                xsh_files(&path, out);
            }
        } else if path.extension().is_some_and(|extension| extension == "xsh") {
            out.push(path);
        }
    }
}

#[cfg(unix)]
struct RestoreDirectoryPermissions(PathBuf);

#[cfg(unix)]
impl Drop for RestoreDirectoryPermissions {
    fn drop(&mut self) {
        std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700))
            .expect("restore scratch directory permissions");
    }
}

#[cfg(unix)]
#[test]
fn grammar_corpus_discovery_skips_work_tree() {
    let scratch = tempfile::tempdir().expect("scratch directory");
    let root = scratch.path();
    let source = root.join("source.xsh");
    std::fs::write(&source, "print \"source\"\n").expect("write corpus source");
    let blocked = root.join(".work/tmp/blocked");
    std::fs::create_dir_all(&blocked).expect("create unreadable scratch child");
    std::fs::write(blocked.join("scratch.xsh"), "print \"scratch\"\n")
        .expect("write scratch source");
    let _restore = RestoreDirectoryPermissions(blocked.clone());
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o000))
        .expect("make scratch child unreadable");

    let mut paths = Vec::new();
    xsh_files(root, &mut paths);
    paths.sort();
    assert_eq!(paths, vec![source]);
}

/// Every `.xsh` file in the repository that parses without diagnostics.
#[test]
fn grammar_recognizes_every_parsed_repository_source() {
    let started = Instant::now();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    xsh_files(root, &mut paths);
    paths.sort();
    let recognizer = Recognizer::new(grammar());
    let recognized = AtomicUsize::new(0);
    let tokens_seen = AtomicUsize::new(0);
    let failures = for_each_parallel(&paths, |path| {
        let source = std::fs::read_to_string(path).ok()?;
        if !Parser::parse_source_arena_only(SourceId::new(0), &source)
            .diagnostics
            .is_empty()
        {
            return None;
        }
        let tokens = lex_grammar_tokens(&source).expect("a parsed source lexes cleanly");
        tokens_seen.fetch_add(tokens.len(), Ordering::Relaxed);
        for part in top_level_parts(&tokens) {
            if let Err(rejection) = recognizer.recognize(part) {
                let near: Vec<&str> = part
                    [rejection.token.saturating_sub(4)..(rejection.token + 3).min(part.len())]
                    .iter()
                    .map(|token| token.text)
                    .collect();
                return Some(format!(
                    "{}: token {:?} near {near:?}; expected {}",
                    path.display(),
                    part.get(rejection.token).map(|token| token.text),
                    rejection.expected.join(" ")
                ));
            }
        }
        recognized.fetch_add(1, Ordering::Relaxed);
        None
    });
    let recognized = recognized.into_inner();
    eprintln!(
        "grammar recognition: {recognized} of {} files, {} tokens, in {:?}",
        paths.len(),
        tokens_seen.into_inner(),
        started.elapsed()
    );
    assert!(
        failures.is_empty(),
        "{} parsed sources are not grammar sentences:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        recognized > 500,
        "only {recognized} corpus files were recognized"
    );
}

/// The recognizer rejects what the parser rejects, so recognition is not
/// vacuous.
#[test]
fn grammar_rejects_sources_the_parser_rejects() {
    let recognizer = Recognizer::new(grammar());
    for source in [
        "let = 1\n",
        "if ready { } else\n",
        "proc build { }\n",
        "let x = 1 +\n",
        "run\n",
        "match x { 1 => f() 2 => g() }\n",
        "cd /tmp\n",
        "if ready { }\nelse { }\n",
        "test inner { test nested { } }\n",
        "export var x = 1\n",
        "run ls | echo\n",
        "on TERM { }\n",
        "enum Empty { }\n",
        "f (x\n",
        "guard x is E.V {} else { }\n",
        "exit when ready\n",
        "fail unless ready\n",
        "exit ?\n",
        "fail ?\n",
        "exit as\n",
        "fail as\n",
        "exit is\n",
        "fail is\n",
    ] {
        assert!(
            !Parser::parse_source_arena_only(SourceId::new(0), source)
                .diagnostics
                .is_empty(),
            "the parser accepts {source:?}"
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_err(),
            "the grammar accepts {source:?}"
        );
    }
}

fn assert_parser_and_grammar_agree(source: &str, accepted: bool) {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert_eq!(parsed.diagnostics.is_empty(), accepted, "{source:?}: {:?}", parsed.diagnostics);
    let tokens = lex_grammar_tokens(source).expect("regression source lexes");
    let recognized = Recognizer::new(grammar()).recognize(&tokens);
    assert_eq!(recognized.is_ok(), accepted, "{source:?}: {recognized:?}");
}

#[test]
fn pure_parameters_allow_inferred_annotations() {
    for source in [
        "pure identity(value) { value }\n",
        "export pure identity(value) { value }\n",
        "pure defaulted(value = 1) { value }\n",
        "pure mixed(first: Int, value, fallback: Str = \"x\") { value }\n",
        "pure choose(value, fallback: Int = 1, ... rest: Int) { value }\n",
        "pure rest(... values) { values }\n",
        "pure nonfinal(... values, last: Int) { last }\n",
        "pure defaults_then_required(first = 1, second) { second }\n",
        "proc defaulted(value = 1) { value }\n",
        "stream defaulted(value = 1) -> Stream[Int] { yield value }\n",
        "type Plugin = module { export pure render(value: Str) -> Str }\n",
        "type Plugin = module { export pure render(value = \"x\") -> Str }\n",
        "type Render = pure(value: Str) -> Str\n",
        "type Render = pure(value = 1) -> Int\n",
    ] {
        assert_parser_and_grammar_agree(source, true);
    }
    for source in [
        "pure rest(... values = 1) { values }\n",
        "pure rest(... values: List[Int] = []) { values }\n",
        "proc identity(value) { value }\n",
        "proc rest(... values) { values }\n",
        "stream identity(value) -> Stream[Int] { yield value }\n",
        "stream rest(... values) -> Stream[Int] { yield 1 }\n",
        "type Plugin = module { export pure render(value) -> Str }\n",
        "type Plugin = module { export pure render(... values) -> Str }\n",
        "type Render = pure(value) -> Str\n",
        "type Render = pure(... values) -> Str\n",
    ] {
        assert_parser_and_grammar_agree(source, false);
    }
}

#[test]
fn unannotated_pure_parameters_require_an_immediate_delimiter() {
    for source in [
        "pure immediate(value,\nother) { value }\n",
        "pure immediate(... values,\nlast: Int) { last }\n",
        "pure annotated(value: Int\n) { value }\n",
        "pure defaulted(value = 1\n) { value }\n",
    ] {
        assert_parser_and_grammar_agree(source, true);
    }
    for source in [
        "pure identity(value\n) { value }\n",
        "pure identity(value\n, other) { value }\n",
        "pure rest(... values\n) { values }\n",
        "pure rest(... values\n, last: Int) { last }\n",
        r#"pure items ( items = $? not in y or $? & $? < $? , ... items
) { | c | } loop { | items , | } exit b or items - - b as c >= - Thing
"#,
        r#"pure c ( y
) -> a . Thing [ proc ( ) [ ] -> y . y [ ] ? ] ? { ; error x { Thing (
) } }
"#,
        r#"cli Thing ( ) -> x . x ? { | items , | } test items [ ] { b /= $? not in $? not in value / $? ? ; pure items ( y
) -> b . Thing [ proc ( ) [ ] -> proc ( ) [ ] -> proc ( ) [ ] -> proc ( ) [ ] -> Thing ] ? { } } within 10ms { ; type c = exact module { } }
"#,
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert_eq!(parsed.diagnostics[0].code, Some(DiagnosticCode::ParseExpectedParamType));
        assert_parser_and_grammar_agree(source, false);
    }
}

#[test]
fn list_rest_bindings_use_binding_names() {
    for source in ["while let [.. tail] = xs { }\n", "while let [.. _] = xs { }\n", "while let [..] = xs { }\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
    assert_parser_and_grammar_agree("while let [.. a-b] = xs { }\n", false);
}

#[test]
fn expression_suffixes_after_stages_allow_spaces() {
    for source in ["xs |> first .name\n", "let selected = xs |> first .name\n", "let selected = xs |> first ?.name\n", "let selected = xs |> first [0]\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
}

#[test]
fn final_builder_blocks_can_belong_to_condition_heads() {
    for source in [
        "if ready and process.command { }\n",
        "tempdir name at ready and process.command { }\n",
        "with x = ready and process.command { }\n",
        "if process.command { } == true { 1 } else { 0 }\n",
        "with x = process.command { }, y = other { }\n",
        "wait until process.command { } within 5s\n",
    ] {
        assert_parser_and_grammar_agree(source, true);
    }
    assert_parser_and_grammar_agree(
        "export enum c: Str { b = with x = true and process.command { return }, }\n",
        false,
    );
}

#[test]
fn context_scopes_cannot_supply_an_enclosing_scope_body() {
    assert_parser_and_grammar_agree("let value = with x = true and cd (x) { }\n", false);
}

#[test]
fn mutated_fuzz_cli_source_is_a_grammar_sentence() {
    assert_parser_and_grammar_agree("cli p() { }\n", true);
    assert_parser_and_grammar_agree("cli main() { }\n", true);
    assert_parser_and_grammar_agree("cli main subcommand() { }\n", true);
    let source = r#"use fuzz_paths
cli p(duration: UInt) {
  let messages = run.text cargo build --release -p xsh-fuzz --bin xsh-fuzz --message-format=json-render-diagnostics
  let binary = fuzz_paths.built_binary(messages)?
  let out = fuzz_paths.failure_dir(binary)?
  run $message all --duration $duration --out $out
}
"#;
    assert_parser_and_grammar_agree(source, true);
}

#[test]
fn tempdir_cannot_steal_a_stream_stage_block() {
    assert_parser_and_grammar_agree(
        "let value = tempdir items at Thing |> each |> all { ; return }\n",
        false,
    );
}

#[test]
fn command_argument_spacing_is_preserved() {
    for source in ["f(1, 2)\n", "let value = f (1, 2)\n", "print (f(1, 2))\n", "f().field (1, 2)\n", "f.field(1, 2)\n", "f.field (1)\n", "let value = f ?.name\n", "f.field when ready\n", "f -1\n", "f - 1\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
    for source in ["f (1, 2)\n", "f.field (1, 2)\n", "f ?.name\n", "print = 1 2\n", "eprint = 1 2\n"] {
        assert_parser_and_grammar_agree(source, false);
    }
}

#[test]
fn command_dot_words_are_recognized() {
    for source in ["f .name\n", "f . name\n", "f.field .name\n", "print .name .field\n", "print. name\n", "ctx \"message\" { f() }\n", "collect { yield 1 }\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
    for source in ["f.field as\n", "f .\n", "cd .name\n"] {
        assert_parser_and_grammar_agree(source, false);
    }
}

#[test]
fn env_without_a_block_uses_proc_command_rules() {
    for source in ["env foo\n", "env . name\n", "env FOO=value { f() }\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
    assert_parser_and_grammar_agree("env value = ${ items |> each { |item| item } } { }\n", true);
    for source in ["env . name {}\n", "env foo ({})\n"] {
        assert_parser_and_grammar_agree(source, false);
    }
}

#[test]
fn cd_scope_arguments_keep_command_word_interpolation() {
    assert_parser_and_grammar_agree("cd +${ x +\n1 } { }\n", true);
    assert_parser_and_grammar_agree("cd (x) { } < []\n", true);
    assert_parser_and_grammar_agree("env (x) { } < []\n", true);
    assert_parser_and_grammar_agree("env (x) { } next\n", false);
}

#[test]
fn bare_exit_and_fail_do_not_end_before_arm_commas() {
    for source in ["match x { _ => fail , }\n", "match x { _ => exit, }\n"] {
        assert_parser_and_grammar_agree(source, false);
    }
    for source in ["fail\n", "exit\n", "match x { _ => fail, }\n", "match x { _ => fail \"message\", }\n", "match x { _ => exit 1, }\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
}

#[test]
fn stage_arguments_can_precede_grouped_inline_expressions() {
    assert_parser_and_grammar_agree("let ordered = xs |> sort-by(desc: reverse) (if .runtime_seconds <= 0 { 0 } else { .runtime_seconds })\n", true);
}

#[test]
fn bare_minus_command_arguments_preserve_source_adjacency() {
    for source in ["f -\n", "f -#comment\n", "f -1\n", "f - 1\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
    assert_parser_and_grammar_agree("f - \n", false);
    for source in [
        "let result = values |> drop - - $?\n",
        "run.bytes --accept= 5s --timeout= y --cpumax= a target ? |> drop - - $?\n",
    ] {
        assert_parser_and_grammar_agree(source, true);
    }
}

#[test]
fn run_option_values_leave_following_words_as_command_arguments() {
    assert_parser_and_grammar_agree("run --accept= spawn - ! value? (\n@y ) ;\n", true);
    assert_parser_and_grammar_agree(
        "guard let items = run --accept= spawn process.command { } --timeout= 1h --cpumax= a (\ne\"HOME\" ) ? else { }\nexport stream b ( ) -> b { }\n",
        true,
    );
}

#[test]
fn grouped_propagation_in_indices_is_recognized() {
    for source in ["let item = xs[(f()) ?]\n", "let item = xs[.. (f()) ?]\n", "let item = xs[(f())? ..]\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
}

#[test]
fn slice_ranges_do_not_extend_pattern_names() {
    for source in ["let part = xs[y is a .. ]\n", "let part = xs[y is a .. end]\n", "let part = xs[y is Family.Variant .. ]\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
    assert_parser_and_grammar_agree("let part = xs[0 . . end]\n", false);
}

#[test]
fn multiline_block_parameters_are_recognized() {
    assert_parser_and_grammar_agree("let block = { ; | c ,\n value | c }\n", true);
}

#[test]
fn duration_pattern_tests_are_recognized() {
    assert_parser_and_grammar_agree("let matches = value is 10ms\n", true);
}

#[test]
fn last_status_statement_comparisons_are_recognized() {
    assert_parser_and_grammar_agree("$? * $? > 0\n", true);
}

#[test]
fn deeper_generated_counterexamples_have_sound_recognition() {
    let recognizer = Recognizer::new(grammar());
    let mut disagreements = Vec::new();
    for (seed, accepted, source) in [
        ("depth 12 seed 5571 target random", false, r####"fail - $? & - $? as Thing as value > ! Thing | ! $? as Thing as Thing as x * - $? as x and items * c - Thing * $? - Thing & $? * $? * $? | value not in Thing - $? == $? in $? not in $? | b + $? because - Thing / - $? * Thing? as x as Thing & ! y as c is value ? == - $? - - $? as value as c and - $? | $?? < ! $? > $? as b >= ! $? | - a as a as c | value as value as Thing as Thing and - b as items and $? + b not in c when a not in

c == $? +


$? in a not in c is
Module [ a . c [ y ] ? ] ? ==

a in $? |> c and
$? is

Map [ y ] == $? not in $? |> $? < b and $? not in a is _ is items is _ is c is
y [ ] ? and x <=
a ??
$? in $? != $? != a |> bytes.chunks *

$? &
a not in Thing == $? in value not in
Thing in $?
"####),
        ("depth 12 seed 8371 target random", false, r####"fail - x & ! $? or $? < value? ?? $? | $? | $? in $? & $? * $? % items | x % x - $? / $? * $? != $? & x + $? not in Thing - a is a . a [ Set [ Stream [ y ] [ ] ? ] [ value [ ] , Module [ a ] [ ] ? , pure ( ) -> c ] ] ? is c ? |> max as items % items | $? & b and b & a & b <= y | $? + $? % items <= $? - $? + Thing * c & $? | value & $? <= $? - $? or b / $? % value * $? > a < Thing and a | b / Thing - y + y and $? + items - $? in y - value and $? % $? % $? + $? + a / $? | value <= $? - b because $?? * - $? < x & $?? as y as value >= ! x & $? | $? * - $? as y < $? unless a is


value is proc ( Thing : y , ) -> proc ( ) -> c as Thing |> count ( x > x or $? < c ? ) . require (

) +
value * x >= y - c * $? - $? -

$? or x in a is _ is Result [ x ] !=

items in $? == value and items - $? -


$? |


$? and items + $? is Thing . c [ ] and

$? <=

$? - $? ??

$? not in y and $? != x +
$? | $? - $? and


y + b <=


c or a + $? + $? +


x <=


x - y +

$? |> bytes.chunks? %

Thing - x & items &


x - c > c & $? +

Thing |> $? in $?
"####),
        ("depth 12 seed 9 target set_comprehension", true, r####"guard let items : y [ List [ proc (
) [ ] -> proc ( ) -> proc ( ) -> Thing ] [ proc ( ) [ time ] -> proc (
items : items , ... b : c , ) -> List [ value ] ? , Stream [ proc ( ... b : c , ... x : value ) -> y [ ] ] [ ] ] , pure ( ) -> items , NonEmpty [ pure ( a = c , x : proc (
) -> value , value : pure ( ) -> b ) -> List [ proc ( ) -> value ] ? ] [ ] ? , proc ( a = $? and


Thing or Thing > c ??


$? and Thing |> $? >= value ) -> proc ( b = a |> flat-map ) [ error ] -> proc ( ) [
io ] -> pure ( ) -> b ] ? = run --accept= {

x or
Thing <= y

for x in $? >= b |> flat-map } --timeout= c --cpumax= $? y = e"HOME" (
( $? < Thing |> each ( ) |> Thing >= $? ??
$? |> $? > a and $? >= $? ?? items >= $? and
Thing or value and $? >= $? and b ) 2>> b"x" ) | run.stream --bytes (
( x - $? and


$? - y | b >= value +
value ?? b == c in value not in $? ?? c not in value and

c is items . x ? or $? |


c >


$? >=


items -

value <
$? +
$? |

c and value !=
value not in
$? ) ) ? else {
; | x |
}

"####),
        ("depth 12 seed 9 target set_lead", true, r####"guard let items : y [ List [ proc (
) [ ] -> proc ( ) -> proc ( ) -> Thing ] [ proc ( ) [ time ] -> proc (
items : items , ... b : c , ) -> List [ value ] ? , Stream [ proc ( ... b : c , ... x : value ) -> y [ ] ] [ ] ] , pure ( ) -> items , NonEmpty [ pure ( a = c , x : proc (
) -> value , value : pure ( ) -> b ) -> List [ proc ( ) -> value ] ? ] [ ] ? , proc ( a = $? and


Thing or Thing > c ??


$? and Thing |> $? >= value ) -> proc ( b = a |> flat-map ) [ error ] -> proc ( ) [
io ] -> pure ( ) -> b ] ? = run --accept= {

x or
Thing <= y

for x in $? >= b |> flat-map } --timeout= c --cpumax= $? y = e"HOME" (
( $? < Thing |> each ( ) |> Thing >= $? ??
$? |> $? > a and $? >= $? ?? items >= $? and
Thing or value and $? >= $? and b ) 2>> b"x" ) | run.stream --bytes (
( x - $? and


$? - y | b >= value +
value ?? b == c in value not in $? ?? c not in value and

c is items . x ? or $? |


c >


$? >=


items -

value <
$? +
$? |

c and value !=
value not in
$? ) ) ? else {
; | x |
}
"####),
        ("depth 16 seed 3908 target random", false, r####"exit spawn run.stream --text c = fp"{a}/x" e"HOME" > & b"x" | run fp"{a}/x"? % - collect { } in - wait value. y?[ y ] & - $? as x . x ? as pure ( ) -> pure ( ) -> b |> sort-by
"####),
        ("depth 16 seed 7212 target random", false, r####"a-b p"p"
continue unless ! [ b
for b in $? ] | spawn run g"*.x" > & b"x" < b"x" g"*.x" | run.capture --text --cpumax= Thing Thing = e"HOME" (
@g"*.x" ) | run.capture --text (
@g"*.x" ) | run.capture --text --timeout= x --cpumax= 42 --accept= 10ms f"x" e"HOME"? as pure ( ) -> proc ( ) -> x as proc ( ... y : x , a : b
) -> Thing as proc ( ) -> pure ( ) -> value - ! tempdir c at $? > $? {
} . value (
) ( ) + { } as Union [ Set [ a ] ] ? as proc (
) [ process
] -> pure ( ) -> items as proc ( ) [ net , fs , process ] -> proc ( ) -> y &
! - ! value? <= ! items as b . c [ proc ( ) -> y , Module [ value ] ? , Thing [ ] , c [ ] ] ? >
process.command { value ; } as pure ( ... x : x ) -> Set [ Thing ] [ ] as pure ( ... value : items , ... items : a , ... c : items ) -> proc ( ) -> Thing and
$?? as pure ( ... value : a , b : c , items : x , ) -> proc ( ) -> a as Set [ Module [ a ] [ ] ] [ b , value ] &


! ! ! - $?
"####),
        ("depth 16 seed 7311 target random", false, r####"cli main ( ... items : Thing . Thing , value = ! ! a as proc ( ) -> b as proc (
) [ ] -> items is Thing is y . Thing [ ] as y ) [ process ] -> value ? { fail - $? >= - b and ! items > ! a % ! $? <= - $? / $? as value as a as b | - $? & c - ! $? as Thing / ! c & ! $? + - $? as x as a / y? % y? as x as items & ! $? because ! $? as x | ! $? & Thing as y >= $?? + $? as a & ! c as c > $? as x unless Thing * $? >= value >= $? or value & $? & b +

$? - value is Map [ proc ( ) [ ] -> Thing , Result [ value ] ] [ ] and
y is x . b ? is Union [ x . x , b ] and
x % items - value <= x >= y } ; cli main ( ) { }
"####),
        ("depth 16 seed 8347 target random", true, r####"var { .. , x , .. } : proc ( b : proc ( ) -> Result [ NonEmpty [ pure ( ) -> items ] ? , Set [ items [ c ] ] ] [ ] ? = y | $? / a >= c &
value & items >=

$? %


value |

$? % Thing |> max %
$? != Thing not in $? is

( . a ) as items as Thing is Result [ items ] [ ] and a + $? + b + $? is

_ is proc ( ) -> y and $? >= $? ) -> Stream [ pure ( ) -> Stream [ proc ( ) -> pure ( ) -> c ] [ ] ? ] ? = run.stream --bytes --accept= Thing[ y |> $? ? ] items = $a @value | run --cpumax= 2m --timeout= {


value <= x and
x ?

, a >= $? and $? > $? and c |> $? |> flat-map (
) |> x < $? ? }( b : a ? , ) --accept= 0o7 a = ( c |

$? < c *

$? or
Thing % items / y and
$? -
y and $? >=
$? / $? % $? % b and

$? <=
y /
$? <= $? +
$? &
$? % a | $? or
a + x in y |
$? != $? -

a in
$? -

c not in c |
$? | Thing |
$? not in Thing ??
c in

b |


$? - $? ==
Thing | $? ) (
( $? in items & $? %


value & x - b not in

b |


a | $? -
a & $? or $? | a * c % Thing & value in b in


c == $? % items not in $? not in
items |
a | $? +


b / b and

b %
$? & b - $? %
$? % a %


$? >= value >


$? and

b - b |
$? % $? <=
$? +
items ?? c <=
$? %
x % x > $? / a |> $? not in b != x in $? !=
$? in b != a not in Thing ?? Thing >
value and
items < $? >=
$? >=
$? and $? <= $? ??
value is c . a as c ?? c >
b ) ) ?
"####),
        ("depth 16 seed 9215 target random", true, r####"on 1 [ process ,
error ] { | value , x , items
| enum y { y ( ) } ; tempdir value { } ; } run.bytes --timeout= { a <=


x |

c >=

Thing and


c == $? not in items in
Thing and $? <= a and $? |> first >=

$? >=

$? and
$? for b in $? in $? is


x [ ] ? == $? not in items is b"x" as items as b as x or $? in $? ?? Thing is b [ ] and $? <= $? and $? is

Set [ items ] [ ] ? and
b not in c or
a not in y is
Module [ value ] [ ] ? and $? and


$? <

$? }?. require ( a . items [ x [ x [ a ] ? , b [ b ] ] ] ? )? --accept= x (
( - - y as proc ( ) -> items %


. value?[ .. ] (
) ( ) as pure ( ) -> y is Thing . b . y { c , .. , ..
} as c is
value ? |> sort [ $? >= $? |> group-by { } ?. require ( ) or
x or c ?? a >= $? ] ?. require ( ) or Thing *
$? &
y in items %
$? /
Thing / $? - $? / $? * $? not in


a is items . value ? != $? &
$? *
value * y |
c not in

items / $? % $? is 1.5 ) ) ? |> tee ( )
"####),
        ("normal depth 7 seed 1419", false, r####"cli main ( ) [ ] { ; exit $? >= $? when Thing } run.status --cpumax= 2m --timeout= 0o7 --accept= a (
""?.match?.y?[ .. ]
) | run y = ( $? >= $? ) c = ( $? >= $? ) b = p"p"? (
f"x"?[ .. ] ) < b"x" ( b ) )
"####),
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let tokens = lex_grammar_tokens(source).expect("counterexample lexes");
        let recognizes = recognizer.recognize(&tokens).is_ok();
        let parses = parsed.diagnostics.is_empty();
        eprintln!("{seed}: parses={parses}, recognizes={recognizes}, expected={accepted}");
        if parses != accepted || recognizes != accepted {
            disagreements.push(seed);
        }
    }
    assert!(disagreements.is_empty(), "counterexamples: {disagreements:?}");
}

#[test]
fn discarded_lookahead_attempts_do_not_change_generated_derivations() {
    use xsh::frontend::syntax::grammar::{Rule, Section, Term};
    use xsh::frontend::syntax::token::TokenTag;
    let terminal = |tag, glued| Item::Term(Term { class: Class::Tag(tag), glued });
    let grammar = Grammar { rules: vec![
        Rule { section: Section::Programs, name: "program", body: Item::Seq(vec![
            Item::Not(vec![vec![Term { class: Class::Tag(TokenTag::Ident), glued: false }]]),
            Item::Alt(vec![Item::Rule("discarded"), Item::Rule("retained")]),
            terminal(TokenTag::Newline, false),
        ]) },
        Rule { section: Section::Expressions, name: "discarded", body: Item::Seq(vec![terminal(TokenTag::Ident, false), terminal(TokenTag::Ident, true)]) },
        Rule { section: Section::Expressions, name: "retained", body: terminal(TokenTag::Int, false) },
    ] };
    let recognizer = Recognizer::new(&grammar);
    for seed in 0..32 {
        let mut generator = Generator::new(&grammar);
        let source = generator.sentence(Grammar::START, seed, 4).expect("a retained derivation is spellable");
        recognizer.recognize(&lex_grammar_tokens(&source).expect("retained derivation lexes")).expect("retained derivation is recognized");
        assert!(generator.expanded_rules().contains("retained"));
        assert!(!generator.expanded_rules().contains("discarded"));
        assert!(generator.targeted_sentence(Grammar::START, "discarded", seed, 4).is_none());
    }
}

#[test]
fn multiline_sets_and_command_word_boundaries_are_recognized() {
    for source in ["let values = {x or\ny for a in xs}\n", "let values = {x +\ny, 1}\n", "let values = {x\n+ y, 1}\n", "run --timeout={x or\ny for a in xs} f\n", "run --timeout={x +\ny, 1}(b: 1) f\n", "run --timeout= spawn (x) cmd\n", "env {}\n", "asrunsert seen == []\n", "f []\n", "f )\n", "pure f() { e { }\n", "let value = (spawn run fp\"{a}/x\")? % - collect {}\n", "f(spawn run a?)\n", "(f(spawn run a))\n", "let value = { f\ng }\n"] {
        assert_parser_and_grammar_agree(source, true);
    }
    for source in ["fail - 1 because 2\n", "let value = spawn run fp\"{a}/x\"? % - collect {}\n", "continue unless spawn run e\"HOME\"? as pure () -> Int\n", "f(spawn run a)\n", "f a-b(1)\n", "run (\n cmd ) out )\n", "run --timeout=fp\"x\"[] cmd\n", "run --timeout= spawn (x)\n", "guard let items = run -- ? else { }\n", "{ } ; cd { { }\n", "let value = match x { E => \"\" 0 => 4 }\n"] {
        assert_parser_and_grammar_agree(source, false);
    }
}

#[test]
fn reduced_mutant_counterexamples_have_sound_recognition() {
    for (seed, accepted, source) in [
        (198, true, r####"env {}
"####),
        (535, true, r####"asrunsert seen == []
"####),
        (420, false, r####"pure f()->R[]{match 3{0=>{match 0{0=>{(match 3{E=>""0=>4})}}}}}
"####),
        (500, true, r####"proc u()->t[]{{for i in{[""]:"",[""]:""}{}match 1{V=>{for i in[""or 3in 0]{R=if m{}else{for i in[]{}E{
v=if([[]]){}else{if 0{}else{}}}}}}}}
"####),
        (678, true, r####"proc f()->t[]{{match(""){(2)=>{if.{e]}}}}}
"####),
    ] {
        eprintln!("mutant seed {seed}");
        assert_parser_and_grammar_agree(source, accepted);
    }
}

/// The continuation tokens the parser joins lines on are the grammar's.
#[test]
fn line_continuation_spellings_come_from_the_operator_table() {
    let spellings = line_continuation_spellings();
    for spelling in [
        "??", "or", "and", "==", "!=", "<", "<=", ">", ">=", "in", "not in", "+", "*", "%", ".",
        "|>",
    ] {
        assert!(
            spellings.contains(&spelling),
            "{spelling} does not continue a line"
        );
    }
    assert!(
        !spellings.contains(&"-") && !spellings.contains(&"/"),
        "a line starting with `-` or `/` begins a statement"
    );
}

/// A target-typed `.Name` pattern is accepted inside patterns and after `is`,
/// and never as the head of a match arm; the parser and the productions agree.
#[test]
fn inferred_variant_patterns_are_parsed_and_recognized_alike() {
    let recognizer = Recognizer::new(grammar());
    for source in [
        "match r {\n  Err(.Usage {message}) => f(message)\n  Ok(.Tree(depth, _)) => g(depth)\n  Ok(k.File | .Binary) => h()\n  _ => i()\n}\n",
        "let found = kind is .Binary\n",
        "if kind is .Binary {\n  f()\n}\n",
        "if failure is .Rejected {status: 1, ..} {\n  f()\n}\n",
        "if let .Rejected {status, ..} = failure {\n  f(status)\n}\n",
        "let v = match r { [.A, .B] => 1, {kind: .A} => 2, _ => 3 }\n",
        "let v = match level {\n  Warn if box\n    .ok => 1\n  _ => 2\n}\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "the parser rejects {source:?}: {:?}",
            parsed.diagnostics
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_ok(),
            "the grammar rejects {source:?}"
        );
    }
    for source in [
        "match kind {\n  .File => f()\n  _ => g()\n}\n",
        "match kind {\n  File => f()\n  .Binary => g()\n}\n",
        "let v = match kind { .File => 1, _ => 2 }\n",
        "let v = match kind {\n  File => 1\n  .Tree(depth) => depth\n}\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::ParseInferredVariantArm)),
            "the parser does not report the arm head of {source:?}: {:?}",
            parsed.diagnostics
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_err(),
            "the grammar accepts {source:?}"
        );
    }
}

/// An error family has an `=` form and a brace form with one variant per
/// line; the parser and the productions agree on both and on what the brace
/// form rejects.
#[test]
fn error_family_forms_are_parsed_and_recognized_alike() {
    let recognizer = Recognizer::new(grammar());
    for source in [
        "error E = A(file: Path) : NotFound | B(file: Path, message: Str) | C\n",
        "error E { A }\n",
        "error E {\n    A\n}\n",
        "error E {\n    A\n    B(code: Int) : NotFound, Timeout\n\n    # note\n    C : Timeout # trailing\n}\n",
        "export error E {\n    A(file: Path,\n      message: Str)\n    B\n}\n",
        "error E {\n    A\n}\nlet x = 1\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "the parser rejects {source:?}: {:?}",
            parsed.diagnostics
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_ok(),
            "the grammar rejects {source:?}"
        );
    }
    for source in [
        "error E { }\n",
        "error E {\n}\n",
        "error E { A, B }\n",
        "error E {\n    A,\n    B\n}\n",
        "error E { A | B }\n",
        "error E { A B }\n",
        "error E {\n    A : NotFound,\n    B\n}\n",
    ] {
        assert!(
            !Parser::parse_source_arena_only(SourceId::new(0), source)
                .diagnostics
                .is_empty(),
            "the parser accepts {source:?}"
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_err(),
            "the grammar accepts {source:?}"
        );
    }
}

/// `else =>` is the catch-all arm of a `match` statement or expression: the
/// parser and the productions agree that it is the last arm, takes no guard,
/// and is not a pattern.
#[test]
fn match_else_arm_forms_are_parsed_and_recognized_alike() {
    let recognizer = Recognizer::new(grammar());
    for source in [
        "match x { else => f() }\n",
        "match x {\n  1 => f()\n  else => g()\n}\n",
        "match x {\n  1 => f(),\n  else => g(),\n}\n",
        "match x {\n  1 => { f() }\n  else => { g() }\n\n}\n",
        "match x { 1 => f(), else => {} }\n",
        "match x {\n  1 => if y { f() }\n  else => g()\n}\n",
        "match x { 1 => if y { f() } else => g() }\n",
        "match x {\n  _ if y => f()\n  else => return 1\n}\n",
        "let v = match x { 1 => 2, else => 3 }\n",
        "let v = match x {\n  1 => 2\n  else => 3\n}\n",
        "let v = match x {\n  1 => 2,\n  else => { 3 },\n\n}\n",
        "let v = match x { else => {a: 1} }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "the parser rejects {source:?}: {:?}",
            parsed.diagnostics
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_ok(),
            "the grammar rejects {source:?}"
        );
    }
    for source in [
        "match x {\n  else => f()\n  1 => g()\n}\n",
        "match x {\n  else => f()\n  else => g()\n}\n",
        "match x {\n  1 => f()\n  else if y => g()\n}\n",
        "match x { Some(else) => f() }\n",
        "match x { 1 | else => f() }\n",
        "let v = match x { else => 1, 2 => 3 }\n",
        "let v = match x { 1 => 2, else if y => 3 }\n",
    ] {
        assert!(
            !Parser::parse_source_arena_only(SourceId::new(0), source)
                .diagnostics
                .is_empty(),
            "the parser accepts {source:?}"
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_err(),
            "the grammar accepts {source:?}"
        );
    }
}

/// A statement that begins `name as` or `name.field as` is a conversion, as
/// one that begins `name is` is a test; neither is a command. The parser and
/// the productions agree, and a quoted `"as"` is still a command argument.
#[test]
fn statement_start_conversions_are_parsed_and_recognized_alike() {
    use xsh::frontend::syntax::arena::{ArenaExprKind, ArenaStmtKind};
    let recognizer = Recognizer::new(grammar());
    for (source, converts) in [
        ("count as UInt\n", true),
        ("row.cell as Int\n", true),
        ("row.cell.text as Int\n", true),
        ("count as UInt as Int\n", true),
        ("count as Int + 1\n", false),
        // `exit` and `fail` are names wherever they do not begin their
        // statements, and an `as` after one continues an expression.
        ("fail as Int\n", true),
        ("exit as Int\n", true),
        ("fail is Int\n", false),
        ("exit is Int\n", false),
        ("exit as Int when ready\n", false),
        ("proc f(count: Str) {\n  count as Int\n}\n", false),
        ("let kept = try { count as Int }\n", false),
        ("count \"as\" UInt\n", false),
        ("row.cell \"as\" Int\n", false),
        ("print $as\n", false),
        ("print $is\n", false),
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "the parser rejects {source:?}: {:?}",
            parsed.diagnostics
        );
        let first = parsed
            .arena
            .statement_ids()
            .next()
            .map(|stmt| parsed.arena.arena.stmt(stmt).kind);
        let arena = &parsed.arena.arena;
        let converted = matches!(
            first,
            Some(ArenaStmtKind::Expr(expr))
                if matches!(arena.expr(expr).kind, ArenaExprKind::Convert { .. })
        );
        assert_eq!(converted, converts, "{source:?}: {first:?}");
        assert!(
            !source.contains(['"', '$']) || matches!(first, Some(ArenaStmtKind::Command(_))),
            "{source:?}: {first:?}"
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_ok(),
            "the grammar rejects {source:?}"
        );
    }
    // The word after `as` is a type, never a second command argument.
    for source in ["count as\n", "count as 4\n", "row.cell as \"Int\"\n"] {
        assert!(
            !Parser::parse_source_arena_only(SourceId::new(0), source)
                .diagnostics
                .is_empty(),
            "the parser accepts {source:?}"
        );
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(
            recognizer.recognize(&tokens).is_err(),
            "the grammar accepts {source:?}"
        );
    }
}
