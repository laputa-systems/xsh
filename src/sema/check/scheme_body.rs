use crate::syntax::arena::{ArenaExprKind, ArenaProgram};

pub(super) fn matches_constructor_producer(
    program: &ArenaProgram,
    expression: crate::syntax::arena::ExprId,
    kind: crate::sema::schemes::NonmaterialConstructor,
) -> bool {
    match kind {
        crate::sema::schemes::NonmaterialConstructor::Record => matches!(program.arena.expr(expression).kind, ArenaExprKind::Record(_)),
        crate::sema::schemes::NonmaterialConstructor::EmptyList => matches!(program.arena.expr(expression).kind, ArenaExprKind::List(elements) if elements.is_empty()),
        crate::sema::schemes::NonmaterialConstructor::EmptyMap => matches!(program.arena.expr(expression).kind, ArenaExprKind::Call { args, .. } if args.is_empty()),
    }
}

#[cfg(test)]
mod ordinary_body_tests {
    use crate::diagnostic::Severity;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn accepts(definition: &str, calls: &str) {
        let source = format!("{definition}\n{calls}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn generic_pattern_condition_preserves_its_branch_binding() {
        accepts(
            "pure select(value: Result[Int], label) { if let Ok(found) = value { return {label, found} }; {label, found: 0} }",
            "let selected = select(Ok(7), \"kept\")\nlet number: Int = selected.found\nlet word: Str = selected.label",
        );
    }

    #[test]
    fn generic_pattern_bindings_stay_inside_their_success_branch() {
        for source in [
            "pure escaped(value: Result[Int], payload) { if let Ok(found) = value { let _ = found }; {payload, found} }",
            "pure escaped(value: Result[Int], payload) { let first = if let Ok(found) = value { found } else { 0 }; let second = if let Ok(other) = value { found + 0 } else { 0 }; {payload, first, second} }",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckUnresolvedName)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pattern_condition_native_controls_run_on_both_routes() {
        let source = include_str!("../../../tests/xsh/generic-pattern-conditions.xsh");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pattern-conditions.xsh", crate::loader::entry_source_from_text("generic-pattern-conditions.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(std::sync::Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_pattern_conditions_preserve_success_and_failure_scopes", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
        }
    }

    #[test]
    fn generic_nominal_result_equality_reads_resolved_call_types() {
        accepts(
            "error SampleFailure = Missing\npure sample() -> Result[Int, SampleFailure] { Ok(9) }\npure attach(payload) { {payload, result: sample()} }",
            "let integer = attach(7) == {payload: 7, result: Ok(9)}\nlet text = attach(\"row\") != {payload: \"other\", result: Ok(8)}",
        );
        for comparison in [
            "attach(7) == {payload: \"wrong\", result: sample()}",
            "attach(7) != {payload: 7, result: other()}",
        ] {
            let source = format!("error SampleFailure = Missing\nerror OtherFailure = Missing\npure sample() -> Result[Int, SampleFailure] {{ Ok(9) }}\npure other() -> Result[Int, OtherFailure] {{ Ok(9) }}\npure attach(payload) {{ {{payload, result: sample()}} }}\nlet rejected = {comparison}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{source}\n{:?}", checked.diagnostics);
        }

    }

    #[test]
    fn generic_named_constants_keep_independent_parameter_instances() {
        accepts(
            "const marker = \"constant:\"\npure attach(value) { {marker: marker, value: value} }",
            "let integer = attach(42)\nlet number: Int = integer.value\nlet text = attach(\"demo\")\nlet word: Str = text.value\nlet label: Str = text.marker",
        );
    }

    #[test]
    fn generic_named_constant_tails_keep_independent_parameter_instances() {
        accepts(
            "const marker = \"constant:\"\npure read(value) { marker }",
            "let integer: Str = read(42)\nlet text: Str = read(\"demo\")",
        );
    }

    #[test]
    fn generic_parameter_shadowing_keeps_its_own_binding() {
        accepts(
            "const marker = \"constant:\"\npure attach(marker) { {value: marker} }",
            "let integer = attach(42)\nlet number: Int = integer.value\nlet text = attach(\"demo\")\nlet word: Str = text.value",
        );
    }

    #[test]
    fn generic_match_results_reuse_the_selected_parameter() {
        accepts(
            "pure choose(flag: Bool, value) { match flag { true => value; false => value } }",
            "let number: Int = choose(true, 7)\nlet text: Str = choose(false, \"value\")",
        );
    }

    #[test]
    fn generic_loop_results_reuse_the_break_payload() {
        accepts(
            "pure keep(value) { let result = loop { break value }; result }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_local_mutation_preserves_the_slot_parameter() {
        accepts(
            "proc keep(value) [] { var result = value; { result = value }; result }",
            "let number: Int = keep(7)?\nlet text: Str = keep(\"value\")?",
        );
    }

    #[test]
    fn generic_nested_bindings_retain_their_lexical_identity() {
        accepts(
            "pure keep(value) { let result = { let copy = value; copy }; result }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_discarded_values_do_not_require_a_named_binding() {
        accepts(
            "pure keep(value) { let _ = \"value\".starts_with(\"v\"); value }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_loop_results_exclude_nested_loop_payloads() {
        accepts(
            "pure keep(value) { let result = loop { let inner = loop { break 1 }; let _ = inner; break value }; result }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_body_results_reject_incompatible_consumers() {
        for definition in [
            "pure keep(value) { match true { true => value; false => value } }",
            "pure keep(value) { let result = loop { break value }; result }",
            "proc keep(value) [] { var result = value; result = value; result }",
        ] {
            let unwrap = if definition.starts_with("proc ") { "?" } else { "" };
            let source = format!("{definition}\nlet wrong: Str = keep(7){unwrap}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_mutable_slots_remain_monomorphic_within_each_call() {
        let source = "pure keep(value) { var result = value; result = 7; result }\nlet wrong: Str = keep(\"value\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_immutable_bindings_cannot_be_reassigned() {
        let source = "pure keep(value) { let result = value; result = value; result }\nlet number: Int = keep(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckAssignLet)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_branch_writers_share_one_activation_local_contract() {
        accepts(
            "pure choose(flag: Bool, left, right) { var result = left; if flag { result = right }; result }",
            "let number: Int = choose(true, 1, 7)\nlet text: Str = choose(false, \"value\", \"other\")",
        );
    }

    #[test]
    fn generic_guarded_optional_receivers_retain_their_checked_types() {
        accepts(
            "pure keep(value, items: List[Int]?) { let _ = items?[0]; value }",
            "let number: Int = keep(7, null)\nlet text: Str = keep(\"value\", [1])",
        );
    }

    #[test]
    fn generic_guarded_fields_retain_their_checked_types() {
        accepts(
            "type Row = {item: Int}\npure keep(value, row: Row?) { let _ = row?.item; value }",
            "let number: Int = keep(7, null)\nlet text: Str = keep(\"value\", {item: 1})",
        );
    }

    #[test]
    fn generic_guarded_methods_retain_the_selected_contract() {
        accepts(
            "pure keep(value, text: Str?) { let _ = text?.starts_with(\"v\"); value }",
            "let number: Int = keep(7, null)\nlet text: Str = keep(\"value\", \"value\")",
        );
    }

    #[test]
    fn generic_guarded_result_receivers_retain_the_error_contract() {
        accepts(
            "pure keep(value, items: Result[List[Int], Str]) -> Result[Int, Str] { let _ = value; Ok(items?[0]) }",
            "let number: Result[Int, Str] = keep(7, Ok([1]))\nlet text: Result[Int, Str] = keep(\"value\", Err(\"failed\"))",
        );
    }

    #[test]
    fn generic_validated_aggregate_projections_keep_their_base_contract() {
        for operation in ["items[0]", "items[0..1]"] {
            accepts(
                &format!("pure select(value, items: NonEmpty[Int]) {{ let _ = {operation}; value }}"),
                "let number: Int = select(7, [1])\nlet text: Str = select(\"value\", [2])",
            );
        }
    }

    #[test]
    fn generic_list_concatenation_preserves_invariant_unsigned_items() {
        let source = "pure keep(value, unsigned: List[UInt], signed: List[Int]) { let _ = unsigned + signed; value }";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch) && diagnostic.labels.iter().any(|label| label.message.as_deref() == Some("expected List[UInt], found List[Int]"))), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_index_instances_report_index_diagnostics() {
        accepts(
            "pure select(values, key) { values[key] }",
            "let integer: Int = select([7], 0)\nlet text: Str = select({[0]: \"value\"}, 0)",
        );
        let source = "pure select(values, key) { values[key] }\nlet _ = select(7, 0)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckIndexType)), "{:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckOperatorType)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_slice_instances_report_slice_diagnostics() {
        accepts(
            "pure select(values) { values[0..1] }",
            "let integers: List[Int] = select([7, 8])\nlet text: Str = select(\"value\")",
        );
        let source = "pure select(values) { values[0..1] }\nlet _ = select(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckSliceType)), "{:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckOperatorType)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_list_from_end_index_preserves_the_element_parameter() {
        accepts(
            "pure select(value) { [value][-1] }",
            "let number: Int = select(7)\nlet text: Str = select(\"value\")",
        );
    }

    #[test]
    fn generic_list_index_preserves_the_element_parameter() {
        accepts(
            "pure select(value) { [value][0] }",
            "let number: Int = select(7)\nlet text: Str = select(\"value\")",
        );
    }

    #[test]
    fn generic_list_slice_preserves_the_element_parameter() {
        accepts(
            "pure select(value) { [value, value][0..1] }",
            "let numbers: List[Int] = select(7)\nlet texts: List[Str] = select(\"value\")",
        );
    }

    #[test]
    fn generic_record_index_preserves_the_selected_field() {
        accepts(
            "pure select(value) { {item: value}[\"item\"] }",
            "let number: Int = select(7)\nlet text: Str = select(\"value\")",
        );
    }

    #[test]
    fn generic_grounded_operators_preserve_unrelated_parameters() {
        accepts(
            "pure select(value, number: Int) { let _ = number + 1; value }",
            "let number: Int = select(7, 1)\nlet text: Str = select(\"value\", 2)",
        );
    }

    #[test]
    fn generic_match_bindings_remain_local_to_the_selected_arm() {
        accepts(
            "pure keep(value) { match value { selected => selected } }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_construction_formats_checked_scalar_operands() {
        accepts(
            "pure keep(value, text: Str, count: Int) { let _ = f\"{text}:{count}\"; value }",
            "let number: Int = keep(7, \"item\", 2)\nlet text: Str = keep(\"value\", \"item\", 3)",
        );
    }

    #[test]
    fn generic_construction_formats_checked_path_operands() {
        accepts(
            "pure keep(value, root: Path, name: Str) { let _ = fp\"{root}/{name}\"; value }",
            "let number: Int = keep(7, /tmp, \"item\")\nlet text: Str = keep(\"value\", /tmp, \"other\")",
        );
    }

    #[test]
    fn generic_construction_splices_lists_with_one_payload_binder() {
        accepts(
            "pure copy(value) { [@[value], value] }",
            "let number: List[Int] = copy(7)\nlet text: List[Str] = copy(\"value\")",
        );
    }

    #[test]
    fn generic_construction_spreads_records_with_one_payload_binder() {
        accepts(
            "type Numbers = {item: Int, count: Int}\ntype Words = {item: Str, count: Int}\npure copy(value) { let row = {item: value}; {...row, count: 1} }",
            "let number: Numbers = copy(7)\nlet text: Words = copy(\"value\")",
        );
    }

    #[test]
    fn generic_construction_maps_retain_key_and_payload_domains() {
        accepts(
            "pure copy(value) { {[0]: value} }",
            "let number: Map[Int, Int] = copy(7)\nlet text: Map[Int, Str] = copy(\"value\")",
        );
    }

    #[test]
    fn generic_construction_sets_retain_the_checked_scalar_domain() {
        accepts(
            "pure keep(value, item: Int) { let _ = {1, item}; value }",
            "let number: Int = keep(7, 2)\nlet text: Str = keep(\"value\", 3)",
        );
    }

    #[test]
    fn generic_iteration_record_bindings_retain_renamed_payloads() {
        accepts("pure keep(value) { let {item: copy} = {item: value}; copy }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_nested_bindings_retain_projected_payloads() {
        accepts("pure keep(value) { let {nested: {item: copy, ..}, ..} = {nested: {item: value, ignored: 1}, extra: true}; copy }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_shadowed_bindings_keep_lexical_identity() {
        accepts("pure keep(value) { let {item: copy} = {item: value}; let result = { let {item: copy} = {item: copy}; copy }; result }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_mutable_record_bindings_keep_one_payload_domain() {
        accepts("proc keep(value) [] { var {item: copy} = {item: value}; copy = value; copy }", "let number: Int = keep(7)?\nlet text: Str = keep(\"value\")?");
    }

    #[test]
    fn sealed_binary_arithmetic_retains_independent_input_and_result_domains() {
        accepts("pure combine(a, b) { a + b }", "let number: Int = combine(1, 2)\nlet text: Str = combine(\"one\", \" two\")");
    }

    #[test]
    fn wrapper_fallback_retains_optional_result_payloads_and_result_failure_modes() {
        let calls = "let failed: Result[Int, Str] = Err(\"kept\")\nlet present: Result[Int, Str]? = failed\nlet kept: Result[Int, Str] = optional_fallback(present, Ok(0))\nlet recovered: Int = result_fallback(failed, 0)";
        accepts("pure optional_fallback(value: Result[Int, Str]?, backup: Result[Int, Str]) -> Result[Int, Str] { value ?? backup }\npure result_fallback(value: Result[Int, Str], backup: Int) -> Int { value ?? backup }", calls);
        accepts("pure fallback(value, backup) { value ?? backup }", "let failed: Result[Int, Str] = Err(\"kept\")\nlet present: Result[Int, Str]? = failed\nlet kept: Result[Int, Str] = fallback(present, Ok(0))\nlet recovered: Int = fallback(failed, 0)");
    }

    #[test]
    fn materialized_named_callable_pure_forwarders_preserve_origin_boundaries() {
        let definition = "pure identity(payload) { payload }\npure apply(action, value) { action(value) }";
        for body in ["apply(identity, value)", "return apply(identity, value)"] {
            accepts(&format!("{definition}\npure relay(value) {{ {body} }}"), "let number: Int = relay(7)\nlet text: Str = relay(\"value\")");
        }
    }

    #[test]
    fn materialized_named_callable_arguments_preserve_generic_body_admission() {
        let definition = "pure identity(payload) { payload }\nproc keep(action, value) { let _ = action(value); value }";
        for body in ["keep(identity, wrapped)", "return keep(identity, wrapped)"] {
            accepts(&format!("{definition}\nproc relay(value) {{ let wrapped = Ok(value); {body} }}"), "let number: Result[Result[Int]] = relay(7)\nlet text: Result[Result[Str]] = relay(\"value\")");
        }
        accepts(&format!("{definition}\nproc relay(action, value) {{ let wrapped = Ok(value); keep(action, wrapped) }}"), "let number: Result[Result[Int]] = relay(identity, 7)\nlet text: Result[Result[Str]] = relay(identity, \"value\")");
    }

    #[test]
    fn materialized_named_callable_arguments_preserve_effect_and_dynamic_rejections() {
        for source in [
            "proc effect(payload) [env] { let home = env.get(\"HOME\"); payload }\npure apply(action, value) { action(value) }\npure relay(value) { apply(effect, value) }\nlet number = relay(7)\n",
            "pure apply(action, value) { action(value) }\npure relay(action: Any, value) { apply(action, value) }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{source}");
        }
    }

    #[test]
    fn wrapper_empty_list_fallbacks_preserve_independent_row_items() {
        let rows = "type IntRow = {items: List[Int]}\ntype StrRow = {items: List[Str]}";
        let calls = "let numbers: IntRow? = {items: [2, 4]}\nlet absent: IntRow? = null\nlet words: StrRow? = {items: [\"a\", \"b\"]}\nlet numeric: List[Int] = inspect(numbers)\nlet empty: List[Int] = inspect(absent)\nlet textual: List[Str] = inspect(words)";
        for body in ["maybe?.items ?? []", "let source = maybe; source?.items ?? []"] {
            accepts(&format!("{rows}\npure inspect(maybe) {{ {body} }}"), calls);
        }
        for body in ["items(maybe)", "let lookup = items; lookup(maybe)"] {
            accepts(&format!("{rows}\npure items(maybe) {{ maybe?.items ?? [] }}\npure inspect(maybe) {{ {body} }}"), calls);
        }
    }

    #[test]
    fn wrapper_empty_list_actuals_preserve_source_and_named_argument_order() {
        let definition = "type IntRow = {items: List[Int]}\ntype StrRow = {items: List[Str]}\npure inspect(maybe, otherwise) { maybe?.items ?? otherwise }";
        for arguments in ["numbers, []", "maybe: numbers, otherwise: []", "otherwise: [], maybe: numbers"] {
            let calls = format!("let numbers: IntRow? = {{items: [2, 4]}}\nlet words: StrRow? = {{items: [\"a\"]}}\nlet numeric: List[Int] = inspect({arguments})\nlet textual: List[Str] = inspect({})", arguments.replace("numbers", "words"));
            accepts(definition, &calls);
        }
    }

    #[test]
    fn contextual_empty_lists_preserve_dynamic_and_unrooted_rejections() {
        for source in [
            "let bare = []\nlet scalar: Int = bare",
            "pure empty(marker) { let _ = marker; [] }\nlet unrooted = empty(7)",
            "let erased: Any = []\nlet concrete: List[Int] = erased",
            "pure inspect(maybe) { maybe?.items ?? [] }\nlet erased: Any = null\nlet value = inspect(erased)",
            "type Row = {items: Int}\npure inspect(maybe) { maybe?.items ?? [] }\nlet row: Row? = {items: 7}\nlet invalid = inspect(row)",
            "proc effectful(value) [env] { let _ = env.get(\"XSH_EMPTY_LIST_EFFECT\"); value }\npure forward(value) { effectful(value) }",
            "proc effectful(value) [env] { let _ = env.get(\"XSH_EMPTY_LIST_EFFECT\"); value }\npure forward(value) { let lookup = effectful; lookup(value) }",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
        accepts("", "let bare = []\nlet explicit: Any = []");
    }

    #[test]
    fn result_pattern_calls_close_only_constructor_phantom_errors() {
        let definition = "pure choose(outcome, fallback) { match outcome { Ok(value) => value; Err(_) => fallback } }";
        let calls = "let number = choose(Ok(9), 4)\nlet word = choose(Ok(\"ready\"), \"waiting\")";
        let source = format!("{definition}\n{calls}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let (work, checked) = crate::sema::inference_work::collect(|| Checker::check_arena(&parsed.arena, &source));
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        assert_eq!(work.function_body_check_entries, 1);
        assert_eq!(checked.function_body_check_entries, 1);
        for call in checked.call_instantiations.values().flat_map(|evidence| evidence.instances()) {
            assert!(call.type_arguments().values().any(|ty| *ty == crate::sema::schemes::SchemeType::Ground(crate::sema::types::Type::Error)));
        }
        accepts("", &format!("{calls}\n{definition}"));
        accepts(&format!("{definition}\npure wrapped(value) {{ choose(Ok(value), value) }}"), "let number = wrapped(9)\nlet word = wrapped(\"ready\")");
        let definition = "error Failure = Missing\npure same(left, right) { var selected = left; selected = right; selected }";
        for call in ["same(Ok(9), typed)", "same(right: typed, left: Ok(9))", "same(left: Ok(9), right: typed)"] {
            accepts(definition, &format!("let typed: Result[Int, Failure] = Err(Failure.Missing())\nlet actual = {call}\nlet verified: Result[Int, Failure] = actual"));
        }
        accepts(&format!("{definition}\npure relay(value) {{ same(Ok(9), value) }}"), "let typed: Result[Int, Failure] = Err(Failure.Missing())\nlet actual = relay(typed)\nlet verified: Result[Int, Failure] = actual");
        accepts(definition, "let typed: Result[Int, Failure] = Err(Failure.Missing())\nlet actual = same(same(Ok(9), Ok(9)), typed)\nlet verified: Result[Int, Failure] = actual");
    }

    #[test]
    fn constructor_phantom_defaults_preserve_bare_and_explicit_contexts() {
        accepts("error Failure = Missing", "let direct: Result[Int, Failure] = Ok(9)\nlet bare = Ok(9)\nlet verified: Result[Int, Error] = bare");
        for source in [
            "let bare = Ok(9)\nlet incompatible: Result[Int, Str] = bare",
            "error Failure = Missing\npure same(left, right) { var selected = left; selected = right; selected }\nlet incompatible: Result[Int, Failure] = same(Ok(9), Ok(9))",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn wrapper_fallback_constructor_context_preserves_result_error_payloads() {
        let calls = "let source: Result[Int, Str]? = null\nlet selected: Result[Int, Str] = fallback(source)";
        accepts("pure fallback(value: Result[Int, Str]?) -> Result[Int, Str] { value ?? Ok(0) }", calls);
        accepts("pure fallback(value) { value ?? Ok(0) }", calls);
        accepts("pure identity(value) { value }", "let ordinary: Result[Int] = identity(Ok(0))");
    }

    #[test]
    fn wrapper_dynamic_return_boundaries_preserve_grounded_annotation_policy() {
        for definition in [
            "pure direct(value: Row?) { try { value?.name } }",
            "pure name(value) { try { value?.name } }\npure direct(value: Row?) { name(value) }",
            "pure direct(value: Any?, backup: Any) { value ?? backup }",
            "pure fallback(value, backup) { value ?? backup }\npure direct(value: Any?, backup: Any) { fallback(value, backup) }",
        ] {
            let source = format!("type Row = {{name: Any}}\n{definition}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
            assert!(!errors.is_empty() && errors.iter().all(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckInferReturn)), "{source}\n{errors:?}");
        }
    }

    #[test]
    fn wrapper_handler_constructor_context_preserves_result_error_payloads() {
        let calls = "let source: Result[Result[Int, Str], Bool] = Err(true)\nlet selected: Result[Int, Str] = handled(source)";
        accepts("pure handled(value: Result[Result[Int, Str], Bool]) -> Result[Int, Str] { value ?? { |error| Ok(0) } }", calls);
        accepts("pure handled(value) { value ?? { |error| Ok(0) } }", calls);
        accepts("pure identity(value) { value }", "let ordinary: Result[Int] = identity(Ok(0))");
    }

    #[test]
    fn wrapper_fallback_context_survives_reversed_named_argument_order() {
        let calls = "let present: Result[Int, Str]? = Err(\"kept\")\nlet kept: Result[Int, Str] = optional_fallback(backup: Ok(0), value: present)";
        accepts("pure optional_fallback(value: Result[Int, Str]?, backup: Result[Int, Str]) -> Result[Int, Str] { value ?? backup }", calls);
        accepts("pure fallback(value, backup) { value ?? backup }", &calls.replace("optional_fallback(", "fallback("));
        accepts("pure identity(value) { value }", "let ordinary: Result[Int] = identity(Ok(0))");
    }

    #[test]
    fn wrapper_fields_retain_independent_optional_payloads_and_optional_field_idempotence() {
        let calls = "type TextRow = {name: Str}\ntype IntRow = {name: Int}\ntype MaybeRow = {name: Str?}\nlet text: TextRow? = {name: \"demo\"}\nlet number: IntRow? = {name: 7}\nlet absent: TextRow? = null\nlet nested: MaybeRow? = {name: null}\nlet text_name: Str? = optional_name(text)\nlet number_name: Int? = optional_name(number)\nlet missing_name: Str? = optional_name(absent)\nlet nullable_name: Str? = optional_name(nested)";
        accepts("pure optional_name(value) { value?.name }", calls);
    }

    #[test]
    fn wrapper_fields_retain_result_errors_and_caught_optional_or_result_sources() {
        accepts("type TextRow = {name: Str}\npure result_name(value: Result[TextRow, Str]) -> Result[Str, Str] { Ok(value?.name) }", "let source: Result[TextRow, Str] = Ok({name: \"demo\"})\nlet name: Result[Str, Str] = result_name(source)");
        accepts("type TextRow = {name: Str}\npure captured_optional(value: TextRow?) { try { value?.name } }", "let source: TextRow? = {name: \"demo\"}\nlet name: Result[Str?] = captured_optional(source)");
        accepts("type TextRow = {name: Str}\npure captured_result(value: Result[TextRow, Str]) { try { value?.name } }", "let source: Result[TextRow, Str] = Ok({name: \"demo\"})\nlet name: Result[Str, Str] = captured_result(source)");
        accepts("type TextRow = {name: Str}\npure result_name(value) -> Result[Str, Str] { Ok(value?.name) }", "let source: Result[TextRow, Str] = Ok({name: \"demo\"})\nlet name: Result[Str, Str] = result_name(source)");
        accepts("type TextRow = {name: Str}\npure captured_name(value) { try { value?.name } }", "let optional: TextRow? = {name: \"demo\"}\nlet source: Result[TextRow, Str] = Ok({name: \"demo\"})\nlet optional_name: Result[Str?] = captured_name(optional)\nlet result_name: Result[Str, Str] = captured_name(source)");
    }

    #[test]
    fn generic_guarded_field_effects_fit_wider_and_error_capturing_callers() {
        let source = concat!(
            "type TextRow = {name: Str}\n",
            "proc projected(value) { value?.name }\n",
            "proc wider(value) [error, fs] { projected(value) }\n",
            "proc captured(value) [] { try { projected(value)? } }\n",
            "let text: Result[TextRow, Str] = Ok({name: \"demo\"})\n",
            "let wider_name: Result[Str, Str] = wider(text)\n",
            "let captured_name = captured(text)\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{errors:?}");

        for ceiling in ["[]", "[fs]"] {
            let rejected = format!(
                "type TextRow = {{name: Str}}\nproc projected(value) {{ value?.name }}\nproc rejected(value: Result[TextRow, Str]) {ceiling} {{ projected(value) }}\n",
            );
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &rejected);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &rejected, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{ceiling}: {:?}", checked.diagnostics);
        }

        let host_effect = "proc blocked() [] { try { run sh -c \"exit 0\"; 1 } }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), host_effect);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, host_effect, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn wrapper_fields_preserve_inferred_proc_completion_for_result_sources() {
        let calls = "let source: Result[TextRow, Str] = Ok({name: \"demo\"})\nlet projected: Result[Str, Str] = project(source)";
        accepts("type TextRow = {name: Str}\nproc project(value: Result[TextRow, Str]) { value?.name }", calls);
        accepts("type TextRow = {name: Str}\nproc project(value) { value?.name }", calls);
        accepts(
            "type TextRow = {name: Str}\nproc defaulted(value: TextRow) { value.name }",
            "let projected: Result[Str] = defaulted({name: \"demo\"})",
        );
    }

    #[test]
    fn wrapper_fields_preserve_declared_error_permission_for_scalar_proc_returns() {
        let calls = "let source: Result[TextRow, Str] = Ok({name: \"demo\"})\nlet name: Str = project(source)";
        accepts("type TextRow = {name: Str}\nproc project(value: Result[TextRow, Str]) [error] -> Str { value?.name }", calls);
        accepts("type TextRow = {name: Str}\nproc project(value) [error] -> Str { value?.name }", calls);
        for declaration in [
            "pure project(value) -> Str { value?.name }",
            "proc project(value) [] -> Str { value?.name }",
            "proc project(value) [error] -> Result[Str, Int] { Ok(value?.name) }",
        ] {
            let source = format!("type TextRow = {{name: Str}}\n{declaration}\nlet source: Result[TextRow, Str] = Ok({{name: \"demo\"}})\nlet _ = project(source)");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code, Some(crate::diagnostic::DiagnosticCode::CheckTryContext | crate::diagnostic::DiagnosticCode::CheckEffectViolation | crate::diagnostic::DiagnosticCode::CheckNullSafeField | crate::diagnostic::DiagnosticCode::CheckCallableMismatch))), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn wrapper_fields_reject_a_second_optional_hop_and_unsupported_receivers() {
        for source in [
            "type TextRow = {name: Str}\npure result_name(value: Result[TextRow?, Str]) -> Result[Str, Str] { Ok(value?.name) }",
            "pure optional_name(value) { value?.name }\nlet invalid = optional_name(1)",
            "pure optional_name(value) { value?.name }\nlet invalid = optional_name({name: \"demo\"})",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckNullSafeField)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn sealed_binary_arithmetic_rejects_an_unsupported_input_tuple() {
        let source = "pure combine(a, b) { a + b }\nlet rejected = combine(1, true)";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckOperatorType)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn inferred_failure_only_result_successes_keep_generic_domains() {
        let source = "pure attempt(flag, value) { if flag { return Err(error.failure(\"stop\")) }; Ok(value) }\npure tail_attempt(flag, value) { if flag { return Ok(value) }; Err(error.failure(\"tail\")) }\nlet generic_int: Result[Int] = attempt(false, 7)\nlet generic_text: Result[Str] = attempt(false, \"kept\")\nlet generic_failure: Result[Int] = attempt(true, 7)\nlet tail_failure: Result[Int] = tail_attempt(true, 4)";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            crate::sema::check::CheckOptions::for_execution(),
        );
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
        let attempt = checked.checked_declarations.functions.values().find(|function| function.name.as_str().as_str() == "attempt").unwrap();
        let attempt_scheme = attempt.scheme.as_ref().expect("attempt retains its inferred type parameter");
        let [attempt_value] = attempt_scheme.type_parameters() else { panic!("the Result success is generic: {attempt_scheme:?}"); };
        assert_eq!(attempt_scheme.params()[1].ty, crate::sema::schemes::SchemeType::Parameter(*attempt_value));
        assert_eq!(attempt_scheme.return_type(), &crate::sema::schemes::SchemeType::Result(
            Box::new(crate::sema::schemes::SchemeType::Parameter(*attempt_value)),
            Box::new(crate::sema::schemes::SchemeType::Ground(crate::sema::types::Type::Error)),
        ));
        let tail_attempt = checked.checked_declarations.functions.values().find(|function| function.name.as_str().as_str() == "tail_attempt").unwrap();
        let tail_scheme = tail_attempt.scheme.as_ref().expect("tail Err preserves its inferred success parameter");
        let [tail_value] = tail_scheme.type_parameters() else { panic!("tail failure keeps its successful return generic: {tail_scheme:?}"); };
        assert_eq!(tail_scheme.params()[1].ty, crate::sema::schemes::SchemeType::Parameter(*tail_value));
        assert_eq!(tail_scheme.return_type(), &crate::sema::schemes::SchemeType::Result(
            Box::new(crate::sema::schemes::SchemeType::Parameter(*tail_value)),
            Box::new(crate::sema::schemes::SchemeType::Ground(crate::sema::types::Type::Error)),
        ));
    }

    #[test]
    fn sealed_binary_arithmetic_preserves_grounded_unsigned_duration_rejection() {
        for source in [
            "let unsigned: UInt = 2\nlet rejected = 1s * unsigned",
            "pure multiply(a, b) { a * b }\nlet unsigned: UInt = 2\nlet rejected = multiply(1s, unsigned)",
            "pure multiply(a, b) { a * b }\nlet unsigned: UInt = 2\nlet rejected = multiply(unsigned, 1s)",
            "pure divide(a, b) { a / b }\nlet unsigned: UInt = 2\nlet rejected = divide(1s, unsigned)",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckOperatorType)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn sealed_binary_arithmetic_propagates_unique_structural_input_domains() {
        accepts("pure append(values) { values + [1] }", "let values: List[Int] = append([2])");
        accepts("pure numbers() { [] + [1] }", "let values: List[Int] = numbers()");
    }

    #[test]
    fn generic_iteration_input_relationship_accepts_independent_collection_domains() {
        accepts("pure collect(values) { [value for value in values] }", "let numbers: List[Int] = collect([1, 2])\nlet words: List[Str] = collect([\"a\"])\nlet letters: List[Str] = collect(\"ab\")");
    }

    #[test]
    fn generic_iteration_input_relationship_rejects_an_unsupported_instance() {
        let source = "pure collect(values) { [value for value in values] }\nlet rejected = collect(7)";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_iteration_list_items_retain_the_payload_binder() {
        accepts("pure keep(value) { var result = value; for item in [value] { result = item }; result }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_record_items_retain_projected_payloads() {
        accepts("pure keep(value) { var result = value; for {item: copy} in [{item: value}] { result = copy }; result }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_nested_loops_own_break_and_continue() {
        accepts("pure keep(value) { var result = value; for row in [[value]] { for item in row { result = item; continue }; break }; result }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_closed_sources_keep_their_item_domains() {
        accepts("pure keep(value) { for item in [1] { let _ = item }; for byte in b\"x\" { let _ = byte }; for letter in \"x\" { let _ = letter }; for entry in {[\"key\"]: value} { let _ = entry.value }; value }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_comprehension_list_items_retain_the_payload_binder() {
        accepts("pure copy(value) { [item for item in [value]] }", "let number: List[Int] = copy(7)\nlet text: List[Str] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_nested_targets_keep_sequential_binding_identity() {
        accepts("pure copy(value) { [inner for outer in [value] for {item: inner} in [{item: outer}]] }", "let number: List[Int] = copy(7)\nlet text: List[Str] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_shadowed_targets_read_the_previous_binding_first() {
        accepts("pure copy(value) { [item for item in [value] for item in [item]] }", "let number: List[Int] = copy(7)\nlet text: List[Str] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_filters_preserve_the_selected_payload_type() {
        accepts("pure copy(value, selected: Bool) { [item for item in [value] if selected] }", "let number: List[Int] = copy(7, true)\nlet text: List[Str] = copy(\"value\", false)");
    }

    #[test]
    fn generic_comprehension_map_values_retain_the_payload_binder() {
        accepts("pure copy(value) { {key: value for key in [\"a\", \"b\"]} }", "let number: Map[Str, Int] = copy(7)\nlet text: Map[Str, Str] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_set_items_keep_the_closed_key_domain() {
        accepts("pure copy(value, items: List[Int]) { let unique = {item for item in items}; {value, unique} }", "let number = copy(7, [1, 2])\nlet text = copy(\"value\", [2, 3])");
    }

    #[test]
    fn generic_comprehension_nonempty_mapping_preserves_its_source_validation() {
        accepts("pure copy(value, items: NonEmpty[Int]) { let copied = [item for item in items]; {value, copied} }", "let number = copy(7, [1])\nlet text = copy(\"value\", [2])");
    }

    #[test]
    fn generic_comprehension_result_map_items_retain_the_success_binder() {
        accepts("pure copy(value) { Ok([item.value for item in Ok({[\"key\"]: value})]) }", "let number: Result[List[Int]] = copy(7)\nlet text: Result[List[Str]] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_filters_retain_the_ordinary_status_condition() {
        accepts("pure copy(value, selected: Status) { [item for item in [value] if selected] }", "pure number(selected: Status) -> List[Int] { copy(7, selected) }\npure text(selected: Status) -> List[Str] { copy(\"value\", selected) }");
    }

    #[test]
    fn generic_arguments_named_order_preserves_the_payload_binder() {
        accepts("pure select(value, flag: Bool) { let _ = flag; value }\npure call(value) { select(flag: true, value: value) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_named_spreads_preserve_the_payload_binder() {
        accepts("pure select(value, flag: Bool) { let _ = flag; value }\npure call(value) { select(...{flag: true, value: value}) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_local_spreads_keep_the_checked_record_shape() {
        accepts("pure select(value, flag: Bool) { let _ = flag; value }\npure call(value) { let args = {flag: true, value: value}; select(...args) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_rest_calls_preserve_the_fixed_payload() {
        accepts("pure head(value, ...items: List[Int]) { let _ = items; value }\npure call(value) { let _ = head(value); head(value, 1, 2) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_rest_splices_keep_the_actual_list_domain() {
        accepts("pure head(value, ...items: List[Int]) { let _ = items; value }\npure call(value, tail: List[Int]) { head(value, @tail) }", "let number: Int = call(7, [1, 2])\nlet text: Str = call(\"value\", [])");
    }

    #[test]
    fn generic_arguments_omitted_defaults_keep_the_declaration_contract() {
        accepts("pure select(value, flag: Bool = false) { let _ = flag; value }\npure call(value) { select(value) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_explicit_defaults_keep_the_supplied_payload() {
        accepts("pure select(value, flag: Bool = false) { let _ = flag; value }\npure call(value) { select(flag: true, value: value) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_inferred_default_types_remain_independent_of_calls() {
        accepts("pure select(value, flag = false) { let _ = flag; value }", "let number: Int = select(7)\nlet text: Str = select(\"value\", true)");
    }

    #[test]
    fn generic_assignment_list_entries_retain_the_payload_binder() {
        accepts("pure replace(value, other) { var items = [value]; items[0] = other; items[0] }", "let number: Int = replace(7, 8)\nlet text: Str = replace(\"value\", \"other\")");
    }

    #[test]
    fn generic_assignment_record_paths_retain_the_payload_binder() {
        accepts("pure replace(value, other) { var row = {nested: {item: value}, count: 1}; row.nested.item = other; row.nested.item }", "let number: Int = replace(7, 8)\nlet text: Str = replace(\"value\", \"other\")");
    }

    #[test]
    fn generic_assignment_map_entries_retain_the_payload_binder() {
        accepts("pure replace(value, other, key: Str) { var items = {[key]: value}; items[key] = other; items[key] }", "let number: Int = replace(7, 8, \"key\")\nlet text: Str = replace(\"value\", \"other\", \"key\")");
    }

    #[test]
    fn generic_assignment_mixed_selectors_retain_the_selected_payload() {
        accepts("pure replace(value, other, index: Int) { var items = [{nested: [value]}]; items[index].nested[0] = other; items[index].nested[0] }", "let number: Int = replace(7, 8, 0)\nlet text: Str = replace(\"value\", \"other\", 0)");
    }

    #[test]
    fn generic_assignment_compound_lists_keep_invariant_elements() {
        accepts("pure append(value, other) { var items = [value]; items += [other]; items }", "let number: List[Int] = append(7, 8)\nlet text: List[Str] = append(\"value\", \"other\")");
    }

    #[test]
    fn generic_assignment_scalar_compounds_keep_the_checked_operator_domains() {
        accepts("pure update(value, delta: Int) { var count = 20; count += delta; count -= 1; count *= 2; count /= 2; count %= 7; {value, count} }", "let number: Int = update(7, 3).value\nlet text: Str = update(\"value\", 4).value");
    }

    #[test]
    fn generic_assignment_unsigned_fields_keep_the_validation_boundary() {
        for definition in [
            "pure update(value, initial: UInt, delta: Int) { var row = {value, count: initial}; row.count += delta; row }",
            "pure update(value, initial: UInt, delta: Int) { var {item: row} = {item: {value, count: initial}}; row.count += delta; row }",
        ] {
            accepts(definition, "let number: Int = update(7, 1, 2).value\nlet text: Str = update(\"value\", 3, 4).value");
        }
    }

    #[test]
    fn generic_assignment_unsigned_map_keys_preserve_the_payload_binder() {
        accepts("pure put(value, other, key: UInt, index: Int) { var items = {[key]: value}; items[index] = other; items[key] }", "pure number(key: UInt, index: Int) -> Int { put(7, 8, key, index) }\npure text(key: UInt, index: Int) -> Str { put(\"value\", \"other\", key, index) }");
    }

    #[test]
    fn generic_assignment_updates_preserve_earlier_alias_types() {
        accepts("pure replace(value, other) { var items = [value]; let before = items; items[0] = other; {before, after: items} }", "let number: Int = replace(7, 8).before[0]\nlet text: Str = replace(\"value\", \"other\").after[0]");
    }

    #[test]
    fn generic_operators_logical_values_retain_the_payload_binder() {
        accepts("pure keep(value, first: Bool, second: Bool) { let _ = first and second; let _ = first or second; value }", "let number: Int = keep(7, true, false)\nlet text: Str = keep(\"value\", false, true)");
    }

    #[test]
    fn generic_operators_equality_uses_the_same_symbolic_domain() {
        accepts("pure keep(value) { let _ = value == value; let _ = value != value; value }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_operators_ordering_keeps_closed_domains() {
        accepts("pure keep(value, number: Int, text: Str, elapsed: Duration) { let _ = number < 7; let _ = text >= \"a\"; let _ = elapsed <= 1s; value }", "let number: Int = keep(7, 2, \"value\", 2s)\nlet text: Str = keep(\"value\", 3, \"other\", 1s)");
    }

    #[test]
    fn generic_operators_ordering_chains_keep_adjacent_operands() {
        accepts("pure keep(value, number: Int) { let _ = 0 <= number < 7; value }", "let number: Int = keep(7, 2)\nlet text: Str = keep(\"value\", 3)");
    }

    #[test]
    fn generic_proc_bare_returns_keep_the_unit_success_contract() {
        for body in ["return when done; let _ = marker", "if done { return }; let _ = marker", "let _ = marker; return"] {
            accepts(&format!("proc early(marker: Int, done: Bool) [] {{ {body} }}"), "let first = early(7, true)\nlet second = early(8, false)");
            for calls in ["", "let first = early(7, true)\nlet second = early(\"word\", false)"] {
                accepts(&format!("proc early(marker, done: Bool) {{ {body} }}"), calls);
            }
        }
    }

    #[test]
    fn generic_proc_bare_returns_preserve_value_effect_and_cleanup_rejections() {
        for source in [
            "proc wrong(marker, done: Bool) -> Result[Int] { let _ = marker; return when done; 7 }",
            "proc wrong(marker) [] { print marker; return }",
            "proc wrong(marker) { defer { return }; let _ = marker }",
            "pure wrong(marker) -> Int { let _ = marker; return }",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn membership_container_typed_declaration_keeps_its_ordinary_domain() {
        accepts("pure has(item: Int, container: List[Int]) -> Bool { item in container }", "assert has(7, [7])\nassert !has(8, [7])");
    }

    #[test]
    fn membership_container_generic_declaration_retains_independent_call_domains() {
        for calls in [
            "",
            "assert has(7, [7])\nassert has(\"key\", \"a key\")",
            "assert has(\"key\", \"a key\")\nassert has(7, [7])",
        ] {
            accepts("pure has(item, container) { item in container }", calls);
        }
    }

    #[test]
    fn generic_membership_rejects_wrong_domains_and_dynamic_containers() {
        for source in [
            "pure has(item, container) { item in container }\nlet _ = has(true, [1])",
            "pure has(item, container) { item in container }\nlet _ = has(1, true)",
            "pure has(item, container) { item in container }\nlet _ = has(1, \"word\")",
            "pure has(item, container) { item in container }\nlet _ = has(\"key\", b\"bytes\")",
            "pure has(item, container) { item in container }\nlet _ = has(1, {key: 7})",
            "pure has(item, container) { item in container }\nlet _ = has(\"key\", {[1]: 7})",
            "pure has(item, container) { item in container }\nproc unknown(container: Any) [] { has(1, container) }",
            "pure forbidden(item: Any, container: Any) { item in container }",
            "enum MembershipFirst: Str { MembershipBuild = \"build\" }\nenum MembershipSecond: Str { MembershipTask = \"task\" }\npure has(item, container) { item in container }\nlet _ = has(MembershipBuild, [MembershipTask])",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_operators_list_membership_keeps_the_symbolic_item() {
        accepts("pure keep(value) { let _ = value in [value]; let _ = value not in [value]; value }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_operators_membership_preserves_closed_key_domains() {
        accepts("pure keep(value, keys: Map[UInt, Int], key: UInt, elements: Set[Int]) { let _ = key in keys; let _ = 1 not in elements; let _ = \"a\" in \"value\"; value }", "let number: Int = keep(7, {}, 1, {1, 2})\nlet text: Str = keep(\"value\", {}, 2, {2, 3})");
    }

    #[test]
    fn generic_operators_set_algebra_keeps_the_element_domain() {
        accepts("pure keep(value, left: Set[Int], right: Set[Int]) { let _ = left | right; let _ = left & right; let _ = left - right; value }", "let number: Int = keep(7, {1, 2}, {2, 3})\nlet text: Str = keep(\"value\", {2, 3}, {3, 4})");
    }

    #[test]
    fn generic_record_forms_shorthand_retains_the_parameter_binder() {
        accepts("type Number = {value: Int}\ntype Text = {value: Str}\npure copy(value) { {value} }", "let number: Number = copy(7)\nlet text: Text = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_shorthand_follows_local_shadowing() {
        accepts("type Number = {item: Int}\ntype Text = {item: Str}\npure copy(value) { let item = value; { let item = 1; let _ = {item} }; {item} }", "let number: Number = copy(7)\nlet text: Text = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_shorthand_retains_an_immutable_capture() {
        accepts("let shared = 1\ntype Number = {value: Int, shared: Int}\ntype Text = {value: Str, shared: Int}\npure copy(value) { {value, shared} }", "let number: Number = copy(7)\nlet text: Text = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_map_shorthand_keeps_the_payload_domain() {
        accepts("pure copy(value) { {[\"other\"]: value, value} }", "let number: Map[Str, Int] = copy(7)\nlet text: Map[Str, Str] = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_set_shorthand_keeps_the_checked_domain() {
        accepts("pure keep(value, first: Int, second: Int) { let items: Set[Int] = {first, second}; let _ = items; value }", "let number: Int = keep(7, 1, 2)\nlet text: Str = keep(\"value\", 2, 3)");
    }

    #[test]
    fn generic_record_forms_nested_updates_preserve_the_payload_binder() {
        accepts("type NumberInner = {item: Int, count: Int}\ntype TextInner = {item: Str, count: Int}\ntype Number = {nested: NumberInner, other: Int}\ntype Text = {nested: TextInner, other: Str}\npure copy(value) { let row = {nested: {item: value, count: 0}, other: value}; {...row, nested.item: value, nested.count: 1} }", "let number: Number = copy(7)\nlet text: Text = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_typed_updates_preserve_the_existing_shape() {
        accepts("type Inner = {count: Int}\ntype Row = {nested: Inner}\npure keep(value, row: Row) { let updated = {...row, nested.count: 2}; let _ = updated; value }", "let number: Int = keep(7, {nested: {count: 0}})\nlet text: Str = keep(\"value\", {nested: {count: 1}})");
    }

    #[test]
    fn inferred_proc_returns_preserve_callable_value_contracts() {
        accepts(
            "pure increment(value: Int) -> Int { value + 1 }\nproc factory() { return increment }",
            "let operation = factory()?\nlet answer: Int = operation(41)",
        );
        accepts(
            "pure identity(value) { value }\nproc factory(flag) { if flag { return identity } return identity }",
            "let operation = factory(true)?\nlet integer: Int = operation(42)\nlet text: Str = operation(\"demo\")",
        );
        accepts(
            "type IntIdentity = pure(value: Int) -> Int\npure identity(value: Int) -> Int { value }\nproc factory() -> Result[IntIdentity] { return identity }",
            "let operation = factory()?\nlet answer: Int = operation(42)",
        );

        accepts(
            "proc null_return(value) { return null }",
            "let integer: Null = null_return(7)?\nlet text: Null = null_return(\"demo\")?",
        );

        let source = "var shared = {items: [0]}\nproc remember(value) { shared.items = shared.items.push(value); value }\nproc factory() { return remember }\nlet operation = factory()?\nlet integer = operation(42)\nlet text = operation(\"demo\")";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)
        }), "{source}\n{:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| {
            matches!(diagnostic.code, Some(crate::diagnostic::DiagnosticCode::CheckInferReturn | crate::diagnostic::DiagnosticCode::CheckUnresolvedCall))
        }), "{source}\n{:?}", checked.diagnostics);
    }

}

#[cfg(test)]
mod contextual_variant_body_tests {
    fn errors(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        checked.diagnostics.into_iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect()
    }

    fn accepts(source: &str) {
        let errors = errors(source);
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn typed_contextual_enum_variant_checks_its_nominal_domain() {
        accepts("enum State: Str { Observed = \"observed\", Missing = \"missing\" }\npure keep(value: Int, state: State) -> Int { if state == .Observed { value } else { value } }\nlet number: Int = keep(7, Observed)\n");
    }

    #[test]
    fn generic_contextual_enum_variant_preserves_its_selected_producer() {
        accepts("enum State: Str { Observed = \"observed\", Missing = \"missing\" }\npure keep(value, state: State) { if state == .Observed { value } else { value } }\nlet number: Int = keep(7, Observed)\nlet word: Str = keep(\"kept\", Missing)\n");
    }

    #[test]
    fn generic_body_reads_a_constant_payload_variant() {
        accepts("enum Payload { Held(Int) }\nconst held = Held(7)\npure keep(value) { {value, held} }\nlet row = keep(1)\n");
    }

    #[test]
    fn generic_named_enum_variant_retains_its_prepared_value() {
        accepts("enum State: Str { Observed = \"observed\", Missing = \"missing\" }\npure keep(value, state: State) { if state == Observed { value } else { value } }\nlet number: Int = keep(7, Observed)\nlet word: Str = keep(\"kept\", Missing)\n");
    }


    #[test]
    fn nullary_variant_support_rejects_payloads_and_unknown_variants() {
        for (source, code) in [
            ("enum State { Ready, Payload(Int) }\npure keep(value, state: State) { if state == .Payload { value } else { value } }\n", crate::diagnostic::DiagnosticCode::CheckArity),
            ("enum State { Ready }\npure keep(value, state: State) { if state == .Missing { value } else { value } }\n", crate::diagnostic::DiagnosticCode::CheckInferredVariant),
        ] {
            let errors = errors(source);
            assert!(errors.iter().any(|diagnostic| diagnostic.code == Some(code)), "{source}\n{errors:?}");
        }
    }

    #[test]
    fn imported_nullary_variants_preserve_same_spelling_nominal_identity() {
        let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        let root = tempfile::TempDir::new_in(scratch).unwrap();
        for module in ["left", "right"] {
            std::fs::write(root.path().join(format!("{module}.xsh")), "##! Nominal state fixture.\n## A state with two nullary choices.\nexport enum State: Str { Ready = \"ready\", Missing = \"missing\" }\n").unwrap();
        }
        for (tail, expected_error) in [
            ("let number: Int = keep(7, a.Ready)\nlet word: Str = keep(\"kept\", a.Missing)\ntest imported_variants { assert keep(7, a.Ready) == 7; assert keep(\"kept\", a.Missing) == \"kept\"; assert named(7, a.Ready) == 7; assert named(\"kept\", a.Missing) == \"kept\" }\n", None),
            ("let wrong = keep(7, b.Ready)\n", Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)),
        ] {
            let source = format!("use left as a\nuse right as b\npure keep(value, state: a.State) {{ if state == .Ready {{ value }} else {{ value }} }}\npure named(value, state: a.State) {{ if state == a.Ready {{ value }} else {{ value }} }}\n{tail}");
            let entry = root.path().join("entry.xsh");
            let file = entry.to_str().unwrap();
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(file, crate::loader::entry_source_from_text(file, source), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, parsed.arena.source_text_source_id().unwrap());
            let rejected = checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error);
            assert_eq!(!rejected, expected_error.is_none(), "{:?}", checked.diagnostics);
            if let Some(code) = expected_error {
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(code)), "{:?}", checked.diagnostics);
            }
            #[cfg(feature = "native-tests")]
            if expected_error.is_none() {
                let source_id = parsed.arena.source_text_source_id().unwrap();
                let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(std::sync::Arc::new(parsed.arena), source_id, checked).unwrap();
                for force_frames in [false, true] {
                    let result = prepared.eval_test("imported_variants", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
                    assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
                    assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
                    assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
                    assert!(matches!(&result.result, Some(crate::runtime::value::Value::Unit)) || matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "{:?}", result.result);
                }
            }
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn nullary_variant_native_controls_run_on_both_routes() {
        let source = include_str!("../../../tests/xsh/generic-nullary-variants.xsh");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-nullary-variants.xsh", crate::loader::entry_source_from_text("generic-nullary-variants.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(std::sync::Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_nullary_variants_preserve_nominal_values", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Unit)) || matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "{:?}", result.result);
        }
    }
}

#[cfg(test)]
mod structured_pipeline_body_tests {
    #[test]
    fn generic_pipeline_named_callback_keeps_owned_item_slot() {
        let source = "pure identity(value) { value }\npure named(value) { [value] |> map(identity) }\nlet number: List[Int] = named(7)\nlet word: List[Str] = named(\"kept\")\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena(&parsed.arena, source);
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn generic_pipeline_callbacks_preserve_item_and_capture_binders() {
        for source in [
            "pure mapped(value) { [value, value] |> map { |item| item } |> where { |item| true } |> flat-map { |item| [item] } }\nlet number: List[Int] = mapped(7)\nlet word: List[Str] = mapped(\"kept\")\n",
            "pure captured(value) { [1, 2] |> map { |item| value } }\nlet number: List[Int] = captured(7)\nlet word: List[Str] = captured(\"kept\")\n",
            "pure nested(value) { [value] |> map { |item| [item] |> map { |item| item } } }\nlet number: List[List[Int]] = nested(7)\nlet word: List[List[Str]] = nested(\"kept\")\n",
            "pure shorthand(value) { [{payload: value}] |> map .payload }\nlet number: List[Int] = shorthand(7)\nlet word: List[Str] = shorthand(\"kept\")\n",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_arena(&parsed.arena, source);
            let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
            assert!(errors.is_empty(), "{source}\n{errors:?}");
        }
    }

    #[test]
    fn generic_structured_pipeline_preserves_item_binders() {
        for source in [
            "pure keep(value) { [value, value] |> drop(1) }\nlet number: List[Int] = keep(7)\nlet word: List[Str] = keep(\"kept\")\n",
            "pure keep(value) { let rows = [1, 2] |> enumerate; {value, rows} }\nlet number = keep(7)\nlet word = keep(\"kept\")\n",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_arena(&parsed.arena, source);
            let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
            assert!(errors.is_empty(), "{source}\n{errors:?}");
        }
    }
}
