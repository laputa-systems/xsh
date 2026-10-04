//! The grammar in `src/syntax/grammar.rs` and the parser accept the same
//! language: sentences generated from the productions parse without
//! diagnostics, and every checked-in source the parser accepts is a sentence
//! of the productions.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::grammar::earley::{Recognizer, top_level_parts};
use xsh::frontend::syntax::grammar::generate::{Generator, NAMES};
use xsh::frontend::syntax::grammar::{
    Class, Grammar, Item, STATEMENT_KEYWORDS, STREAM_STAGES, grammar, lex_grammar_tokens, line_continuation_spellings, stream_stage,
};
use xsh::frontend::syntax::parser::Parser;

/// Fixed seeds and depths: shallow sentences cover every production's
/// shortest forms, deeper ones nest them.
const SENTENCE_SEEDS: u64 = 1500;
const SENTENCE_DEPTHS: [u32; 3] = [4, 7, 10];

fn workers() -> usize {
    std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get).clamp(1, 4)
}

/// Runs `body` over `items` on a few threads, collecting failure reports.
fn for_each_parallel<T: Sync>(items: &[T], body: impl Fn(&T) -> Option<String> + Sync) -> Vec<String> {
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
        Item::Seq(items) | Item::Alt(items) => items.iter().for_each(|item| rule_references(item, out)),
        Item::Opt(inner) | Item::Star(inner) | Item::Plus(inner) | Item::Line(inner) => rule_references(inner, out),
        Item::List { item, .. } => rule_references(item, out),
        Item::Term(_) | Item::Not(_) | Item::Peek(_) => {}
    }
}

fn words(item: &Item, out: &mut Vec<&'static str>) {
    let mut terms = Vec::new();
    match item {
        Item::Term(term) => terms.push(*term),
        Item::Not(sequences) | Item::Peek(sequences) => terms.extend(sequences.iter().flatten().copied()),
        Item::Seq(items) | Item::Alt(items) => items.iter().for_each(|item| words(item, out)),
        Item::Opt(inner) | Item::Star(inner) | Item::Plus(inner) | Item::Line(inner) => words(inner, out),
        Item::List { item, .. } => words(item, out),
        Item::Rule(_) => {}
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
        let rule = grammar.rule(name).unwrap_or_else(|| panic!("rule `{name}` is referenced but not defined"));
        let mut references = Vec::new();
        rule_references(&rule.body, &mut references);
        for reference in references {
            if !reachable.contains(&reference) {
                reachable.push(reference);
            }
        }
        index += 1;
    }
    let unreachable: Vec<&str> = grammar.rules.iter().map(|rule| rule.name).filter(|name| !reachable.contains(name)).collect();
    assert!(unreachable.is_empty(), "rules unreachable from `program`: {unreachable:?}");
    assert_eq!(Generator::new(grammar).underivable_rules(), Vec::<&str>::new(), "rules with no finite derivation");
    // Generated names must never spell a contextual word, or a sentence
    // could dispatch differently from the production that built it.
    let mut spelled = Vec::new();
    for rule in &grammar.rules {
        words(&rule.body, &mut spelled);
    }
    let clashes: Vec<&str> = NAMES.iter().copied().filter(|name| spelled.contains(name)).collect();
    assert!(clashes.is_empty(), "generator names that the grammar spells as words: {clashes:?}");
    // `stream_stage` indexes the stage table by kind.
    for stage in &STREAM_STAGES {
        assert_eq!(stream_stage(stage.kind).name, stage.name, "the stage table is out of `StreamStageKind` order");
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
        let leads: Vec<&str> = STATEMENT_KEYWORDS.iter().filter(|(_, other)| *other == form).map(|(keyword, _)| keyword.as_str()).collect();
        assert!(leads.contains(&keyword.as_str()));
        for seed in 0..16 {
            let Some(sentence) = generator.sentence(rule, seed, 3) else { continue };
            let first = sentence.split_whitespace().next().unwrap_or("");
            let first = first.split(['.', '(']).next().unwrap_or("");
            assert!(leads.contains(&first), "`{rule}` sentence does not begin with {leads:?}: {sentence}");
        }
    }
}

/// Seeds per rule for candidates that take the shortest way to that rule,
/// so every production is exercised.
const TARGETED_SEEDS: u64 = 6;

#[test]
fn grammar_generated_sentences_parse_without_diagnostics() {
    let started = Instant::now();
    let grammar = grammar();
    let recognizer = Recognizer::new(grammar);
    let mut jobs: Vec<(u32, u64, Option<&'static str>)> =
        SENTENCE_DEPTHS.iter().flat_map(|depth| (0..SENTENCE_SEEDS).map(move |seed| (*depth, seed, None))).collect();
    jobs.extend(grammar.rules.iter().flat_map(|rule| (0..TARGETED_SEEDS).map(move |seed| (4, seed, Some(rule.name)))));
    let sentences = AtomicUsize::new(0);
    let covered = Mutex::new(std::collections::BTreeSet::new());
    let failures = for_each_parallel(&jobs, |&(depth, seed, target)| {
        let mut generator = Generator::new(grammar);
        let source = match target {
            Some(target) => generator.targeted_sentence(Grammar::START, target, seed, depth)?,
            None => generator.sentence(Grammar::START, seed, depth)?,
        };
        // A candidate that misses a lookahead or lexes differently is not a
        // sentence of the grammar.
        let tokens = lex_grammar_tokens(&source)?;
        recognizer.recognize(&tokens).ok()?;
        sentences.fetch_add(1, Ordering::Relaxed);
        covered.lock().unwrap().extend(generator.expanded_rules().iter().copied());
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        let diagnostic = parsed.diagnostics.first()?;
        Some(format!("depth {depth} seed {seed} target {target:?}: [{}] {}\n{source}", diagnostic.code.as_deref().unwrap_or(""), diagnostic.message))
    });
    let sentences = sentences.into_inner();
    eprintln!("grammar generation: {sentences} sentences of {} candidates in {:?}", jobs.len(), started.elapsed());
    assert!(failures.is_empty(), "{} generated sentences failed to parse:\n\n{}", failures.len(), failures.join("\n\n"));
    assert!(sentences * 10 >= jobs.len() * 8, "only {sentences} of {} candidates were sentences", jobs.len());
    let covered = covered.into_inner().unwrap();
    let missing: Vec<&str> = grammar.rules.iter().map(|rule| rule.name).filter(|name| !covered.contains(name)).collect();
    assert!(missing.is_empty(), "rules no generated sentence used: {missing:?}");
}

fn xsh_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).expect("read corpus directory") {
        let entry = entry.expect("read corpus entry");
        let path = entry.path();
        if entry.file_type().expect("corpus entry type").is_dir() {
            if !matches!(entry.file_name().to_str(), Some("target" | ".git" | ".claude")) {
                xsh_files(&path, out);
            }
        } else if path.extension().is_some_and(|extension| extension == "xsh") {
            out.push(path);
        }
    }
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
        if !Parser::parse_source_arena_only(SourceId::new(0), &source).diagnostics.is_empty() {
            return None;
        }
        let tokens = lex_grammar_tokens(&source).expect("a parsed source lexes cleanly");
        tokens_seen.fetch_add(tokens.len(), Ordering::Relaxed);
        for part in top_level_parts(&tokens) {
            if let Err(rejection) = recognizer.recognize(part) {
                let near: Vec<&str> = part[rejection.token.saturating_sub(4)..(rejection.token + 3).min(part.len())].iter().map(|token| token.text).collect();
                return Some(format!("{}: token {:?} near {near:?}; expected {}", path.display(), part.get(rejection.token).map(|token| token.text), rejection.expected.join(" ")));
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
    assert!(failures.is_empty(), "{} parsed sources are not grammar sentences:\n{}", failures.len(), failures.join("\n"));
    assert!(recognized > 500, "only {recognized} corpus files were recognized");
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
    ] {
        assert!(!Parser::parse_source_arena_only(SourceId::new(0), source).diagnostics.is_empty(), "the parser accepts {source:?}");
        let tokens = lex_grammar_tokens(source).expect("lexes");
        assert!(recognizer.recognize(&tokens).is_err(), "the grammar accepts {source:?}");
    }
}

/// The continuation tokens the parser joins lines on are the grammar's.
#[test]
fn line_continuation_spellings_come_from_the_operator_table() {
    let spellings = line_continuation_spellings();
    for spelling in ["??", "or", "and", "==", "!=", "<", "<=", ">", ">=", "in", "not in", "+", "*", "%", ".", "|>"] {
        assert!(spellings.contains(&spelling), "{spelling} does not continue a line");
    }
    assert!(!spellings.contains(&"-") && !spellings.contains(&"/"), "a line starting with `-` or `/` begins a statement");
}
