#[test]
fn generic_named_stage_invocation_uses_checked_evidence_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let generic = "pure identity(value) { value }\npure observed() -> List[Int] { [1, 2] |> map(identity) }\nlet result = observed()\nprint f\"${result[0]}:${result[1]}\"\n";
        let monomorphic = "pure identity(value: Int) -> Int { value }\npure observed() -> List[Int] { [1, 2] |> map(identity) }\nlet result = observed()\nprint f\"${result[0]}:${result[1]}\"\n";
        let parsed = Parser::parse_source_arena_only(crate::source::SourceId::new(0), generic);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, generic);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert!(checked.solved.graph.counters().instantiations > 0);
        assert_eq!(checked.solved.stage_operations.len(), 1);
        let stage = checked.solved.stage_operations.values().next().unwrap();
        let Some(crate::sema::check::StageCallback::Callable { requirement, declaration: Some(declaration), .. }) = stage.callback else { panic!("named stage retains its original declaration and invocation") };
        let callback = &checked.solved.declarations[&declaration];
        assert_eq!(checked.solved.graph.scheme(callback.scheme).unwrap().quantifiers.len(), 1);
        let invocation = checked.solved.graph.invocation_evidence(requirement).unwrap().expect("concrete stage item discharges its original invocation");
        let (_, binding, timing) = invocation.unique_plan().expect("identity has one original signature");
        assert_eq!(binding.supplied_slots, vec![0]);
        assert!(binding.default_slots.is_empty());
        assert_eq!(timing, crate::sema::inference::InvocationDefaultTiming::AtCall);
        drop(checked);
        drop(parsed);
        crate::runtime::eval::run_eval(|| {
        for force_recursive in [false, true] {
            let control = run_program_through_route_inspecting(monomorphic, force_recursive, |program| {
                assert!(program.function_view(LoweredFunctionKey::Name(Name::intern("identity")), LoweredFunctionKind::Pure).unwrap().is_some());
            });
            assert_eq!(control, (0, b"1:2\n".to_vec(), Vec::new()));
            let actual = run_program_through_route_inspecting(generic, force_recursive, |program| {
                let evidence = program.generic_evidence().expect("quantified callback retains a prepared scope");
                let callback = program.function_view(LoweredFunctionKey::Name(Name::intern("identity")), LoweredFunctionKind::Pure).unwrap().unwrap();
                assert_eq!(evidence.scope(callback.generic_scope().unwrap()).unwrap().quantifiers.len(), 1);
            });
            assert_eq!(actual, control);
        }
        });
    });
}

#[test]
fn generic_stage_defaults_run_per_item_and_skip_an_empty_source() {
    crate::runtime::eval::run_eval(|| {
        let generic = "proc marker() [io] -> Int { print \"default\"; 0 }\nproc callback(value, spare: Int = marker()) [io] { value }\nproc observed(values: List[Int]) [io] -> List[Int] { values |> map(callback) }\nlet empty = observed([])\nprint \"empty\"\nlet result = observed([1, 2])\nprint f\"${result[0]}:${result[1]}\"\n";
        let monomorphic = generic.replace("proc callback(value,", "proc callback(value: Int,");
        let parsed = Parser::parse_source_arena_only(crate::source::SourceId::new(0), generic);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, generic);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let stage = checked.solved.stage_operations.values().next().expect("checked map retains its source stage");
        let Some(crate::sema::check::StageCallback::Callable { requirement, declaration: Some(declaration), .. }) = stage.callback else { panic!("defaulted stage retains its original callback") };
        assert_eq!(checked.solved.graph.scheme(checked.solved.declarations[&declaration].scheme).unwrap().quantifiers.len(), 1);
        let invocation = checked.solved.graph.invocation_evidence(requirement).unwrap().expect("known Int items discharge the callback");
        let (_, binding, timing) = invocation.unique_plan().expect("callback has one original signature");
        assert_eq!(binding.supplied_slots, vec![0]);
        assert_eq!(binding.default_slots, vec![1]);
        assert_eq!(timing, crate::sema::inference::InvocationDefaultTiming::AtCall);
        assert_eq!(checked.solved.graph.closed_effect_summary(invocation.effects).unwrap(), crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::IO));
        drop(checked);
        drop(parsed);
        crate::runtime::eval::run_eval(|| {
        for force_recursive in [false, true] {
            let control = run_program_through_route_inspecting(&monomorphic, force_recursive, |_| {});
            assert_eq!(control, (0, b"empty\ndefault\ndefault\n1:2\n".to_vec(), Vec::new()));
            let actual = run_program_through_route_inspecting(generic, force_recursive, |program| {
                let evidence = program.generic_evidence().expect("defaulted generic callback retains scoped evidence");
                let callback = program.function_view(LoweredFunctionKey::Name(Name::intern("callback")), LoweredFunctionKind::Proc).unwrap().unwrap();
                assert_eq!(evidence.scope(callback.generic_scope().unwrap()).unwrap().quantifiers.len(), 1);
            });
            assert_eq!(actual, control);
        }
        });
    });
}

#[test]
fn generic_stage_preparation_preserves_frozen_counters_and_releases_solved_types() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure identity(value) { value }\npure observed() -> List[Int] { [1, 2] |> map(identity) }\nlet result = observed()\nprint f\"${result[0]}:${result[1]}\"\n";
        for force_recursive in [false, true] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("stage-frozen-evidence.xsh", source.to_string());
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let counters = checked.solved.graph.counters().clone();
            let solved = Arc::downgrade(&checked.solved);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
            let prepared = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked);
            // This observes the published owner's counters, not work in a
            // separately created context or a global solver activity counter.
            assert_eq!(checked.solved.graph.counters(), &counters);
            let plan = prepared.unwrap_or_else(|diagnostic| panic!("checked stage prepares: {diagnostic:?}"));
            drop(checked);
            drop(parsed);
            assert!(solved.upgrade().is_none(), "prepared stage must release the inference bundle");
            let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
            let output = crate::runtime::eval::run_eval(move || symbols.with_current(|| {
                let run = || {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), force_recursive);
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                };
                if force_recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(run) } else { run() }
            }));
            let output = output.unwrap_or_else(|_| panic!("prepared stage remains installed"));
            assert_eq!((output.status, output.stdout, output.stderr), (0, b"1:2\n".to_vec(), Vec::new()));
        }
    });
}
