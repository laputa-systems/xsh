#![allow(clippy::single_call_fn)]

use std::collections::BTreeSet;
use std::sync::Arc;

use xsh::frontend::check::{CheckOptions, CheckOutput, Checker, Type};
use xsh::frontend::source::{SourceId, SourceMap};
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaProgramBuilder};
use xsh::frontend::syntax::node::Effect;
use xsh::frontend::syntax::parser::Parser;

fn prepared_sources(output: &CheckOutput, program: &ArenaProgram) -> BTreeSet<SourceId> {
    output
        .prepared_constants
        .values
        .keys()
        .map(|expression| program.arena.expr(*expression).span.source_id)
        .collect()
}

#[test]
fn checker_preparation_is_scoped_to_each_root_in_a_shared_arena() {
    let mut sources = SourceMap::new();
    let alpha_entry_source = "##! Alpha entry.\nuse alpha\nuse unrelated\nproc main(...argv: List[Bytes]) {}\nconst root_alpha = 11\n";
    let other_entry_source = "##! Other entry.\nuse alpha\nconst root_other = 13\nproc main(...argv: List[Str]) {}\n";
    let alpha_module_source = "##! Alpha module.\npure incoming() { args }\n## Observe wall-clock time.\nexport proc observe() { let _ = time.now() }\nconst alpha_value = 7\n";
    let unrelated_module_source = "##! Unrelated module.\nconst unrelated_value = 17\nconst invalid = 1 / 0\n";
    let alpha_entry_id = sources.add_file("alpha-entry.xsh", alpha_entry_source);
    let other_entry_id = sources.add_file("other-entry.xsh", other_entry_source);
    let alpha_module_id = sources.add_file("alpha.xsh", alpha_module_source);
    let unrelated_module_id = sources.add_file("unrelated.xsh", unrelated_module_source);

    let mut builder = ArenaProgramBuilder::with_token_capacity(128);
    let alpha_entry = Parser::parse_source_into_arena_builder(
        alpha_entry_id,
        alpha_entry_source,
        &mut builder,
    );
    let other_entry = Parser::parse_source_into_arena_builder(
        other_entry_id,
        other_entry_source,
        &mut builder,
    );
    let alpha_module = Parser::parse_source_into_arena_builder(
        alpha_module_id,
        alpha_module_source,
        &mut builder,
    );
    let unrelated_module = Parser::parse_source_into_arena_builder(
        unrelated_module_id,
        unrelated_module_source,
        &mut builder,
    );
    for parsed in [&alpha_entry, &other_entry, &alpha_module, &unrelated_module] {
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    }

    let alpha_name = builder.symbol_owner().intern("alpha");
    builder.push_arena_module("alpha".to_string(), alpha_name, alpha_module.statements);
    let unrelated_name = builder.symbol_owner().intern("unrelated");
    builder.push_arena_module(
        "unrelated".to_string(),
        unrelated_name,
        unrelated_module.statements,
    );
    for (statements, modules) in [
        (alpha_entry.statements, ["alpha", "unrelated"].as_slice()),
        (other_entry.statements, ["alpha"].as_slice()),
    ] {
        let mut modules = modules.iter();
        for statement in builder.statement_ids(statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from(*modules.next().unwrap()));
            }
        }
    }

    let mut program = builder.finish_with_statements(alpha_entry.statements);
    let type_program = Arc::new(program.clone());
    let alpha_check = Checker::check_arena_with_sources_options_and_type_program(
        &program,
        &sources,
        alpha_entry_id,
        CheckOptions::default(),
        type_program.clone(),
    );
    assert_eq!(alpha_check.diagnostics.len(), 1, "{:?}", alpha_check.diagnostics);
    assert_eq!(
        alpha_check.diagnostics[0].labels[0].span.source_id,
        unrelated_module_id
    );
    assert!(alpha_check
        .function_return_types
        .iter()
        .any(|(span, ty)| span.source_id == alpha_module_id
            && ty == &Type::List(Box::new(Type::Bytes))));
    assert!(alpha_check.function_effect_facts.values().any(|fact| {
        fact.effective
            .as_ref()
            .is_some_and(|effects| effects.contains(&Effect::Time))
    }));
    let alpha_sources = prepared_sources(&alpha_check, &program);
    assert!(alpha_sources.contains(&alpha_entry_id));
    assert!(alpha_sources.contains(&alpha_module_id));
    assert!(alpha_sources.contains(&unrelated_module_id));
    assert!(!alpha_sources.contains(&other_entry_id));

    program.statements = other_entry.statements;
    let other_check = Checker::check_arena_with_sources_options_and_type_program(
        &program,
        &sources,
        other_entry_id,
        CheckOptions::default(),
        type_program,
    );
    assert!(other_check.diagnostics.is_empty(), "{:?}", other_check.diagnostics);
    assert!(other_check
        .function_return_types
        .iter()
        .any(|(span, ty)| span.source_id == alpha_module_id
            && ty == &Type::List(Box::new(Type::Str))));
    assert!(other_check.function_effect_facts.values().any(|fact| {
        fact.effective
            .as_ref()
            .is_some_and(|effects| effects.contains(&Effect::Time))
    }));
    let other_sources = prepared_sources(&other_check, &program);
    assert!(other_sources.contains(&other_entry_id));
    assert!(!other_sources.contains(&alpha_entry_id));
    assert!(other_sources.contains(&alpha_module_id));
    assert!(!other_sources.contains(&unrelated_module_id));
}
