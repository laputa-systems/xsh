use crate::sema::check::{Checker, NominalMemberKind, QualifiedNominalIdentity};
use crate::sema::types::Type;
use crate::source::SourceId;
use crate::syntax::parser::Parser;

#[test]
fn checked_nominal_member_unused_declarations_survive_frontend_drop() {
    let source = "enum ReceiptChoice { Selected(Int, Str), Other(Int, Str), Empty }\nerror ReceiptFailure = Missing(message: Str, code: Int) : NotFound | Broken(message: Str, code: Int) : InvalidData\npure selected(value: ReceiptChoice) -> Int { match value { Selected(number, _) => number\n _ => 0 } }\npure message(value: ReceiptFailure) -> Str { match value { ReceiptFailure.Missing {message} => message\n _ => \"\" } }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(91), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(parsed);
    let solved = &checked.solved;
    let _symbols = solved.symbol_owner().enter();
    solved.validate().unwrap();
    let members = solved.nominal_members.iter().filter(|(identity, _)| matches!(identity, QualifiedNominalIdentity::Source { source, .. } if *source == SourceId::new(91))).collect::<Vec<_>>();
    assert_eq!(members.len(), 5, "unused and pattern-only members have independent declaration receipts");
    assert!(solved.constructor_nominals.is_empty(), "no constructor application supplied this authority");
    for (&identity, member) in members {
        assert!(std::ptr::eq(member.as_ref(), solved.checked_nominal_member(identity).unwrap()));
        if member.member == "Selected" {
            assert_eq!(member.kind, NominalMemberKind::Tag);
            assert_eq!(member.fields.iter().map(|(label, ty)| (*label, solved.graph.export_type(*ty).unwrap())).collect::<Vec<_>>(), [(None, Type::Int), (None, Type::Str)]);
        }
        if member.member == "Missing" {
            assert_eq!(member.kind, NominalMemberKind::Error);
            assert_eq!(member.fields.iter().map(|(label, ty)| (label.unwrap().as_str().to_string(), solved.graph.export_type(*ty).unwrap())).collect::<Vec<_>>(), [("message".to_string(), Type::Str), ("code".to_string(), Type::Int)]);
        }
    }
}

fn checked_members(source_id: SourceId, source: &str) -> crate::sema::check::CheckOutput {
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(parsed);
    checked.solved.validate().unwrap();
    checked
}

#[test]
fn checked_nominal_member_cold_receipts_reject_member_owner_and_payload_replacement() {
    use crate::sema::check::{Name, NominalDeclaration};
    use std::sync::Arc;
    let checked = checked_members(SourceId::new(92), "enum OwnerChoice { Selected(Int, Str), Other(Int, Str) }\nerror OwnerFailure = Missing(message: Str, code: Int) | Broken(message: Str, code: Int)\n");
    let mut solved = Arc::try_unwrap(checked.solved).unwrap();
    let _symbols = solved.symbol_owner().enter();
    let (&identity, record) = solved.nominal_members.iter().find(|(identity, member)| matches!(identity, QualifiedNominalIdentity::Source { .. }) && member.member == "Selected").unwrap();
    let original = Arc::clone(record);
    let before = solved.graph.counters().clone();
    assert!(std::ptr::eq(record.as_ref(), solved.checked_nominal_member(identity).unwrap()));
    assert_eq!(solved.graph.counters(), &before, "cold receipt projection does not check or solve source again");
    let QualifiedNominalIdentity::Source { source, namespace, declaration, .. } = identity else { unreachable!() };
    let other_member = QualifiedNominalIdentity::Source { source, namespace, declaration, member: Some(Name::intern("Other")) };
    solved.nominal_members.insert(identity, Arc::clone(&solved.nominal_members[&other_member]));
    assert!(solved.checked_nominal_member(identity).is_err(), "a same-typed sibling is a different registered member");
    assert!(solved.validate().is_err());
    solved.nominal_members.insert(identity, Arc::clone(&original));
    let foreign_source = QualifiedNominalIdentity::Source { source: SourceId::new(93), namespace, declaration, member: Some(original.member) };
    let moved = solved.nominal_members.remove(&identity).unwrap();
    solved.nominal_members.insert(foreign_source, moved);
    assert!(solved.checked_nominal_member(foreign_source).is_err());
    assert!(solved.validate().is_err(), "equal numeric declaration indices cannot move between source owners");
    solved.nominal_members.remove(&foreign_source);
    solved.nominal_members.insert(identity, Arc::clone(&original));
    let wrong_kind = QualifiedNominalIdentity::Source { source, namespace, declaration: NominalDeclaration::Error(crate::syntax::arena::ErrorDefId::from_index(0)), member: Some(original.member) };
    solved.nominal_members.insert(wrong_kind, Arc::clone(&original));
    assert!(solved.checked_nominal_member(wrong_kind).is_err());
    assert!(solved.validate().is_err());
    solved.nominal_members.remove(&wrong_kind);
    Arc::make_mut(solved.nominal_members.get_mut(&identity).unwrap()).fields.swap(0, 1);
    assert!(solved.checked_nominal_member(identity).is_err(), "field order is part of the original payload contract");
    assert!(solved.validate().is_err());
    solved.nominal_members.insert(identity, original);
    solved.validate().unwrap();
}

#[test]
fn checked_nominal_member_retains_registered_builtins_without_source_declarations() {
    let checked = checked_members(SourceId::new(94), "let selected = 1\n");
    let solved = &checked.solved;
    let _symbols = solved.symbol_owner().enter();
    let builtins = solved.nominal_members.iter().filter(|(identity, _)| matches!(identity, QualifiedNominalIdentity::Builtin { .. })).collect::<Vec<_>>();
    assert!(!builtins.is_empty());
    for (&identity, member) in builtins {
        let QualifiedNominalIdentity::Builtin { family, member: selected } = identity else { unreachable!() };
        assert_eq!(family, member.family);
        assert_eq!(selected, Some(member.member));
        assert_eq!(member.kind, NominalMemberKind::Error);
        assert_eq!(solved.graph.export_type(member.tested).unwrap(), Type::ErrorVariant { family, variant: selected.unwrap() });
        solved.checked_nominal_member(identity).unwrap();
    }
}

#[test]
fn checked_nominal_member_namespace_and_import_alias_keep_original_declaration_owner() {
    use crate::sema::check::{Name, NominalDeclaration, SolvedPatternDecision};
    use crate::syntax::arena::ArenaProgramBuilder;
    let source = "use model as first\nuse model as second\npure selected(value: first.Choice) -> Int { match value { first.Selected(number) => number\n _ => 0 } }\npure message(value: second.Failure) -> Str { match value { second.Failure.Missing {message} => message\n _ => \"\" } }\n";
    let module_source = "##! Member ownership.\nenum PrivateChoice { PrivateSelected(Str), PrivateUnused(Int) }\nerror PrivateFailure = Unused(message: Str)\n## An exported choice.\nexport enum Choice { Selected(Int), Other(Int) }\n## An exported failure.\nexport error Failure = Missing(message: Str) | Unused(message: Str)\n";
    let mut builder = ArenaProgramBuilder::with_token_capacity(256);
    let entry = Parser::parse_source_into_arena_builder(SourceId::new(95), source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(SourceId::new(96), module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    let namespace = builder.symbol_owner().with_current(|| Name::intern("member-model"));
    for statement in builder.statement_ids(entry.statements) {
        if let Some((import, _, _)) = builder.use_stmt_for_statement(statement) {
            builder.set_use_resolved(import, std::sync::Arc::from("member-model"));
        }
    }
    builder.push_arena_module("member-model".to_string(), namespace, module.statements);
    let program = builder.finish_with_statements(entry.statements);
    let checked = Checker::check_arena(&program, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(program);
    checked.solved.validate().unwrap();
    let solved = &checked.solved;
    let _symbols = solved.symbol_owner().enter();
    let members = solved.nominal_members.iter().filter(|(identity, _)| matches!(identity, QualifiedNominalIdentity::Source { source, .. } if *source == SourceId::new(96))).collect::<Vec<_>>();
    assert_eq!(members.len(), 7, "private and unused registered members survive without constructor applications");
    for (&identity, member) in members {
        assert!(matches!(identity, QualifiedNominalIdentity::Source { namespace: Some(owner), .. } if owner == namespace));
        assert!(member.family.as_str().starts_with("member-model."));
        solved.checked_nominal_member(identity).unwrap();
    }
    let error = solved.patterns.values().find(|plan| matches!(plan.decision, SolvedPatternDecision::ErrorVariant { .. })).unwrap();
    let SolvedPatternDecision::ErrorVariant { family, variant, identity, .. } = error.decision else { unreachable!() };
    assert!(matches!(identity, QualifiedNominalIdentity::Source { source, namespace: Some(owner), declaration: NominalDeclaration::Error(_), .. } if source == SourceId::new(96) && owner == namespace));
    let member = solved.checked_nominal_member(identity).unwrap();
    assert_eq!((family, variant), (member.family, member.member));
    assert_eq!(solved.graph.export_type(member.tested).unwrap(), Type::ErrorVariant { family, variant });
    assert_eq!(solved.graph.export_type(error.tested.unwrap()).unwrap(), Type::ErrorVariant { family: Name::intern("second.Failure"), variant: Name::intern("Missing") }, "the original alias type test is retained separately from the declaration member");
}

#[test]
fn checked_nominal_member_same_spelled_families_remain_distinct_between_modules() {
    use crate::sema::check::Name;
    use crate::syntax::arena::ArenaProgramBuilder;
    let source = "use left_model as left\nuse right_model as right\n";
    let module_source = "##! Same spelled declarations.\nenum Choice { Selected(Int) }\nerror Failure = Missing(message: Str)\n";
    let mut builder = ArenaProgramBuilder::with_token_capacity(128);
    let entry = Parser::parse_source_into_arena_builder(SourceId::new(97), source, &mut builder);
    for (source_id, key) in [(98, "left_model"), (99, "right_model")] {
        let module = Parser::parse_source_into_arena_builder(SourceId::new(source_id), module_source, &mut builder);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let namespace = builder.symbol_owner().with_current(|| Name::intern(key));
        builder.push_arena_module(key.to_string(), namespace, module.statements);
    }
    for (statement, key) in builder.statement_ids(entry.statements).into_iter().zip(["left_model", "right_model"]) {
        if let Some((import, _, _)) = builder.use_stmt_for_statement(statement) {
            builder.set_use_resolved(import, std::sync::Arc::from(key));
        }
    }
    let program = builder.finish_with_statements(entry.statements);
    let checked = Checker::check_arena(&program, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(program);
    checked.solved.validate().unwrap();
    let solved = &checked.solved;
    let _symbols = solved.symbol_owner().enter();
    let members = solved.nominal_members.iter().filter(|(identity, _)| matches!(identity, QualifiedNominalIdentity::Source { .. })).collect::<Vec<_>>();
    assert_eq!(members.len(), 4);
    let errors = members.iter().filter(|(_, member)| member.kind == NominalMemberKind::Error).collect::<Vec<_>>();
    assert_ne!(errors[0].0, errors[1].0);
    assert_eq!(errors[0].1.member, errors[1].1.member);
    assert_ne!(errors[0].1.family, errors[1].1.family);
    assert_ne!(errors[0].1.tested, errors[1].1.tested);
}

#[test]
fn checked_nominal_member_shared_payload_is_accounted_and_foreign_roots_cannot_replace_it() {
    use std::sync::Arc;
    let source = "enum SharedChoice { Selected(Int, Str) }\n";
    let checked = checked_members(SourceId::new(100), source);
    let foreign = checked_members(SourceId::new(100), source);
    let mut solved = Arc::try_unwrap(checked.solved).unwrap();
    let (&identity, record) = solved.nominal_members.iter().find(|(identity, _)| matches!(identity, QualifiedNominalIdentity::Source { .. })).unwrap();
    let original = Arc::clone(record);
    let before_bytes = solved.retained_source_bytes();
    let fields = &mut Arc::make_mut(solved.nominal_members.get_mut(&identity).unwrap()).fields;
    fields.reserve(16);
    let copied_capacity = fields.capacity() * std::mem::size_of::<(Option<crate::symbol::Name>, crate::sema::inference::TypeId)>();
    assert!(solved.retained_source_bytes() >= before_bytes + copied_capacity, "public and private payloads are counted once each when mutation separates their Arcs");
    solved.checked_nominal_member(identity).unwrap();
    solved.validate().unwrap();
    let foreign_member = foreign.solved.nominal_members.iter().find(|(identity, _)| matches!(identity, QualifiedNominalIdentity::Source { .. })).unwrap().1;
    let local = Arc::make_mut(solved.nominal_members.get_mut(&identity).unwrap());
    local.tested = foreign_member.tested;
    local.fields = foreign_member.fields.clone();
    assert!(solved.checked_nominal_member(identity).is_err());
    assert!(solved.validate().is_err(), "equal source/declaration indices cannot launder another graph's roots");
    solved.nominal_members.insert(identity, original);
    solved.validate().unwrap();
}

#[test]
fn checked_nominal_member_source_ledger_obeys_the_shared_node_budget() {
    use crate::sema::check::{Name, NominalDeclaration};
    use crate::sema::inference::{InferenceContext, Limits};
    let parsed = Parser::parse_source_arena_only(SourceId::new(101), "enum BudgetChoice { Selected(Int) }\n");
    parsed.arena.symbol_owner().with_current(|| {
        let mut checker = Checker::new(Default::default());
        let graph = InferenceContext::new(Limits { type_row_nodes: 3, ..Limits::default() });
        let owner = graph.owner();
        {
            let mut state = checker.generic.borrow_mut();
            state.facts = Default::default();
            state.facts.graph = graph;
            state.facts.owner = owner;
            state.facts.producer_flows = crate::sema::check::ProducerFlowGraph::new(owner);
        }
        let identity = QualifiedNominalIdentity::Source { source: SourceId::new(101), namespace: None, declaration: NominalDeclaration::Type(crate::syntax::arena::TypeDefId::from_index(0)), member: Some(Name::intern("Selected")) };
        checker.record_checked_nominal_member(identity, NominalMemberKind::Tag, Name::intern("BudgetChoice"), Name::intern("Selected"), &[(None, Type::Int)], &[], parsed.arena.arena.stmt(parsed.arena.statement_ids().next().unwrap()).span);
        assert_eq!(checker.generic.borrow().facts.graph.counters().attempted_nodes, 4);
        assert!(!checker.diagnostics.is_empty());
        assert!(checker.generic.borrow().facts.nominal_members.is_empty(), "a failed shared-node charge cannot publish a partial declaration receipt");
    });
}

#[test]
fn checked_nominal_member_invalid_fields_do_not_publish_successful_payload_authority() {
    let source = "enum RecoveryChoice { Selected(UnknownField), Empty }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(102), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(!checked.diagnostics.is_empty());
    assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.inference-boundary")), "unavailable member metadata cannot replace the original annotation diagnostic");
    drop(parsed);
    checked.solved.validate().unwrap();
    let _symbols = checked.solved.symbol_owner().enter();
    assert!(!checked.solved.nominal_members.iter().any(|(identity, member)| matches!(identity, QualifiedNominalIdentity::Source { .. }) && member.member == "Selected"));
}

#[test]
fn checked_nominal_member_imported_error_input_preserves_original_family_owner() {
    use crate::sema::check::{Name, SolvedPatternDecision};
    use crate::syntax::arena::ArenaProgramBuilder;
    let source = "use failures as model\npure message(value: model.Failure) -> Str { match value { model.Failure.Missing {message} => message\n _ => \"\" } }\n";
    let module_source = "##! Original error owner.\n## A failure family.\nexport error Failure = Missing(message: Str) | Other(message: Str)\n";
    let mut builder = ArenaProgramBuilder::with_token_capacity(128);
    let entry = Parser::parse_source_into_arena_builder(SourceId::new(103), source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(SourceId::new(104), module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    let namespace = builder.symbol_owner().with_current(|| Name::intern("error-owner"));
    for statement in builder.statement_ids(entry.statements) {
        if let Some((import, _, _)) = builder.use_stmt_for_statement(statement) {
            builder.set_use_resolved(import, std::sync::Arc::from("error-owner"));
        }
    }
    builder.push_arena_module("error-owner".to_string(), namespace, module.statements);
    let program = builder.finish_with_statements(entry.statements);
    let checked = Checker::check_arena(&program, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(program);
    checked.solved.validate().unwrap();
    let solved = &checked.solved;
    let _symbols = solved.symbol_owner().enter();
    let plan = solved.patterns.values().find(|plan| matches!(plan.decision, SolvedPatternDecision::ErrorVariant { .. })).unwrap();
    let SolvedPatternDecision::ErrorVariant { identity, .. } = plan.decision else { unreachable!() };
    let QualifiedNominalIdentity::Source { source, namespace, declaration, .. } = identity else { unreachable!() };
    assert_eq!(solved.graph.export_type(plan.input).unwrap(), Type::ErrorFamily(Name::intern("model.Failure")));
    assert_eq!(solved.nominals.get(&plan.input), Some(&QualifiedNominalIdentity::Source { source, namespace, declaration, member: None }), "surface input retains the original registered family owner");
    assert_eq!(solved.nominals.get(&plan.tested.unwrap()), Some(&identity), "surface tested member retains the original registered member owner");
}

#[test]
fn checked_nominal_member_pattern_owner_ports_reject_same_graph_substitution_and_deletion() {
    use crate::sema::check::{PatternTypePosition, SolvedPatternDecision};
    let checked = checked_members(SourceId::new(105), "error PortFailure = Missing(message: Str) | Other(message: Str)\nerror PortOther = Missing(message: Str)\npure message(value: PortFailure) -> Str { match value { PortFailure.Missing {message} => message\n _ => \"\" } }\n");
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    let _symbols = solved.symbol_owner().enter();
    let (&pattern, plan) = solved.patterns.iter().find(|(_, plan)| matches!(plan.decision, SolvedPatternDecision::ErrorVariant { .. })).unwrap();
    let (input, tested) = (plan.input, plan.tested.unwrap());
    let original_input = solved.nominals[&input];
    let original_tested = solved.nominals[&tested];
    let before = solved.graph.counters().clone();
    assert_eq!(solved.checked_pattern_nominal(pattern, PatternTypePosition::Input).unwrap(), Some(original_input));
    assert_eq!(solved.checked_pattern_nominal(pattern, PatternTypePosition::Tested).unwrap(), Some(original_tested));
    assert!(solved.checked_pattern_scope(pattern).unwrap().is_some());
    assert_eq!(solved.graph.counters(), &before);
    solved.nominals.remove(&input);
    assert!(solved.checked_pattern_nominal(pattern, PatternTypePosition::Input).is_err(), "deleting original owner metadata cannot erase its source proof");
    assert!(solved.validate().is_err());
    solved.nominals.insert(input, original_input);
    let (&other, _) = solved.nominal_members.iter().find(|(identity, member)| matches!(identity, QualifiedNominalIdentity::Source { .. }) && member.family == "PortOther").unwrap();
    let QualifiedNominalIdentity::Source { source, namespace, declaration, .. } = other else { unreachable!() };
    let other_family = QualifiedNominalIdentity::Source { source, namespace, declaration, member: None };
    solved.nominals.insert(input, other_family);
    assert!(solved.checked_pattern_nominal(pattern, PatternTypePosition::Input).is_err(), "another valid family in the same graph cannot replace the checked input owner");
    assert!(solved.validate().is_err());
    solved.nominals.insert(input, original_input);
    solved.nominals.insert(tested, other);
    assert!(solved.checked_pattern_nominal(pattern, PatternTypePosition::Tested).is_err());
    assert!(solved.validate().is_err());
    solved.nominals.insert(tested, original_tested);
    solved.validate().unwrap();
}
