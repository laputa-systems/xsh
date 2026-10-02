use crate::sema::check::{Checker, PatternIdentity, SolvedPatternDecision, SolvedPatternShape};
use crate::sema::types::Type;
use crate::source::SourceId;
use crate::syntax::arena::{ArenaPatternKind, PatternId};
use crate::syntax::parser::Parser;

fn checked_patterns(source_id: SourceId, source: &str) -> crate::sema::check::CheckOutput {
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(parsed);
    checked.solved.validate().unwrap();
    checked
}

#[test]
fn checked_pattern_result_capture_preserves_inferred_return_and_shadowing() {
    let source = "pure selected(outcome: Result[Int]) { if let Ok(selected) = outcome { selected + 1 } else { 0 } }\n";
    let checked = checked_patterns(SourceId::new(86), source);
    assert!(checked.function_return_types.values().all(|ty| ty == &Type::Int));
}

#[test]
fn checked_pattern_test_retains_original_false_arm_after_frontend_drop() {
    let source = "enum Event { Added(Str) }\npure selected(event: Event) -> Bool { event is Added(_) }\n";
    for checker_source in [source, ""] {
        let source_id = SourceId::new(89);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let arms = (0..parsed.arena.arena.expr_tags.len()).find_map(|index| {
            let expression = crate::syntax::arena::ExprId::from_index(index);
            match parsed.arena.arena.expr(expression).kind {
                crate::syntax::arena::ArenaExprKind::PatternTest { arms, .. } => Some(arms),
                _ => None,
            }
        }).unwrap();
        let patterns = parsed.arena.arena.match_expr_arms(arms).iter().map(|arm| {
            PatternIdentity { source: source_id, namespace: None, pattern: arm.pattern }
        }).collect::<Vec<_>>();
        assert_eq!(patterns.len(), 2);
        assert!(matches!(parsed.arena.arena.pattern(patterns[1].pattern).kind, ArenaPatternKind::Wildcard));
        let checked = Checker::check_arena(&parsed.arena, checker_source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        checked.solved.validate().unwrap();
        let tested = checked.solved.checked_pattern(patterns[0]).unwrap();
        let fallback = checked.solved.checked_pattern(patterns[1]).expect("the original false arm must retain its checked pattern authority");
        assert!(matches!(tested.decision, SolvedPatternDecision::TagConstructor { .. }));
        assert!(matches!(fallback.shape, SolvedPatternShape::Wildcard));
        assert_eq!(fallback.input, tested.input);
        assert_eq!(fallback.caller, tested.caller);
        assert!(fallback.children.is_empty());
        assert!(fallback.captures.is_empty());
    }
}

#[test]
fn checked_pattern_principal_subject_preserves_original_value_scope_after_frontend_drop() {
    let source = "error PatternFailure = Missing(message: Str)\nlet selected = match retry [] on (PatternFailure) { Err(PatternFailure.Missing(message: \"missing\"))? } { Err(failure) => 1\n Ok(absent) => 0 }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(87), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let (&subject, &scope) = checked.solved.expression_schemes.iter().find(|(identity, _)| {
        matches!(parsed.arena.arena.expr(identity.expression).kind, crate::syntax::arena::ArenaExprKind::Retry { .. })
    }).unwrap();
    drop(parsed);
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    solved.validate().unwrap();
    assert_eq!(subject.source, SourceId::new(87));
    assert!(!solved.graph.scheme(scope).unwrap().quantifiers.is_empty());
    assert_eq!(solved.pattern_value_scopes.len(), solved.patterns.len());
    for (&identity, plan) in &solved.patterns {
        assert_eq!(solved.pattern_value_scopes.get(&identity), Some(&scope));
        assert_eq!(solved.checked_pattern_scope(identity).unwrap(), Some(scope));
        assert!(std::ptr::eq(plan.as_ref(), solved.checked_pattern(identity).unwrap()));
    }
    let identity = *solved.pattern_value_scopes.keys().next().unwrap();
    solved.pattern_value_scopes.remove(&identity);
    assert!(solved.checked_pattern(identity).is_err(), "deleting the public scope cannot erase its original source authority");
    assert!(solved.checked_pattern_scope(identity).is_err());
    assert!(solved.validate().is_err());
    solved.pattern_value_scopes.insert(identity, scope);
    let other = *solved.expression_schemes.values().find(|other| **other != scope).unwrap();
    solved.pattern_value_scopes.insert(identity, other);
    assert!(solved.checked_pattern(identity).is_err(), "another valid scheme in the same graph cannot replace the original subject scope");
    assert!(solved.checked_pattern_scope(identity).is_err());
    assert!(solved.validate().is_err());
    solved.pattern_value_scopes.insert(identity, scope);
    solved.validate().unwrap();
}

#[test]
fn checked_pattern_aliases_and_reordered_alternatives_survive_frontend_drop() {
    let source = include_str!("../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh");
    let source_id = SourceId::new(71);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let identities = parsed.arena.arena.patterns.iter().enumerate().map(|(index, _)| {
        PatternIdentity { source: source_id, namespace: None, pattern: PatternId::from_index(index) }
    }).collect::<Vec<_>>();
    let alias = identities.iter().copied().find(|identity| {
        matches!(parsed.arena.arena.pattern(identity.pattern).kind, ArenaPatternKind::Alias { .. })
    }).unwrap();
    let alternatives = identities.iter().copied().filter(|identity| {
        matches!(parsed.arena.arena.pattern(identity.pattern).kind, ArenaPatternKind::Alternation(_))
    }).collect::<Vec<_>>();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(parsed);
    let _symbols = checked.solved.symbol_owner().enter();
    assert_eq!(checked.solved.patterns.len(), identities.len(), "every checked original pattern owns its facts");
    let alias_plan = &checked.solved.patterns[&alias];
    assert_eq!(alias_plan.captures.len(), 1, "the alias does not copy descendant captures");
    assert_eq!(alias_plan.captures[0].identity.name.as_str(), "original");
    assert_eq!(checked.solved.graph.export_type(alias_plan.captures[0].ty).unwrap(), Type::List(Box::new(Type::List(Box::new(Type::Int)))));
    for identity in alternatives {
        let plan = &checked.solved.patterns[&identity];
        assert!(matches!(plan.decision, SolvedPatternDecision::Alternation));
        assert_eq!(plan.children.len(), 2);
        assert_eq!(plan.captures.len(), 2);
        for capture in &plan.captures {
            assert_eq!(capture.identity.pattern, identity);
            assert_eq!(capture.branches.len(), 2);
            assert_ne!(capture.branches[0], capture.branches[1]);
            for branch in &capture.branches {
                let origin = &checked.solved.patterns[&branch.pattern];
                let original = origin.captures.iter().find(|original| original.identity == *branch).unwrap();
                assert_eq!(original.identity.name, capture.identity.name);
                assert_eq!(checked.solved.graph.export_type(original.ty).unwrap(), checked.solved.graph.export_type(capture.ty).unwrap());
            }
        }
    }
    checked.solved.validate().unwrap();
}

#[test]
fn checked_pattern_join_rejects_capture_forged_from_another_alternative_child() {
    let source = "pure nested(values: List[Int]) -> Int {\n match values {\n ([left, right] | [left, right]) | [right, left] => left * 10 + right\n _ => 0\n }\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(72), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(parsed);
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    solved.validate().unwrap();
    let outer = solved.patterns.iter().find_map(|(&identity, plan)| {
        (matches!(plan.decision, SolvedPatternDecision::Alternation)
            && plan.children.iter().any(|child| solved.patterns[child].children.iter().any(|child| matches!(solved.patterns[child].decision, SolvedPatternDecision::Alternation))))
            .then_some(identity)
    }).unwrap();
    let original = solved.patterns[&outer].captures[0].branches[0];
    let original_branches = solved.patterns[&outer].captures[0].branches.clone();
    let forged = solved.patterns[&original.pattern].captures.iter().find(|capture| capture.identity == original).unwrap().branches[0];
    assert_ne!(forged, original);
    std::sync::Arc::make_mut(solved.patterns.get_mut(&outer).unwrap()).captures[0].branches[1] = forged;
    assert!(solved.validate().is_err(), "same-name and same-type capture in the first child cannot authorize the second child");
    std::sync::Arc::make_mut(solved.patterns.get_mut(&outer).unwrap()).captures[0].branches = original_branches.clone();
    std::sync::Arc::make_mut(solved.patterns.get_mut(&outer).unwrap()).captures[0].branches.swap(0, 1);
    assert!(solved.validate().is_err(), "contributing captures retain textual child order");
    std::sync::Arc::make_mut(solved.patterns.get_mut(&outer).unwrap()).captures[0].branches = original_branches;
    std::sync::Arc::make_mut(solved.patterns.get_mut(&outer).unwrap()).captures[0].branches.pop();
    assert!(solved.validate().is_err(), "every alternative child contributes its checked capture");
}

#[test]
fn checked_pattern_decisions_retain_checked_payloads_and_narrowed_captures() {
    let source = r#"
enum PatternChoice { PatternEmpty, PatternPayload(Int, Str) }
error PatternFailure = Missing(message: Str) : NotFound | Broken(code: Int) : InvalidData
pure tag(value: PatternChoice) -> Int {
 match value {
  PatternPayload(number, text) => number
  PatternEmpty => 0
 }
}
pure error_payload(value: PatternFailure) -> Str {
 match value {
  PatternFailure.Missing {message} => message
  _ => ""
 }
}
pure select_type(value: Any) -> Int {
 match value {
  number is Int => number
  _ => 0
 }
}
pure select_result(value: Result[Int, PatternFailure]) -> Int {
 match value {
  Ok(number) => number
  Err(_) => 0
 }
}
pure facet(value: PatternFailure) -> Bool { value is NotFound }
"#;
    let checked = checked_patterns(SourceId::new(73), source);
    let solved = &checked.solved;
    let _symbols = solved.symbol_owner().enter();
    let export = |ty| solved.graph.export_type(ty).unwrap();
    let tuple = solved.patterns.values().find(|plan| matches!(plan.decision, SolvedPatternDecision::TagFields { .. })).unwrap();
    assert!(matches!(export(tuple.input), Type::Tag(_)), "tag field topology retains its real tagged subject");
    let SolvedPatternDecision::TagFields { fields } = &tuple.decision else { unreachable!() };
    assert_eq!(fields.iter().map(|&ty| export(ty)).collect::<Vec<_>>(), [Type::Int, Type::Str]);
    assert_eq!(tuple.children.iter().map(|child| export(solved.patterns[child].input)).collect::<Vec<_>>(), [Type::Int, Type::Str]);
    let error = solved.patterns.values().find(|plan| matches!(plan.decision, SolvedPatternDecision::ErrorVariant { ref fields, .. } if !fields.is_empty())).unwrap();
    assert!(matches!(export(error.tested.unwrap()), Type::ErrorVariant { .. }));
    let SolvedPatternDecision::ErrorVariant { fields, .. } = &error.decision else { unreachable!() };
    assert_eq!(fields[0].0.as_str(), "message");
    assert_eq!(export(fields[0].1), Type::Str);
    let narrowed = solved.patterns.values().find(|plan| matches!(plan.decision, SolvedPatternDecision::Type) && !plan.captures.is_empty()).unwrap();
    assert_eq!(export(narrowed.input), Type::Any);
    assert_eq!(export(narrowed.tested.unwrap()), Type::Int);
    assert_eq!(export(narrowed.captures[0].ty), Type::Int);
    assert!(solved.patterns.values().any(|plan| matches!(plan.decision, SolvedPatternDecision::Result { success: true, payload: Some(ty) } if export(ty) == Type::Int)));
    assert!(solved.patterns.values().any(|plan| matches!(plan.decision, SolvedPatternDecision::Facet { .. })));
}

#[test]
fn checked_pattern_aliases_compare_resolved_types_without_copying_child_captures() {
    let checked = checked_patterns(SourceId::new(74), r#"
type AliasText = Str
type AliasOtherText = Str
pure aliases(value: Any) -> Str {
 match value {
  (text is AliasText | text is AliasOtherText) as original => text
  _ => ""
 }
}
"#);
    let _symbols = checked.solved.symbol_owner().enter();
    let alias = checked.solved.patterns.values().find(|plan| plan.captures.iter().any(|capture| capture.identity.name == "original")).unwrap();
    assert_eq!(alias.captures.len(), 1);
    assert_eq!(checked.solved.graph.export_type(alias.captures[0].ty).unwrap(), Type::Any);
    let alternative = checked.solved.patterns.values().find(|plan| matches!(plan.decision, SolvedPatternDecision::Alternation)).unwrap();
    assert_eq!(alternative.captures.len(), 1);
    assert_eq!(checked.solved.graph.export_type(alternative.captures[0].ty).unwrap(), Type::Str);
    assert_eq!(alternative.captures[0].branches.len(), 2);
}

#[test]
fn checked_pattern_equal_source_ids_do_not_authorize_foreign_graph_types() {
    let source = "pure select(value: Int) -> Int { match value { selected => selected } }";
    let first = checked_patterns(SourceId::new(75), source);
    let second = checked_patterns(SourceId::new(75), source);
    let mut solved = std::sync::Arc::try_unwrap(first.solved).unwrap();
    let (&identity, plan) = solved.patterns.iter().next().unwrap();
    let original = plan.input;
    std::sync::Arc::make_mut(solved.patterns.get_mut(&identity).unwrap()).input = second.solved.patterns[&identity].input;
    assert!(solved.validate().is_err(), "equal numeric source and pattern IDs do not own foreign type handles");
    std::sync::Arc::make_mut(solved.patterns.get_mut(&identity).unwrap()).input = original;
    solved.validate().unwrap();
    std::sync::Arc::make_mut(solved.patterns.get_mut(&identity).unwrap()).captures[0].identity.pattern.source = SourceId::new(76);
    assert!(solved.validate().is_err(), "capture source identity remains exact");
}

#[test]
fn checked_pattern_unavailable_dynamic_children_preserve_source_acceptance() {
    let checked = checked_patterns(SourceId::new(77), "pure unknown_field(value: Any) -> Int { match value { {field: captured} => 0\n _ => 0 } }");
    assert!(checked.solved.patterns.values().all(|plan| plan.captures.is_empty()));
    assert_eq!(checked.solved.patterns.len(), 1, "the wildcard is complete; the erased record and unknown child remain unavailable");
}

#[test]
fn checked_pattern_imported_namespace_owns_original_patterns_and_nominal_types() {
    use crate::sema::check::{Name, QualifiedNominalIdentity};
    use crate::syntax::arena::ArenaProgramBuilder;
    let source = "use model as m\npure local(value: Int) -> Int { match value { selected => selected } }\n";
    let module_source = "##! Pattern ownership module.\nenum ModelChoice { ModelEmpty, ModelValue(Int) }\n## Select the declared payload.\nexport pure select(value: ModelChoice) -> Int { match value { ModelValue(selected) => selected\n ModelEmpty => 0 } }\n";
    let mut builder = ArenaProgramBuilder::with_token_capacity(128);
    let entry = Parser::parse_source_into_arena_builder(SourceId::new(78), source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(SourceId::new(79), module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    let namespace = builder.symbol_owner().with_current(|| Name::intern("pattern-model"));
    for statement in builder.statement_ids(entry.statements) {
        if let Some((import, _, _)) = builder.use_stmt_for_statement(statement) {
            builder.set_use_resolved(import, std::sync::Arc::from("pattern-model"));
        }
    }
    builder.push_arena_module("pattern-model".to_string(), namespace, module.statements);
    let program = builder.finish_with_statements(entry.statements);
    let checked = Checker::check_arena(&program, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(program);
    checked.solved.validate().unwrap();
    let imported = checked.solved.patterns.iter().filter(|(identity, _)| identity.source == SourceId::new(79)).collect::<Vec<_>>();
    assert_eq!(imported.len(), 3);
    for (identity, plan) in &imported {
        assert_eq!(identity.namespace, Some(namespace));
        assert_eq!(plan.caller.unwrap().namespace, Some(namespace));
    }
    let (&identity, tag) = imported.into_iter().find(|(_, plan)| matches!(plan.decision, SolvedPatternDecision::TagConstructor { .. })).unwrap();
    assert!(matches!(checked.solved.nominals.get(&tag.tested.unwrap()), Some(QualifiedNominalIdentity::Source { source, namespace: Some(owner), .. }) if *source == SourceId::new(79) && *owner == namespace));
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    let plan = solved.patterns.remove(&identity).unwrap();
    let foreign_namespace = PatternIdentity { namespace: None, ..identity };
    solved.patterns.insert(foreign_namespace, plan);
    assert!(solved.validate().is_err(), "the original module pattern cannot be republished into the entry namespace");
}

#[test]
fn checked_pattern_invalid_alternative_does_not_publish_successful_join_authority() {
    let source = "pure invalid(values: List[Any]) -> Int { match values { [left, right] | [left is Int, right] => 0\n _ => 0 } }";
    let parsed = Parser::parse_source_arena_only(SourceId::new(80), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.pattern-alternative-binding")), "{:?}", checked.diagnostics);
    assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.inference-boundary")), "publication must preserve the original source diagnostic");
    assert!(!checked.solved.patterns.values().any(|plan| matches!(plan.decision, SolvedPatternDecision::Alternation)));
    drop(parsed);
    checked.solved.validate().unwrap();
}

#[test]
fn checked_pattern_generic_capture_stays_owned_by_its_declaration_scheme() {
    let checked = checked_patterns(SourceId::new(81), "pure retain(value) { match value { captured => captured } }\nlet integer = retain(1)\nlet text = retain(\"word\")\n");
    let solved = &checked.solved;
    let plan = solved.patterns.values().find(|plan| !plan.captures.is_empty()).unwrap();
    assert_eq!(plan.captures.len(), 1);
    assert_eq!(plan.captures[0].ty, plan.input);
    let declaration = &solved.declarations[&plan.caller.unwrap()];
    let identity = solved.patterns.iter().find(|(_, original)| std::ptr::eq(original.as_ref(), plan.as_ref())).unwrap().0;
    assert_eq!(solved.checked_pattern_scope(*identity).unwrap(), Some(declaration.scheme));
    let input = solved.graph.resolved(plan.input).unwrap();
    assert!(matches!(solved.graph.node(input).unwrap(), crate::sema::inference::TypeNode::Rigid { .. }));
    assert!(solved.graph.scheme_type_binders(declaration.scheme).unwrap().contains(&input));
    assert_eq!(solved.calls.len(), 2);
}

#[test]
fn checked_pattern_original_shapes_survive_frontend_drop_and_refuse_cold_replacement() {
    let source = "type PatternFields = {left: Int, right: Int}\npure selected(values: List[Int], fields: PatternFields) -> Int {\n let from_list = match values { [first, ..rest] as original => first + rest.len() + original.len()\n _ => 0 }\n match fields { {right: captured, left: _} => captured + from_list\n _ => 0 }\n}\n";
    let checked = checked_patterns(SourceId::new(82), source);
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    let _symbols = solved.symbol_owner().enter();
    let (&list, plan) = solved.patterns.iter().find(|(_, plan)| matches!(plan.shape, SolvedPatternShape::List { .. })).unwrap();
    assert!(matches!(plan.shape, SolvedPatternShape::List { elements: 1, has_rest: true }));
    assert_eq!(plan.children.len(), 2);
    assert_eq!(solved.graph.export_type(solved.patterns[&plan.children[0]].input).unwrap(), Type::Int);
    assert_eq!(solved.graph.export_type(solved.patterns[&plan.children[1]].input).unwrap(), Type::List(Box::new(Type::Int)));
    let original = std::sync::Arc::clone(&solved.patterns[&list]);
    std::sync::Arc::make_mut(solved.patterns.get_mut(&list).unwrap()).shape = SolvedPatternShape::List { elements: 2, has_rest: false };
    assert!(solved.checked_pattern(list).is_err());
    assert!(solved.validate().is_err());
    solved.patterns.insert(list, original);
    let (&record, plan) = solved.patterns.iter().find(|(_, plan)| matches!(plan.shape, SolvedPatternShape::Record { .. })).unwrap();
    let SolvedPatternShape::Record { fields } = &plan.shape else { unreachable!() };
    assert_eq!(fields.iter().map(|name| name.as_str().to_string()).collect::<Vec<_>>(), ["right", "left"]);
    let original = std::sync::Arc::clone(&solved.patterns[&record]);
    let SolvedPatternShape::Record { fields } = &mut std::sync::Arc::make_mut(solved.patterns.get_mut(&record).unwrap()).shape else { unreachable!() };
    fields.swap(0, 1);
    assert!(solved.checked_pattern(record).is_err());
    assert!(solved.validate().is_err());
    solved.patterns.insert(record, original);
    let (&alias, _) = solved.patterns.iter().find(|(_, plan)| matches!(plan.shape, SolvedPatternShape::Alias { .. })).unwrap();
    let original = std::sync::Arc::clone(&solved.patterns[&alias]);
    std::sync::Arc::make_mut(solved.patterns.get_mut(&alias).unwrap()).captures.clear();
    assert!(solved.checked_pattern(alias).is_err());
    assert!(solved.validate().is_err());
    solved.patterns.insert(alias, original);
    solved.validate().unwrap();
    solved.patterns.remove(&alias);
    assert!(solved.validate().is_err(), "deleting a public fact cannot delete its retained source authority");
}

#[test]
fn checked_pattern_tag_members_keep_original_definition_owner_through_module_aliases() {
    use crate::sema::check::{Name, NominalDeclaration, QualifiedNominalIdentity};
    use crate::syntax::arena::ArenaProgramBuilder;
    let source = "use model as first\nuse model as second\npure selected(value: first.Choice) -> Int { match value { second.Payload(number) => number\n second.Other(number) => number\n _ => 0 } }\n";
    let module_source = "##! Constructor ownership.\n## A tagged choice.\nexport enum Choice { Payload(Int), Other(Int) }\n";
    let mut builder = ArenaProgramBuilder::with_token_capacity(128);
    let entry = Parser::parse_source_into_arena_builder(SourceId::new(83), source, &mut builder);
    let module = Parser::parse_source_into_arena_builder(SourceId::new(84), module_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    let namespace = builder.symbol_owner().with_current(|| Name::intern("pattern-constructor-owner"));
    for statement in builder.statement_ids(entry.statements) {
        if let Some((import, _, _)) = builder.use_stmt_for_statement(statement) {
            builder.set_use_resolved(import, std::sync::Arc::from("pattern-constructor-owner"));
        }
    }
    builder.push_arena_module("pattern-constructor-owner".to_string(), namespace, module.statements);
    let program = builder.finish_with_statements(entry.statements);
    let checked = Checker::check_arena(&program, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(program);
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    let _symbols = solved.symbol_owner().enter();
    solved.validate().unwrap();
    let (&pattern, plan) = solved.patterns.iter().find(|(_, plan)| matches!(plan.decision, SolvedPatternDecision::TagConstructor { constructor, .. } if constructor == "Payload")).unwrap();
    let SolvedPatternDecision::TagConstructor { constructor, identity, fields, .. } = &plan.decision else { unreachable!() };
    assert_eq!(constructor.as_str(), "Payload");
    assert!(matches!(identity, QualifiedNominalIdentity::Source { source, namespace: Some(owner), declaration: NominalDeclaration::Type(_), member: Some(member) } if *source == SourceId::new(84) && *owner == namespace && *member == *constructor));
    assert_eq!(fields.iter().map(|&ty| solved.graph.export_type(ty).unwrap()).collect::<Vec<_>>(), [Type::Int]);
    let original = std::sync::Arc::clone(&solved.patterns[&pattern]);
    let SolvedPatternDecision::TagConstructor { constructor, .. } = &mut std::sync::Arc::make_mut(solved.patterns.get_mut(&pattern).unwrap()).decision else { unreachable!() };
    *constructor = Name::intern("Other");
    assert!(solved.checked_pattern(pattern).is_err(), "a different member cannot reuse the original source receipt");
    assert!(solved.validate().is_err());
    solved.patterns.insert(pattern, original);
    solved.validate().unwrap();
}

#[test]
fn checked_pattern_literal_keeps_prepared_scalar_without_ast_or_reanalysis() {
    let source = include_str!("../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh");
    let checked = checked_patterns(SourceId::new(85), source);
    let literals = checked.solved.patterns.values().filter_map(|plan| {
        if let SolvedPatternShape::Literal { expression, value } = &plan.shape { Some((expression, value)) } else { None }
    }).collect::<Vec<_>>();
    assert_eq!(literals.len(), 2);
    for (expression, value) in literals {
        assert_eq!(expression.source, SourceId::new(85));
        assert_eq!(*value, Some(crate::sema::constants::LiteralConstant::Int(99)));
    }
}

#[test]
fn checked_pattern_source_receipt_is_shared_readonly_and_payload_copies_are_accounted() {
    let checked = checked_patterns(SourceId::new(86), "type Fields = {left: Int, right: Int}\npure selected(value: Fields) -> Int { match value { {right: number, left: _} => number\n _ => 0 } }\n");
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    let (&identity, current) = solved.patterns.iter().find(|(_, plan)| matches!(plan.shape, SolvedPatternShape::Record { .. })).unwrap();
    let before_counters = solved.graph.counters().clone();
    assert!(std::ptr::eq(current.as_ref(), solved.checked_pattern(identity).unwrap()), "public view and private source ledger share one payload");
    assert_eq!(solved.graph.counters(), &before_counters, "receipt lookup does not solve or regenerate source facts");
    let before_bytes = solved.retained_source_bytes();
    let SolvedPatternShape::Record { fields } = &mut std::sync::Arc::make_mut(solved.patterns.get_mut(&identity).unwrap()).shape else { unreachable!() };
    fields.reserve(16);
    let added_capacity = fields.capacity() * std::mem::size_of::<crate::symbol::Name>();
    assert!(solved.retained_source_bytes() >= before_bytes + added_capacity, "the new public payload is counted while the original shared authority remains retained");
    solved.checked_pattern(identity).unwrap();
    solved.validate().unwrap();
}

#[test]
fn checked_pattern_original_source_ledger_uses_the_shared_node_budget() {
    use crate::sema::inference::{InferenceContext, Limits};
    let source = "pure selected(value: Int) -> Int { match value { _ => 0 } }";
    let parsed = Parser::parse_source_arena_only(SourceId::new(87), source);
    assert!(parsed.diagnostics.is_empty());
    let pattern = parsed.arena.arena.patterns.iter().enumerate().find_map(|(index, pattern)| matches!(pattern.kind, ArenaPatternKind::Wildcard).then_some(PatternId::from_index(index))).unwrap();
    parsed.arena.symbol_owner().with_current(|| {
        let mut checker = Checker::new(Default::default());
        let graph = InferenceContext::new(Limits { type_row_nodes: 2, ..Limits::default() });
        let owner = graph.owner();
        {
            let mut state = checker.generic.borrow_mut();
            state.facts.graph = graph;
            state.facts.owner = owner;
            state.facts.producer_flows = crate::sema::check::ProducerFlowGraph::new(owner);
        }
        checker.check_pattern_arena(&parsed.arena, source, pattern, &Type::Int);
        assert!(!checker.diagnostics.is_empty(), "type atom and public pattern exhaust the two-node cap before the original ledger entry");
        assert!(checker.generic.borrow().facts.patterns.is_empty(), "failed source authority must not publish a partial public plan");
        assert_eq!(checker.generic.borrow().facts.graph.counters().attempted_nodes, 3);
    });
}
