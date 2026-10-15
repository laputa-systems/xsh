#![allow(clippy::single_call_fn)]

use std::collections::BTreeSet;
use std::sync::Arc;

use xsh::diagnostic::DiagnosticCode;
use xsh::frontend::check::{
    AnnotationFactKind, CheckBundleView, CheckOptions, CheckOutput, CheckSession, Checker, Type,
};
use xsh::frontend::source::{SourceId, SourceMap};
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaProgramBuilder};
use xsh::frontend::syntax::grouping::grouping_diagnostics_in_range;
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
fn embedded_module_records_do_not_depend_on_unimported_embedded_modules() {
    let mut sources = SourceMap::new();
    let entry_source = "1\n";
    let hash_source = "const hash_value = 1\n";
    let json_source = "const json_value = 2\n";
    let entry_id = sources.add_file("entry.xsh", entry_source);
    let hash_id = sources.add_file("hash.xsh", hash_source);
    let json_id = sources.add_file("json.xsh", json_source);
    let mut builder = ArenaProgramBuilder::with_token_capacity(64);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let hash = Parser::parse_source_into_arena_builder(hash_id, hash_source, &mut builder);
    let json = Parser::parse_source_into_arena_builder(json_id, json_source, &mut builder);
    for parsed in [&entry, &hash, &json] {
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    }
    let hash_name = builder.symbol_owner().intern("hash");
    builder.push_internal_arena_module("hash".to_string(), hash_name, hash.statements);
    let json_name = builder.symbol_owner().intern("json");
    builder.push_internal_arena_module("json".to_string(), json_name, json.statements);
    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let view = CheckBundleView::for_root(&program, entry.statements, Some(&sources), Some(entry_id));
    let output = Checker::check_bundle_view_with_type_program(
        &view,
        entry_source,
        CheckOptions::default(),
        Arc::clone(&program),
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn suppressed_embedded_record_constructors_are_not_published() {
    let mut sources = SourceMap::new();
    let entry_source = "1\n";
    let module_source = "##! Embedded record constructors.\n## A generic record.\nexport type Box[T] = {value: T}\n## Constructs one box.\nexport pure make() -> Box[Int] { Box(value: 7) }\n";
    let entry_id = sources.add_file("entry.xsh", entry_source);
    let module_id = sources.add_file("embedded.xsh", module_source);
    let mut builder = ArenaProgramBuilder::with_token_capacity(96);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    let module_name = builder.symbol_owner().intern("embedded");
    builder.push_internal_arena_module("embedded".to_string(), module_name, module.statements);

    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let module = program
        .modules
        .iter()
        .find(|module| module.key == "embedded")
        .expect("embedded module was registered");
    let type_id = program
        .module_statements(module)
        .find_map(|statement| match program.arena.stmt(statement).kind {
            xsh::frontend::syntax::arena::ArenaStmtKind::Export(inner) => {
                match program.arena.stmt(inner).kind {
                    xsh::frontend::syntax::arena::ArenaStmtKind::TypeDef(type_id) => Some(type_id),
                    _ => None,
                }
            }
            _ => None,
        })
        .expect("embedded module declares Box");
    let sources = Arc::new(sources);
    let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
    let default_output = session.check(entry.statements, entry_id, CheckOptions::default());
    assert!(!default_output.embedded_bodies_checked);
    let instance = format!("({type_id:?}, [Int], Record");
    assert!(
        !format!("{:?}", default_output.checked_declarations.record_constructors)
            .contains(&instance),
        "default checks suppress the embedded constructor body; found {instance}"
    );

    let execution_output = session.check(
        entry.statements,
        entry_id,
        CheckOptions::for_execution(),
    );
    assert!(execution_output.embedded_bodies_checked);
    assert!(
        format!("{:?}", execution_output.checked_declarations.record_constructors)
            .contains(&instance),
        "execution checks publish the embedded constructor body; expected {instance}"
    );

    let default_output = session.check(entry.statements, entry_id, CheckOptions::default());
    assert!(!default_output.embedded_bodies_checked);
    assert!(
        !format!("{:?}", default_output.checked_declarations.record_constructors)
            .contains(&instance),
        "projecting a default check must not mutate the cached execution facts"
    );
}

#[test]
fn suppressed_embedded_modules_keep_declaration_effect_facts() {
    let mut sources = SourceMap::new();
    let entry_source = "1\n";
    let module_source = "##! Embedded effect declarations.\n## Declares an effectful operation.\nexport proc read_data() [fs] -> Result[Unit] { return Ok() }\n";
    let entry_id = sources.add_file("entry.xsh", entry_source);
    let module_id = sources.add_file("embedded.xsh", module_source);
    let mut builder = ArenaProgramBuilder::with_token_capacity(96);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    let module_name = builder.symbol_owner().intern("embedded");
    builder.push_internal_arena_module("embedded".to_string(), module_name, module.statements);

    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let sources = Arc::new(sources);
    let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
    let output = session.check(entry.statements, entry_id, CheckOptions::default());
    let module_effect_facts = output
        .function_effect_facts
        .iter()
        .filter(|(identity, _)| identity.body.source_id == module_id)
        .map(|(_, fact)| fact)
        .collect::<Vec<_>>();
    assert_eq!(module_effect_facts.len(), 1, "{:?}", output.function_effect_facts);
    assert_eq!(module_effect_facts[0].effective, Some(vec![Effect::Fs]));
    assert_eq!(module_effect_facts[0].required, Some(Vec::new()));
    assert!(!output.embedded_bodies_checked);
}

#[test]
fn suppressed_embedded_modules_keep_resource_transfer_facts() {
    let mut sources = SourceMap::new();
    let entry_source = "1\n";
    let module_source = "##! Embedded local transfer.\n## Assigns to a local variable.\nexport proc scalar() -> Int { var value = 0; value = 1; return value }\n";
    let entry_id = sources.add_file("entry.xsh", entry_source);
    let module_id = sources.add_file("embedded.xsh", module_source);
    let mut builder = ArenaProgramBuilder::with_token_capacity(96);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let module_parse = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module_parse.diagnostics.is_empty(), "{:?}", module_parse.diagnostics);
    let module_name = builder.symbol_owner().intern("embedded");
    builder.push_internal_arena_module(
        "embedded".to_string(),
        module_name,
        module_parse.statements,
    );

    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let module = program
        .modules
        .iter()
        .find(|module| module.key == "embedded")
        .expect("embedded module was registered");
    let assignment = program
        .module_statements(module)
        .find_map(|statement| {
            let definition = match program.arena.stmt(statement).kind {
                xsh::frontend::syntax::arena::ArenaStmtKind::Export(inner) => {
                    program.arena.stmt(inner).kind
                }
                kind => kind,
            };
            let xsh::frontend::syntax::arena::ArenaStmtKind::ProcDef(function) = definition else {
                return None;
            };
            let body = program.arena.function_def(function).body;
            program
                .arena
                .stmt_ids(program.arena.block(body).statements)
                .find(|&statement| matches!(
                    program.arena.stmt(statement).kind,
                    xsh::frontend::syntax::arena::ArenaStmtKind::Assign { .. }
                ))
        })
        .expect("embedded procedure assigns to a local");
    let session = CheckSession::new(Arc::clone(&program), Arc::new(sources));
    let output = session.check(entry.statements, entry_id, CheckOptions::default());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(!output.embedded_bodies_checked);
    assert_eq!(
        output.resource_transfer(xsh::frontend::check::ResourceTransferSite::Assignment(assignment)),
        Some(false),
        "a suppressed embedded body still has a published local transfer result"
    );
}

#[test]
fn internal_module_dependencies_remain_dependency_first() {
    let mut sources = SourceMap::new();
    let entry_source = "use owner\n";
    let owner_source = "use dependency\n";
    let dependency_source = "use base\nconst value = 1\n";
    let base_source = "const base_value = 0\n";
    let entry_id = sources.add_file("entry.xsh", entry_source);
    let owner_id = sources.add_file("owner.xsh", owner_source);
    let dependency_id = sources.add_file("dependency.xsh", dependency_source);
    let base_id = sources.add_file("base.xsh", base_source);
    let mut builder = ArenaProgramBuilder::with_token_capacity(64);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let owner = Parser::parse_source_into_arena_builder(owner_id, owner_source, &mut builder);
    let dependency =
        Parser::parse_source_into_arena_builder(dependency_id, dependency_source, &mut builder);
    let base = Parser::parse_source_into_arena_builder(base_id, base_source, &mut builder);
    for parsed in [&entry, &owner, &dependency, &base] {
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    }
    let owner_name = builder.symbol_owner().intern("owner");
    builder.push_internal_arena_module("owner".to_string(), owner_name, owner.statements);
    let dependency_name = builder.symbol_owner().intern("dependency");
    builder.push_internal_arena_module(
        "dependency".to_string(),
        dependency_name,
        dependency.statements,
    );
    let base_name = builder.symbol_owner().intern("base");
    builder.push_internal_arena_module("base".to_string(), base_name, base.statements);
    for (statements, key) in [
        (entry.statements, "owner"),
        (owner.statements, "dependency"),
        (dependency.statements, "base"),
    ] {
        for statement in builder.statement_ids(statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from(key));
            }
        }
    }

    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let view = CheckBundleView::for_root(&program, entry.statements, Some(&sources), Some(entry_id));
    assert_eq!(
        view.modules().map(|module| module.key.as_str()).collect::<Vec<_>>(),
        ["base", "dependency", "owner"]
    );
    let _output = Checker::check_bundle_view_with_type_program(
        &view,
        entry_source,
        CheckOptions::default(),
        Arc::clone(&program),
    );
}

#[test]
fn checked_entry_retains_record_constructor_instances_from_module_bodies() {
    let mut sources = SourceMap::new();
    let entry_source = "use shared\n";
    let shared_source = "##! Shared record constructors.\ntype Box[T] = {value: T}\n## Make values with two record instances.\nexport pure make() -> Int { let _ = Box(value: 1); let _ = Box(value: \"one\"); 1 }\n";
    let entry_id = sources.add_file("entry.xsh", entry_source);
    let shared_id = sources.add_file("shared.xsh", shared_source);
    let mut builder = ArenaProgramBuilder::with_token_capacity(128);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let shared = Parser::parse_source_into_arena_builder(shared_id, shared_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(shared.diagnostics.is_empty(), "{:?}", shared.diagnostics);
    let shared_name = builder.symbol_owner().intern("shared");
    builder.push_arena_module("shared".to_string(), shared_name, shared.statements);
    for statement in builder.statement_ids(entry.statements) {
        if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
            builder.set_use_resolved(use_id, Arc::from("shared"));
        }
    }

    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let sources = Arc::new(sources);
    let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
    let view = session.view_for_root(entry.statements, entry_id);
    let output = session.check_view(&view, CheckOptions::default());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let constructors = format!("{:?}", output.checked_declarations.record_constructors);
    assert!(
        constructors.contains("data: [(TypeDefId(1), [Int], Record"),
        "the entry publication includes the integer record instance from the imported module body; found {constructors}"
    );
    assert!(
        constructors.contains("(TypeDefId(1), [Str], Record"),
        "the entry publication includes the string record instance from the imported module body; found {constructors}"
    );
}

#[test]
fn internal_resource_diagnostics_follow_embedded_body_publication() {
    let mut sources = SourceMap::new();
    let entry_source = "1\n";
    let module_source = "##! Managed resource example.\n## Reports a use after release.\nexport proc bad() [fs, error] -> Result[Unit] { let root = fs.tempdir()?; root.close()?; root.close()?; return Ok() }\n";
    let entry_id = sources.add_file("entry.xsh", entry_source);
    let module_id = sources.add_file("owned.xsh", module_source);
    let mut builder = ArenaProgramBuilder::with_token_capacity(96);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    let module_name = builder.symbol_owner().intern("owned");
    builder.push_internal_arena_module("owned".to_string(), module_name, module.statements);

    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let sources = Arc::new(sources);
    let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
    let count_resource_diagnostics = |output: &CheckOutput| {
        output
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == Some(DiagnosticCode::CheckUseAfterRelease)
                    && diagnostic.labels.iter().any(|label| label.span.source_id == module_id)
            })
            .count()
    };
    let output = session.check(entry.statements, entry_id, CheckOptions::default());
    assert!(!output.embedded_bodies_checked);
    assert_eq!(count_resource_diagnostics(&output), 0, "{:?}", output.diagnostics);
    let output = session.check(entry.statements, entry_id, CheckOptions::for_execution());
    assert!(output.embedded_bodies_checked);
    assert_eq!(count_resource_diagnostics(&output), 1, "{:?}", output.diagnostics);
}

#[test]
#[should_panic(expected = "session checks require a view created by this session")]
fn check_session_rejects_a_module_view_that_aliases_an_entry_range() {
    let mut sources = SourceMap::new();
    let entry_id = sources.add_file("entry.xsh", "1\n");
    let source = sources.get(entry_id).unwrap().text();
    let mut builder = ArenaProgramBuilder::with_token_capacity(16);
    let entry = Parser::parse_source_into_arena_builder(entry_id, source, &mut builder);
    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let sources = Arc::new(sources);
    let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
    let root_view = session.view_for_root(entry.statements, entry_id);
    session.check_view(&root_view, CheckOptions::default());
    let view = CheckBundleView::for_module(
        &program,
        entry.statements,
        Some(&sources),
        Some(entry_id),
    );
    session.check_view(&view, CheckOptions::default());
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

    let program = builder.finish_with_statements(alpha_entry.statements);
    let type_program = Arc::new(program.clone());
    let alpha_view = CheckBundleView::for_root(
        &program,
        alpha_entry.statements,
        Some(&sources),
        Some(alpha_entry_id),
    );
    let alpha_check = Checker::check_bundle_view_with_type_program(
        &alpha_view,
        alpha_entry_source,
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

    let other_view = CheckBundleView::for_root(
        &program,
        other_entry.statements,
        Some(&sources),
        Some(other_entry_id),
    );
    assert_eq!(alpha_view.modules().count(), 2);
    assert_eq!(other_view.modules().count(), 1);
    assert_eq!(program.statements, alpha_entry.statements);
    let other_check = Checker::check_bundle_view_with_type_program(
        &other_view,
        other_entry_source,
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

#[test]
fn grouping_diagnostics_match_standalone_sources_in_a_shared_arena() {
    let entry_source = "let root_value = (1 + 2)\n";
    let module_source = "pure calculate(x: Int) -> Int {\n  let values = {(x + 1) for item in [1, 2]}\n  let selected = match x {\n    0 => ((2 + 3)),\n    _ => (4 + 5),\n  }\n  return (selected + x) when x > 0\n}\n";
    let unrelated_source = "let foreign = {(1 + 2), (3 + 4)}\nif true { let nested = (5 + 6) }\n";
    let entry_id = SourceId::new(0);
    let module_id = SourceId::new(1);
    let unrelated_id = SourceId::new(2);
    let mut builder = ArenaProgramBuilder::with_token_capacity(128);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
    let unrelated =
        Parser::parse_source_into_arena_builder(unrelated_id, unrelated_source, &mut builder);
    for parsed in [&entry, &module, &unrelated] {
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    }
    let module_name = builder.symbol_owner().intern("grouping");
    builder.push_internal_arena_module("grouping".to_string(), module_name, module.statements);
    let shared = builder.finish_with_statements(entry.statements);

    for (source_id, source, shared_statements) in [
        (entry_id, entry_source, entry.statements),
        (module_id, module_source, module.statements),
        (unrelated_id, unrelated_source, unrelated.statements),
    ] {
        let mut standalone_builder = ArenaProgramBuilder::with_token_capacity(128);
        let standalone = Parser::parse_source_into_arena_builder(
            source_id,
            source,
            &mut standalone_builder,
        );
        assert!(standalone.diagnostics.is_empty(), "{:?}", standalone.diagnostics);
        let standalone_program = standalone_builder.finish_with_statements(standalone.statements);
        let shared_diagnostics = grouping_diagnostics_in_range(&shared, shared_statements, source);
        assert!(
            !shared_diagnostics.is_empty(),
            "expected grouping diagnostics for source {source_id:?}"
        );
        assert_eq!(
            shared_diagnostics,
            grouping_diagnostics_in_range(&standalone_program, standalone.statements, source),
            "grouping diagnostics changed for source {source_id:?}"
        );
    }
}

#[test]
fn wire_enum_and_cli_validation_use_the_selected_root() {
    let mut sources = SourceMap::new();
    let alpha_source = "enum AlphaState: Str { Ready = \"ready\" }\ncli main() [] {}\n";
    let beta_source = "enum BetaState: Str { Broken = unresolved }\ncli main() [] {}\nproc main() [] {}\n";
    let alpha_id = sources.add_file("alpha-entry.xsh", alpha_source);
    let beta_id = sources.add_file("beta-entry.xsh", beta_source);

    let mut builder = ArenaProgramBuilder::with_token_capacity(64);
    let alpha = Parser::parse_source_into_arena_builder(alpha_id, alpha_source, &mut builder);
    let beta = Parser::parse_source_into_arena_builder(beta_id, beta_source, &mut builder);
    assert!(alpha.diagnostics.is_empty(), "{:?}", alpha.diagnostics);
    assert!(beta.diagnostics.is_empty(), "{:?}", beta.diagnostics);

    let program = builder.finish_with_statements(alpha.statements);
    let type_program = Arc::new(program.clone());
    let alpha_view = CheckBundleView::for_root(&program, alpha.statements, None, Some(alpha_id));
    let alpha_check = Checker::check_bundle_view_with_type_program(
        &alpha_view,
        alpha_source,
        CheckOptions::default(),
        type_program.clone(),
    );
    assert!(
        alpha_check
            .diagnostics
            .iter()
            .all(|diagnostic| !matches!(
                diagnostic.code,
                Some(DiagnosticCode::CheckEnumWireMapping | DiagnosticCode::CheckCliEntry)
            )),
        "{:?}",
        alpha_check.diagnostics
    );
    assert!(alpha_check
        .checked_declarations
        .wire_enums
        .mappings
        .keys()
        .any(|name| name.as_str() == "AlphaState"));
    assert!(!alpha_check
        .checked_declarations
        .wire_enums
        .mappings
        .keys()
        .any(|name| name.as_str() == "BetaState"));

    let beta_view = CheckBundleView::for_root(&program, beta.statements, None, Some(beta_id));
    let beta_check = Checker::check_bundle_view_with_type_program(
        &beta_view,
        beta_source,
        CheckOptions::default(),
        type_program,
    );
    assert!(beta_check
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEnumWireMapping)));
    assert!(beta_check
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckCliEntry)));
    assert!(!beta_check
        .checked_declarations
        .wire_enums
        .mappings
        .keys()
        .any(|name| name.as_str() == "AlphaState"));
}

#[test]
fn imported_module_bodies_publish_generic_type_instantiations() {
    let mut sources = SourceMap::new();
    let entry_source = "##! Entry.\nuse model\nproc main() { let root_values = [\"root\"] }\n";
    let module_source = "##! Generic models.\n## Generic box.\nexport type Box[T] = {value: T}\n## Build one box.\nexport pure make() -> Box[Int] { let module_values = [\"module\"]; Box(value: 7) }\n";
    let entry_id = sources.add_file("entry.xsh", entry_source);
    let module_id = sources.add_file("model.xsh", module_source);
    let mut builder = ArenaProgramBuilder::with_token_capacity(64);
    let entry = Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    let model_name = builder.symbol_owner().intern("model");
    builder.push_arena_module("model".to_string(), model_name, module.statements);
    for statement in builder.statement_ids(entry.statements) {
        if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
            builder.set_use_resolved(use_id, Arc::from("model"));
        }
    }

    let program = Arc::new(builder.finish_with_statements(entry.statements));
    let session = CheckSession::new(program, Arc::new(sources));
    let has_box_instantiation = |output: &CheckOutput| {
        output
            .resolved_type_expressions
            .instantiations
            .iter()
            .any(|(span, instance)| {
                span.source_id == module_id
                    && module_source.get(span.start()..span.end()) == Some("Box(value: 7)")
                    && instance.expressions.values().any(|ty| ty == &Type::Int)
            })
    };
    let module_precedes_entry_bindings = |output: &CheckOutput| {
        let binding_source = |fact: &xsh::frontend::check::AnnotationFact| match &fact.kind {
            AnnotationFactKind::Binding { span, .. } => Some(span.source_id),
            _ => None,
        };
        let module_binding = output
            .annotation_facts
            .iter()
            .position(|fact| binding_source(fact) == Some(module_id));
        let entry_binding = output
            .annotation_facts
            .iter()
            .position(|fact| binding_source(fact) == Some(entry_id));
        module_binding.zip(entry_binding).is_some_and(|(module, entry)| module < entry)
    };

    let first = session.check(entry.statements, entry_id, CheckOptions::default());
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(has_box_instantiation(&first));
    assert!(module_precedes_entry_bindings(&first));
    let cached = session.check(
        entry.statements,
        entry_id,
        CheckOptions {
            reveal_types: true,
            ..CheckOptions::default()
        },
    );
    assert!(cached.diagnostics.is_empty(), "{:?}", cached.diagnostics);
    assert!(has_box_instantiation(&cached));
    assert!(module_precedes_entry_bindings(&cached));
}

#[test]
fn imported_wire_mapping_diagnostics_replay_once_from_session_records() {
    use xsh::frontend::load::{
        entry_source_from_text, parse_load_entry_source_arena_only, project_module_roots,
    };

    let root = tempfile::tempdir().expect("create imported enum fixture");
    let entry_path = root.path().join("entry.xsh");
    let module_path = root.path().join("helper.xsh");
    let entry_source = "use helper\nenum EntryState: Str { Broken = unresolved_entry }\n";
    let module_source = "##! Invalid imported enum.\n## The bad value must remain attached to this owner.\nexport enum BrokenState: Str { Broken = unresolved }\n";
    std::fs::write(&entry_path, entry_source).expect("write entry source");
    std::fs::write(&module_path, module_source).expect("write module source");

    let entry_file = entry_path.to_str().expect("fixture path is UTF-8");
    let entry = entry_source_from_text(entry_file, entry_source.to_owned());
    let entry_source_id = entry.source_id;
    let (sources, parsed) = parse_load_entry_source_arena_only(
        entry_file,
        entry,
        project_module_roots(&entry_path).expect("resolve module roots"),
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let module_source_id = sources
        .files()
        .iter()
        .find(|source| source.name() == module_path.to_str().unwrap())
        .expect("imported module was loaded")
        .id();

    let root_statements = parsed.arena.statements;
    let program = Arc::new(parsed.arena);
    let sources = Arc::new(sources);
    let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
    let view = session.view_for_root(root_statements, entry_source_id);
    let outputs = [
        session.check_view(&view, CheckOptions::default()),
        session.check_view(
            &view,
            CheckOptions {
                reveal_types: true,
                ..CheckOptions::default()
            },
        ),
    ];

    for output in &outputs {
        let diagnostics = output
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEnumWireMapping))
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 2, "{:?}", output.diagnostics);
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.labels[0].span.source_id)
                .collect::<Vec<_>>(),
            [entry_source_id, module_source_id],
        );
    }
    assert_eq!(outputs[0].diagnostics, outputs[1].diagnostics);
}
