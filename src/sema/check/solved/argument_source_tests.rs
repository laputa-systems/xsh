use super::*;
use crate::sema::arguments::ArgumentValueSource;
use crate::sema::check::Checker;
use crate::syntax::parser::Parser;

#[test]
fn argument_recipe_validation_rejects_reordered_shared_and_foreign_sources_after_frontend_drop() {
    let source = "pure pair(first: Int, second: Int) -> Int { first + second }\nlet fields = {first: 1, second: 2}\nlet result = pair(...fields)\nlet ordinary = pair(1, 2)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(97), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let spread = *checked.solved.argument_sources.keys().find(|identity| {
        &source[parsed.arena.arena.expr(identity.expression).span.range()] == "pair(...fields)"
    }).unwrap();
    let ordinary = *checked.solved.argument_sources.keys().find(|identity| {
        &source[parsed.arena.arena.expr(identity.expression).span.range()] == "pair(1, 2)"
    }).unwrap();
    drop(parsed);
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    let _symbols = solved.symbols.enter();
    solved.validate().unwrap();
    let spread_rows = solved.argument_sources[&spread].clone();
    let ordinary_rows = solved.argument_sources[&ordinary].clone();

    solved.argument_sources.get_mut(&ordinary).unwrap().swap(0, 1);
    assert!(solved.validate().is_err(), "argument evaluation order is the original entry order");
    solved.argument_sources.insert(ordinary, ordinary_rows.clone());

    let rows = solved.argument_sources.get_mut(&spread).unwrap();
    rows[1].value = ordinary_rows[0].value;
    assert!(solved.validate().is_err(), "one spread entry cannot read a different expression for its second field");
    solved.argument_sources.insert(spread, spread_rows.clone());

    let rows = solved.argument_sources.get_mut(&spread).unwrap();
    rows[1].name = rows[0].name;
    assert!(solved.validate().is_err(), "spread field identity and destination label must agree");
    solved.argument_sources.insert(spread, spread_rows.clone());

    solved.argument_sources.get_mut(&ordinary).unwrap()[0].span.source_id = SourceId::new(98);
    assert!(solved.validate().is_err(), "argument spans cannot escape their source owner");
    solved.argument_sources.insert(ordinary, ordinary_rows.clone());

    let mut foreign = ordinary;
    foreign.namespace = Some(Name::intern("foreign"));
    let rows = solved.argument_sources.remove(&ordinary).unwrap();
    solved.argument_sources.insert(foreign, rows);
    assert!(solved.validate().is_err(), "source expression IDs are qualified by the checked namespace");
    solved.argument_sources.remove(&foreign);
    solved.argument_sources.insert(ordinary, ordinary_rows);

    let rows = solved.argument_sources.get_mut(&spread).unwrap();
    rows[1].value = rows[0].value;
    rows[1].name = rows[0].name;
    assert!(matches!(rows[0].value, ArgumentValueSource::RecordField { .. }));
    assert!(solved.validate().is_err(), "one finite record field cannot be evaluated twice as two supplied arguments");
    solved.argument_sources.insert(spread, spread_rows);
    solved.validate().unwrap();
}

#[test]
fn invocation_recipe_modes_and_missing_call_recipes_reject_after_frontend_drop() {
    let source = "pure first(value: Int) -> Int { value }\npure second(value: Int) -> Int { value + 1 }\npure choose(flag: Bool) { if flag { (first) } else { (second) } }\nlet callback = choose(true)\nlet result = callback(value: 7)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(99), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let dynamic = *checked.solved.invocations.keys().find(|identity| {
        &source[parsed.arena.arena.expr(identity.expression).span.range()] == "callback(value: 7)"
    }).unwrap();
    let ordinary = *checked.solved.calls.keys().find(|identity| {
        &source[parsed.arena.arena.expr(identity.expression).span.range()] == "choose(true)"
    }).unwrap();
    drop(parsed);
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    let _symbols = solved.symbols.enter();
    solved.validate().unwrap();
    let original = solved.argument_sources[&dynamic].clone();
    solved.argument_sources.get_mut(&dynamic).unwrap()[0].name = None;
    assert!(solved.validate().is_err(), "invocation branch binding cannot change a named source into a positional source");
    solved.argument_sources.insert(dynamic, original);
    let original = solved.argument_sources.remove(&ordinary).unwrap();
    assert!(solved.validate().is_err(), "a checked declaration call requires its original argument recipe");
    solved.argument_sources.insert(ordinary, original);
    solved.validate().unwrap();
}
