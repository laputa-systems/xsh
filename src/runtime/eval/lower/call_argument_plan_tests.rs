use super::*;
use crate::sema::check::Checker;
use crate::source::SourceId;
use crate::syntax::parser::Parser;

#[test]
fn original_record_destructure_leaves_retain_checked_binding_storage() {
    crate::runtime::eval::run_eval(|| {
        let source = "let {count, nested: {label: title, ..}, ..} = {count: 4, nested: {label: \"word\"}}\nvar {enabled} = {enabled: true}\npure selected() -> Str { title }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            fn leaves(arena: &crate::syntax::arena::ArenaProgram, target: BindingTargetId, result: &mut Vec<(BindingTargetId, Name)>) {
                match arena.arena.binding_target(target).kind {
                    ArenaBindingTargetKind::Name(name) => result.push((target, name)),
                    ArenaBindingTargetKind::Record { fields, .. } => for field in arena.arena.destructure_fields(fields) { leaves(arena, field.target, result); },
                }
            }
            let mut targets = Vec::new();
            for statement in parsed.arena.statement_ids() {
                if let ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } = parsed.arena.arena.stmt(statement).kind {
                    leaves(&parsed.arena, target, &mut targets);
                }
            }
            assert_eq!(targets.len(), 3);
            for (target, name) in targets {
                let identity = crate::sema::check::BindingIdentity { source: SourceId::new(0), namespace: None, target };
                let binding = bodies.solved.bindings.get(&identity).unwrap_or_else(|| panic!("original destructure leaf {name} has no checked binding root"));
                let view = storage::checked_storage_view(&bodies.solved.graph, Some(crate::sema::inference::ScopedRoot { ty: binding.ty, scope: binding.scheme })).unwrap();
                let expected = match name.as_str().as_ref() { "count" => LoweredType::Int, "title" => LoweredType::Str, "enabled" => LoweredType::Bool, _ => panic!() };
                assert_eq!(view.kind, expected);
                assert_eq!(binding.mutable, name == "enabled");
                assert_eq!(binding.owner, None);
            }
            let known = compact_top_level_known(&parsed.arena, &declarations, &bodies, source, None, None, None).unwrap();
            assert_eq!(known.len(), 4);
            let lowered = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
            assert_eq!(lowered.blocker_events, 0);
        });
        drop(parsed);
        bodies.solved.validate().unwrap();
    });
}

#[test]
fn missing_original_binding_storage_refuses_the_first_statement_and_function_prefix() {
    crate::runtime::eval::run_eval(|| {
        let source = "let first = 4\nlet second = 7\npure selected() -> Int { first }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut declarations = Checker::check_compact_declarations(&parsed.arena);
        let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let statements = parsed.arena.statement_ids().collect::<Vec<_>>();
        let ArenaStmtKind::PureDef(function) = parsed.arena.arena.stmt(statements[2]).kind else { panic!() };
        parsed.arena.symbol_owner().with_current(|| {
            assert!(compact_top_level_known(&parsed.arena, &declarations, &bodies, source, None, None, None).is_ok());
            declarations.solved = Default::default();
            let solved = Arc::get_mut(&mut bodies.solved).unwrap();
            for statement in &statements[..2] {
                let ArenaStmtKind::Let { target, .. } = parsed.arena.arena.stmt(*statement).kind else { panic!() };
                solved.bindings.remove(&crate::sema::check::BindingIdentity { source: SourceId::new(0), namespace: None, target });
            }
            let complete = compact_top_level_known(&parsed.arena, &declarations, &bodies, source, None, None, None);
            assert_eq!(complete.unwrap_err(), statements[0]);
            let prefix = compact_function_top_level_known(&parsed.arena, &declarations, &bodies, source, None, None, function, None);
            assert_eq!(prefix.unwrap_err(), statements[0]);
            let refused = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
            assert_eq!(refused.constructed_functions, 0);
            assert_eq!(refused.blocker_events, 1);
            assert_eq!(refused.top_level_blocker_sample_spans["binding_type"], [parsed.arena.arena.stmt(statements[0]).span]);
            let names = FxHashSet::default();
            let qualified = FxHashSet::default();
            let functions = LowerableFunctions::all(&names, &names, &qualified, &qualified);
            let (program, refused) = lower_compact_top_level_program_with_probe(&parsed.arena, &declarations, &bodies,
                source, &SourceMap::new(), &functions);
            assert_eq!(program.statements.len(), statements.len());
            assert!(program.statements.iter().all(Option::is_none));
            assert_eq!(refused.blocker_events, 1);
            assert_eq!(refused.top_level_blocker_sample_spans["binding_type"], [parsed.arena.arena.stmt(statements[0]).span]);
        });
    });
}

#[test]
fn original_generic_function_parameters_and_results_keep_container_storage() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure copied(values) { [@values] }\npure wrap(value) { [value] }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("function-storage.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let mut units = Vec::new();
        parsed.arena.symbol_owner().with_current(|| {
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| { units.push(unit); Ok(()) }).unwrap();
            let first = units.iter().find(|unit| unit.key == LoweredFunctionKey::Name(Name::intern("copied"))).unwrap();
            let first = first.body.as_ref().unwrap_or_else(|| panic!("{:?}", first.blocker_detail));
            let callable = &bodies.solved.declarations[&first.solved_declaration.unwrap()];
            let signature = bodies.solved.graph.resolved(callable.signature).unwrap();
            let crate::sema::inference::TypeNode::Arrow(arrow) = bodies.solved.graph.node(signature).unwrap() else { panic!() };
            let parameter = bodies.solved.graph.resolved(arrow.params[0].ty).unwrap();
            assert!(matches!(bodies.solved.graph.node(parameter).unwrap(), crate::sema::inference::TypeNode::List(_)));
            assert_eq!(first.param_kinds[0], LoweredType::List);
            assert!(matches!(first.return_kind, LoweredReturnKind::Plain(LoweredType::List)));
            let wrap = units.iter().find(|unit| unit.key == LoweredFunctionKey::Name(Name::intern("wrap"))).unwrap();
            let wrap = wrap.body.as_ref().unwrap_or_else(|| panic!("{:?}", wrap.blocker_detail));
            assert_eq!(wrap.param_kinds[0], LoweredType::Generic);
            assert!(matches!(wrap.return_kind, LoweredReturnKind::Plain(LoweredType::List)));
            for body in [first, wrap] {
                let declaration = body.solved_declaration.unwrap();
                let callable = &bodies.solved.declarations[&declaration];
                let signature = bodies.solved.graph.resolved(callable.signature).unwrap();
                let crate::sema::inference::TypeNode::Arrow(arrow) = bodies.solved.graph.node(signature).unwrap() else { panic!() };
                for ty in [arrow.params[0].ty, arrow.result] {
                    bodies.solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope: Some(callable.scheme) }).unwrap();
                }
            }
        });
        drop(parsed);
        bodies.solved.validate().unwrap();
    });
}

fn assert_callable_execution_after_frontend_drop(
    source: &str, expected: &[u8], inspect: impl Fn(&crate::syntax::arena::ArenaProgram, &crate::sema::check::CheckOutput) + Sync,
) {
    crate::runtime::eval::run_eval(|| {
        for recursive in [false, true] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("checked-callable-value.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone();
            symbols.with_current(|| {
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.is_empty(), "semantic phase: {:?}", checked.diagnostics);
                checked.solved.validate().unwrap();
                inspect(&parsed.arena, &checked);
                let solved = Arc::downgrade(&checked.solved);
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked)
                    .unwrap_or_else(|diagnostic| panic!("preparation phase: {diagnostic:?}"));
                drop(checked);
                drop(parsed);
                assert!(solved.upgrade().is_none(), "the prepared callable cannot retain its inference bundle");
                let execute = || {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                        .unwrap_or_else(|_| panic!("the prepared indexed callable remains installed"))
                };
                let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) }
                    else { execute() };
                assert_eq!(output.status, 0, "execution phase, recursive={recursive}: {:?}; {:?}", output.diagnostics, output.traceback);
                assert_eq!(output.stdout, expected, "recursive={recursive}");
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                assert!(output.traceback.is_none(), "{:?}", output.traceback);
                assert!(output.stderr.is_empty(), "{:?}", output.stderr);
            });
        }
    });
}

#[test]
fn ground_user_callable_alias_executes_after_source_and_inference_disposal() {
    let source = "pure increment(value: Int) -> Int { value + 1 }\nlet alias = increment\nprint ${alias(41)}\n";
    assert_callable_execution_after_frontend_drop(source, b"42\n", |arena, checked| {
        let (identity, call) = checked.solved.calls.iter().find(|(identity, _)| {
            &source[arena.arena.expr(identity.expression).span.range()] == "alias(41)"
        }).unwrap();
        assert_eq!(call.binding.supplied_slots, [0]);
        let crate::syntax::arena::ArenaExprKind::Call { callee, .. } = arena.arena.expr(identity.expression).kind else { panic!() };
        let mut callee_identity = *identity;
        callee_identity.expression = callee;
        assert!(checked.solved.expression_callables[&callee_identity].declaration.is_some());
    });
}

#[test]
fn generic_callable_parameter_executes_with_its_original_user_authority_after_disposal() {
    let source = "pure increment(value: Int) -> Int { value + 1 }\npure apply(callback, value) { callback(value) }\nprint ${apply(increment, 41)}\n";
    assert_callable_execution_after_frontend_drop(source, b"42\n", |arena, checked| {
        let (identity, call) = checked.solved.calls.iter().find(|(identity, _)| {
            &source[arena.arena.expr(identity.expression).span.range()] == "apply(increment, 41)"
        }).unwrap();
        assert_eq!(call.binding.supplied_slots, [0, 1]);
        assert!(!call.requirements.is_empty());
        let crate::sema::arguments::ArgumentValueSource::Expression(callback) = checked.solved.argument_sources[identity][0].value else { panic!() };
        let mut callback_identity = *identity;
        callback_identity.expression = callback;
        assert!(checked.solved.expression_callables[&callback_identity].declaration.is_some());
        assert!(call.requirements.iter().any(|requirement| matches!(checked.solved.graph.requirement_template(*requirement),
            Ok(crate::sema::inference::RequirementTemplate::CallableInvocation { .. }))));
    });
}

#[test]
fn native_callable_alias_executes_with_its_selected_original_receipt_after_disposal() {
    let source = "let encode = json.encode\nprint ${encode(42)?}\n";
    assert_callable_execution_after_frontend_drop(source, b"42\n", |arena, checked| {
        let invocation = checked.solved.invocations.iter().find(|(identity, _)| {
            &source[arena.arena.expr(identity.expression).span.range()] == "encode(42)"
        }).unwrap().1;
        let evidence = checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().unwrap();
        assert_eq!(evidence.native_alternatives.len(), 1);
        let (_, binding, _) = evidence.unique_plan().unwrap();
        assert_eq!(binding.supplied_slots, [0]);
        assert_eq!(binding.default_slots, [1]);
        let operation = evidence.native_alternatives[0].operation;
        assert!(checked.solved.graph.candidate_evidence(operation).unwrap().is_some());
        assert_eq!(checked.solved.registry_references.len(), 1);
    });
}

#[test]
fn direct_module_contract_calls_retain_original_binding_recipes_after_syntax_drop() {
    let source = "type Plugin = module { export pure total(first: Int, second: Int = 20, third: Int = 30) -> Int }\nproc inspect(plugin: Plugin) [] -> Int { plugin.total(...{first: 1, third: 3}) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(96), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let identity = *checked.solved.expressions.keys().find(|identity| {
        &source[parsed.arena.arena.expr(identity.expression).span.range()] == "plugin.total(...{first: 1, third: 3})"
    }).unwrap();
    drop(parsed);
    checked.solved.validate().unwrap();
    let recipes = &checked.solved.argument_sources[&identity];
    assert_eq!(recipes.len(), 2);
    assert!(recipes.iter().all(|argument| argument.entry_index == 0));
    let call = &checked.solved.calls[&identity];
    assert_eq!(call.binding.supplied_slots, [0, 2]);
    assert_eq!(call.binding.default_slots, [1]);
    assert_eq!(call.declaration, None);
}

#[test]
fn imported_module_callable_promises_keep_pure_empty_and_proc_unknown_effects() {
    let definitions = "type Plugin = module { export pure value() -> Int; export proc clock() -> Int }\n";
    let source = format!("{definitions}pure inspect(plugin: Plugin) -> Int {{ plugin.value() }}\nproc unrestricted(plugin: Plugin) -> Int {{ plugin.clock() }}\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(96), &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(parsed);
    checked.solved.validate().unwrap();
    let contracts = checked.solved.calls.values().map(|call| {
        let graph = &checked.solved.graph;
        let crate::sema::inference::TypeNode::Arrow(arrow) = graph.node(graph.resolved(call.signature).unwrap()).unwrap() else { panic!() };
        (arrow.kind, graph.resolved_effect_summary(arrow.effects).unwrap())
    }).collect::<Vec<_>>();
    assert_eq!(contracts.len(), 2);
    assert!(contracts.contains(&(crate::sema::inference::CallableKind::Pure, crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY))));
    assert!(contracts.contains(&(crate::sema::inference::CallableKind::Proc, crate::sema::inference::EffectSummary::Unknown)));
    let source = format!("{definitions}proc restricted(plugin: Plugin) [] -> Int {{ plugin.clock() }}\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(96), &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let rejected = Checker::check_arena(&parsed.arena, &source);
    assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", rejected.diagnostics);
}

#[test]
fn typed_callable_call_method_uses_the_original_checked_argument_binding() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure combine(first: Int = 4, second: Int = 2) -> Int { first + second }\nlet alias = combine\nlet result = alias.call(second: 7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let identity = *bodies.solved.calls.keys().find(|identity| {
            &source[parsed.arena.arena.expr(identity.expression).span.range()] == "alias.call(second: 7)"
        }).unwrap();
        assert_eq!(bodies.solved.calls[&identity].binding.supplied_slots, [1]);
        assert_eq!(bodies.solved.calls[&identity].binding.default_slots, [0]);
        assert_eq!(bodies.solved.argument_sources[&identity].len(), 1);
        parsed.arena.symbol_owner().with_current(|| {
            let lowered = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
            assert_eq!(lowered.blocker_events, 0, "typed .call consumes its checked source recipe");
        });
        drop(parsed);
        bodies.solved.validate().unwrap();
    });
}

#[test]
fn user_call_lowering_requires_the_original_checked_binding_and_sources() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure combine(left: Int = 1, right: Int = 2) -> Int { left + right }\nlet result = combine(right: 9)\n";
        for remove_sources in [false, true] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut declarations = Checker::check_compact_declarations(&parsed.arena);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let valid = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert_eq!(valid.blocker_events, 0);
                declarations.solved = Default::default();
                let solved = Arc::get_mut(&mut bodies.solved).unwrap();
                if remove_sources { solved.argument_sources.clear(); }
                else { solved.calls.clear(); }
                let absent = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert!(absent.blocker_events > 0, "missing checked argument evidence must refuse lowering");
            });
        }
    });
}

#[test]
fn checked_user_argument_recipes_execute_after_source_arena_disposal() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        for recursive in [false, true] {
            let source = r#"type Third = {third: Int}
const lexical = 4
pure total(left: Int = lexical, right: Int = 2, third: Int = 3) -> Int { left + right + third }
proc marked(value: Int) [io] -> Int { print $value; value }
proc options() [io] -> Third { print spread; {third: 9} }
proc caller() [io] -> Int {
    let lexical = 100
    let _ = lexical
    total(right: marked(7), ...options())
}
print ${caller()}
print ${total(third: marked(9), right: marked(7), left: marked(4))}
"#;
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("checked-argument-recipe.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
            let plan = parsed.arena.symbol_owner().with_current(|| {
                evaluator.prepare_compact_indexed_only_or_diagnostic(&parsed.arena, source_id, false)
            }).expect("retained arguments must build a verified indexed program");
            let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
            drop(parsed);
            let output = crate::runtime::eval::run_eval(move || symbols.with_current(|| {
                let execute = || {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                };
                if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) }
                else { execute() }
            })).unwrap_or_else(|_| panic!("retained arguments execute without source syntax"));
            assert_eq!(output.status, 0);
            assert!(output.traceback.is_none(), "{:?}", output.traceback);
            assert_eq!(output.stdout, b"7\nspread\n20\n9\n7\n4\n20\n");
            assert!(output.stderr.is_empty(), "{:?}", output.stderr);
        }
    }).unwrap().join().unwrap();
}

#[test]
fn original_named_callback_recipes_keep_the_declaration_owned_residual_invocation() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure combine(first: Str, second: Str) -> Str { first + second }\npure reversed(callback, left, right) { callback(second: right, first: left) }\nproc ordered() [] -> Str { reversed(combine, \"left-\", \"right\") }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("original-residual-named-callback.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
        let (&call, recipes) = checked.solved.argument_sources.iter().find(|(_, recipes)|
            recipes.first().is_some_and(|source| source.name == Some(Name::intern("second")))).unwrap();
        assert_eq!(recipes.iter().map(|source| source.name).collect::<Vec<_>>(),
            [Some(Name::intern("second")), Some(Name::intern("first"))]);
        let invocation = &checked.solved.invocations[&call];
        assert!(checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().is_none());
        let caller = invocation.caller.unwrap();
        assert_eq!(checked.solved.expression_owners[&call], caller);
        let scheme = checked.solved.declarations[&caller].scheme;
        assert!(checked.solved.graph.scheme(scheme).unwrap().requirement_origins.contains(&invocation.requirement));
        let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let mut functions = Vec::new();
        lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
            StdlibLowerLinkage::Local, |unit| {
                assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                functions.push((unit.key, unit.body.unwrap())); Ok(())
            }).unwrap();
        drop(parsed);
        checked.solved.validate().unwrap();
        let function = &functions.iter().find(|(key, _)| *key == LoweredFunctionKey::Name(Name::intern("reversed"))).unwrap().1;
        let scratch = function.scratch.borrow();
        let mut arguments = scratch.argument_binding_origins.values().filter(|source| source.call == call).collect::<Vec<_>>();
        arguments.sort_by_key(|source| source.ordinal);
        assert_eq!(arguments.len(), 2);
        for (ordinal, argument) in arguments.iter().enumerate() {
            assert_eq!(argument.recipe, checked.solved.argument_sources[&call][ordinal]);
            assert!(argument.wrapper.is_some());
        }
    });
}

#[test]
fn original_named_spread_lowering_uses_retained_recipes_without_legacy_expansion_type_hints() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure sum(left: Int, right: Int) -> Int { left + right }\nlet fields = {right: 4}\nlet result = sum(left: 3, ...fields)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
        let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
        let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let (&call, recipe) = bodies.solved.argument_sources.iter().find(|(_, recipe)| recipe.iter().any(|source|
            matches!(source.value, crate::sema::arguments::ArgumentValueSource::RecordField { .. }))).unwrap();
        assert!(bodies.solved.calls.contains_key(&call));
        assert_eq!(recipe.len(), 2);
        let crate::sema::arguments::ArgumentValueSource::RecordField { record, field } = recipe[1].value else { panic!(); };
        assert_eq!(field, "right");
        assert_eq!(recipe[1].entry_index, 1);
        let retained = bodies.solved.expressions[&ExpressionIdentity { expression: record, ..call }];
        assert!(matches!(bodies.solved.graph.node(bodies.solved.graph.resolved(retained).unwrap()).unwrap(), crate::sema::inference::TypeNode::Record(_)));
        let baseline = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
        assert_eq!(baseline.blocker_events, 0);
        // The obsolete tree mirror must not expand a checked call again. Its
        // original graph endpoints, field recipe, and destination plan remain
        // unchanged when this untrusted lowering hint is erased.
        bodies.expr_types.insert(record, Type::ErasedRecord);
        bodies.solved.validate().unwrap();
        let lowered = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
        assert_eq!(lowered.blocker_events, 0, "an original checked field recipe does not depend on the legacy expansion facade");
        drop(parsed);
        bodies.solved.validate().unwrap();
    });
}

#[test]
fn original_finite_named_spread_entries_preserve_order_defaults_and_disposal_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        let source = r#"type Third = {third: Int}
type Pair = {left: Int, right: Int}
const lexical = 4
pure total(left: Int = lexical, right: Int = 2, third: Int = 3) -> Int { left + right + third }
proc marked(label: Str, value: Int) [io] -> Int { print $label; value }
proc third() [io] -> Third { print spread; {third: 9} }
proc pair() [io] -> Pair { print pair; {left: 1, right: 2} }
proc caller() [io] -> Int {
 let lexical = 100
 let _ = lexical
 total(right: marked("right", 7), ...third())
}
print ${caller()}
print ${total(third: marked("third", 9), ...pair())}
"#;
        for force_recursive in [false, true] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-finite-spread-routes.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone();
            symbols.with_current(|| {
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.is_empty(), "semantic phase: {:?}", checked.diagnostics);
                let spread_calls = checked.solved.argument_sources.iter().filter(|(_, recipe)| recipe.iter().any(|source|
                    matches!(source.value, crate::sema::arguments::ArgumentValueSource::RecordField { .. }))).collect::<Vec<_>>();
                assert_eq!(spread_calls.len(), 2);
                let (call, recipe) = spread_calls.iter().find(|(_, recipe)| recipe.len() == 3).unwrap();
                assert_eq!(recipe.iter().map(|source| source.entry_index).collect::<Vec<_>>(), [0, 1, 1]);
                assert_eq!(checked.solved.calls[*call].binding.supplied_slots, [2, 0, 1]);
                let (default_call, _) = spread_calls.iter().find(|(_, recipe)| recipe.len() == 2).unwrap();
                assert_eq!(checked.solved.calls[*default_call].binding.default_slots, [0]);
                let solved = Arc::downgrade(&checked.solved);
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked)
                    .unwrap_or_else(|diagnostic| panic!("preparation phase: {diagnostic:?}"));
                drop(checked);
                drop(parsed);
                assert!(solved.upgrade().is_none(), "saved original argument recipes do not retain the inference bundle");
                let execute = || {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), force_recursive);
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                        .unwrap_or_else(|_| panic!("the original spread program remains installed"))
                };
                let output = if force_recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
                assert_eq!(output.status, 0, "recursive={force_recursive}: {:?}", output.diagnostics);
                assert_eq!(output.stdout, b"right\nspread\n20\nthird\npair\n12\n", "each supplied entry runs once in authored order before declaration defaults, recursive={force_recursive}");
                assert!(output.stderr.is_empty(), "{:?}", output.stderr);
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                assert!(output.traceback.is_none(), "{:?}", output.traceback);
            });
        }
    });
}
