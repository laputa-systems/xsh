use super::{CallBinding, Checker, SolvedOperation, Type};
use crate::sema::inference::{EffectSet, EffectSummary, InferenceError, OperationCall};
use crate::syntax::arena::{ArenaProgram, ExprId};
use crate::syntax::node::{BinaryOp, UnaryOp};

#[derive(Clone, Copy)]
pub(super) enum LanguageOperator { Binary(BinaryOp), Unary(UnaryOp), Index { field: Option<crate::symbol::Name> }, Slice { bounds: [bool; 2] }, ErrorField { receiver: crate::sema::inference::Atom, field: crate::symbol::Name } }

impl Checker {
    pub(super) fn check_graph_language_operation(&mut self, arena: &ArenaProgram, expression: ExprId, operator: LanguageOperator, operands: &[Type], expected: Option<&Type>) -> Type {
        let identity = self.expression_identity(arena, expression);
        let span = arena.arena.expr(expression).span;
        if !self.graph_generation || self.generic.borrow().facts.operations.contains_key(&identity) {
            return self.generic.borrow().facts.operations.get(&identity).map(|operation| self.graph_view(operation.result)).unwrap_or(Type::Invalid);
        }
        let outcome = (|| {
            let actuals = operands.iter().map(|operand| self.graph_type(operand, span)).collect::<Result<Vec<_>, _>>()?;
            let expected = expected.map(|expected| self.graph_type(expected, span)).transpose()?;
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let mut state = self.generic.borrow_mut();
            let family = {
                let super::generic::GenericState { facts, language_operations, .. } = &mut *state;
                match operator {
                    LanguageOperator::Binary(operator) => language_operations.binary_family(&mut facts.graph, operator, span)?,
                    LanguageOperator::Unary(operator) => language_operations.unary_family(&mut facts.graph, operator, span)?,
                    LanguageOperator::Index { field: Some(field) } => language_operations.constant_key_index_family(&mut facts.graph, field, span)?,
                    LanguageOperator::Index { field: None } => language_operations.index_family(&mut facts.graph, span)?,
                    LanguageOperator::Slice { .. } => language_operations.all_slice_family(&mut facts.graph, span)?,
                    LanguageOperator::ErrorField { receiver, field } => language_operations.error_field_family(&mut facts.graph, receiver, field)?,
                }
            };
            let (receiver, arguments) = if matches!(operator, LanguageOperator::Binary(BinaryOp::In | BinaryOp::NotIn)) {
                (Some(actuals[1]), vec![actuals[0]])
            } else { (None, actuals) };
            let (supplied, supplied_slots, default_slots) = if let LanguageOperator::Slice { bounds } = operator {
                let mut supplied = vec![Some(arguments[0]), None, None];
                let mut supplied_slots = vec![0];
                let mut default_slots = Vec::new();
                let mut actual = arguments.iter().copied().skip(1);
                for (index, present) in bounds.into_iter().enumerate() {
                    if present { supplied[index + 1] = actual.next(); supplied_slots.push(index + 1); }
                    else { default_slots.push(index + 1); }
                }
                (supplied, supplied_slots, default_slots)
            } else {
                (arguments.iter().copied().map(Some).collect(), (0..arguments.len()).collect(), Vec::new())
            };
            let graph = &mut state.facts.graph;
            let result = graph.fresh(level, span)?;
            let effects = EffectSummary::Closed(EffectSet::EMPTY);
            let why = graph.reason(span, None)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver, arguments: supplied, result, effects, effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, why)?;
            graph.solve()?;
            if let Some(expected) = expected { graph.assignable(expected, result, why)?; graph.solve()?; }
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
            state.facts.operations.insert(identity, SolvedOperation { requirement, result, effects, receiver, binding: CallBinding { supplied_slots, default_slots, rest_slot: None, dynamic: None }, argument_coercions: Vec::new(), actual_arguments: arguments, caller: self.current_generic });
            state.facts.expressions.insert(identity, result);
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
            drop(state);
            Ok(self.graph_view(result))
        })();
        match outcome { Ok(result) => result, Err(error) => { self.graph_error(span, error); Type::Invalid } }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn selected_language_operations(output: &super::super::CheckOutput) -> Vec<crate::sema::operation_graph::OperationCandidate> {
        output.solved.calls.values().flat_map(|call| &call.requirements).filter_map(|requirement| {
            let evidence = output.solved.graph.candidate_evidence(*requirement).unwrap()?;
            match output.solved.operation_catalog.candidate(&output.solved.graph, evidence.candidate).unwrap() {
                super::super::operation_catalog::SolvedOperationAuthority::Language(metadata) => Some(metadata.clone()),
                _ => None,
            }
        }).collect()
    }

    #[test]
    fn generalized_constant_key_indexing_retains_record_and_map_relationships() {
        let declarations = "pure named(row) { row[\"name\"] }\npure forwarded(row) { named(row) }\n";
        for calls in [
            "let integer: Int = forwarded({name: 7, extra: false})\nlet text: Str = forwarded({name: \"seven\"})\n",
            "let text: Str = forwarded({name: \"seven\"})\nlet integer: Int = forwarded({name: 7, extra: false})\n",
            "let labels: Map[Str, Int] = {\"name\": 9}\nlet value: Int = forwarded(labels)\n",
        ] {
            let source = format!("{declarations}{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let output = Checker::check_arena(&parsed.arena, &source);
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            assert_eq!(output.solved.operations.len(), 1);
            drop(parsed);
            output.solved.validate().unwrap();
        }
        let source = format!("{declarations}let invalid: Int = forwarded({{other: 7}})\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let output = Checker::check_arena(&parsed.arena, &source);
        assert!(!output.diagnostics.is_empty());
    }

    #[test]
    fn constant_key_indexing_preserves_callables_and_exact_producer_permissions() {
        let source = "pure select(row) { row[\"worker\"] }\npure plus(value: Int) -> Int { value + 1 }\nlet selected = select({worker: plus})\nlet value: Int = selected(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let output = Checker::check_arena(&parsed.arena, source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        drop(parsed);
        output.solved.validate().unwrap();
        let prefix = "stream delayed() [time] -> Stream[Int] { let _ = time.now(); yield 7 }\npure select(row) { row[\"rows\"] }\nlet rows: Stream[Int] = select({rows: delayed()})\n";
        for (effects, valid) in [("time, error", true), ("error", false)] {
            let source = format!("{prefix}proc consumed() [{effects}] -> List[Int] {{ rows.collect() }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let output = Checker::check_arena(&parsed.arena, &source);
            if valid { assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics); }
            else { assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", output.diagnostics); }
        }
    }

    #[test]
    fn generalized_arithmetic_covers_every_finite_domain_and_call_order() {
        let cases = [
            ("+", "Int", "7", "Int", "2", "Int"),
            ("+", "UInt", "7", "UInt", "2", "Int"),
            ("+", "UInt", "7", "Int", "2", "Int"),
            ("+", "Int", "7", "UInt", "2", "Int"),
            ("+", "Float", "7.0", "Float", "2.0", "Float"),
            ("+", "Str", "\"a\"", "Str", "\"b\"", "Str"),
            ("+", "List[Int]", "[1]", "List[Int]", "[2]", "List[Int]"),
            ("+", "Duration", "7s", "Duration", "2s", "Duration"),
            ("-", "Int", "7", "Int", "2", "Int"),
            ("-", "UInt", "7", "UInt", "2", "Int"),
            ("-", "UInt", "7", "Int", "2", "Int"),
            ("-", "Int", "7", "UInt", "2", "Int"),
            ("-", "Float", "7.0", "Float", "2.0", "Float"),
            ("-", "Duration", "7s", "Duration", "2s", "Duration"),
            ("*", "Int", "7", "Int", "2", "Int"),
            ("*", "UInt", "7", "UInt", "2", "Int"),
            ("*", "UInt", "7", "Int", "2", "Int"),
            ("*", "Int", "7", "UInt", "2", "Int"),
            ("*", "Float", "7.0", "Float", "2.0", "Float"),
            ("*", "Duration", "7s", "Int", "2", "Duration"),
            ("*", "Int", "2", "Duration", "7s", "Duration"),
            ("/", "Int", "7", "Int", "2", "Int"),
            ("/", "UInt", "7", "UInt", "2", "Int"),
            ("/", "UInt", "7", "Int", "2", "Int"),
            ("/", "Int", "7", "UInt", "2", "Int"),
            ("/", "Float", "7.0", "Float", "2.0", "Float"),
            ("/", "Duration", "7s", "Int", "2", "Duration"),
            ("/", "Duration", "7s", "Duration", "2s", "Int"),
            ("%", "Int", "7", "Int", "2", "Int"),
            ("%", "UInt", "7", "UInt", "2", "Int"),
            ("%", "UInt", "7", "Int", "2", "Int"),
            ("%", "Int", "7", "UInt", "2", "Int"),
        ];
        for operator in ["+", "-", "*", "/", "%"] {
            for reverse in [false, true] {
                let mut selected = cases.iter().filter(|case| case.0 == operator).collect::<Vec<_>>();
                if reverse { selected.reverse(); }
                let mut source = format!("pure operate(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ operate(left, right) }}\n");
                for (index, (_, left, lhs, right, rhs, result)) in selected.iter().enumerate() {
                    source.push_str(&format!("let left_{index}: {left} = {lhs}\nlet right_{index}: {right} = {rhs}\nlet direct_{index}: {result} = operate(left_{index}, right_{index})\nlet forwarded_{index}: {result} = forwarded(left_{index}, right_{index})\n"));
                }
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{operator}: {:?}", parsed.diagnostics);
                let output = Checker::check_arena(&parsed.arena, &source);
                assert!(output.diagnostics.is_empty(), "{operator}: {:?}", output.diagnostics);
                assert!(output.solved.declarations.values().all(|declaration| !output.solved.graph.scheme(declaration.scheme).unwrap().requirements.is_empty()));
                if operator == "+" {
                    let discharges = output.solved.calls.values().flat_map(|call| &call.requirements)
                        .filter_map(|requirement| output.solved.graph.discharge(*requirement).unwrap())
                        .map(|evidence| format!("{:?}", evidence.operation)).collect::<std::collections::BTreeSet<_>>();
                    assert_eq!(discharges, ["AddInt", "AddFloat", "AddStr", "AddList", "AddDuration"].into_iter().map(str::to_owned).collect());
                } else {
                    let selected = selected_language_operations(&output);
                    let authorities = selected.iter().map(|metadata| metadata.authority).collect::<std::collections::BTreeSet<_>>();
                    let expected: &[&str] = match operator {
                        "-" => &["language.binary.Sub.integer", "language.binary.Sub.float", "language.binary.Sub.duration"],
                        "*" => &["language.binary.Mul.integer", "language.binary.Mul.float", "language.binary.Mul.duration_scale", "language.binary.Mul.duration_scale_reverse"],
                        "/" => &["language.binary.Div.integer", "language.binary.Div.float", "language.binary.Div.duration_scale", "language.binary.Div.duration_ratio"],
                        "%" => &["language.binary.Rem.integer"],
                        _ => unreachable!(),
                    };
                    assert_eq!(authorities, expected.iter().copied().collect());
                    let integer_domains = selected.iter().filter_map(|metadata| match metadata.operation {
                        crate::sema::operation_graph::PreparedLanguageOperation::Arithmetic { domain: crate::sema::operation_graph::ArithmeticDomain::Integer { left, right }, .. } => Some(format!("{left:?}/{right:?}")),
                        _ => None,
                    }).collect::<std::collections::BTreeSet<_>>();
                    assert_eq!(integer_domains, ["Int/Int", "UInt/UInt", "UInt/Int", "Int/UInt"].into_iter().map(str::to_owned).collect());
                }
                drop(parsed);
                output.solved.validate().unwrap();
            }
            let source = format!("pure operate(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ operate(left, right) }}\nlet invalid = forwarded(true, false)\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "{operator}");
        }
    }

    #[test]
    fn generalized_ordering_covers_all_operators_and_scalar_domains() {
        for operator in ["<", "<=", ">", ">="] {
            for reverse in [false, true] {
                let mut domains = [("Int", "1"), ("UInt", "1"), ("Float", "1.0"), ("Duration", "1s"), ("Str", "\"a\"")];
                if reverse { domains.reverse(); }
                let mut source = format!("pure ordered(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ ordered(left, right) }}\n");
                for (index, (ty, value)) in domains.iter().enumerate() {
                    source.push_str(&format!("let value_{index}: {ty} = {value}\nlet direct_{index}: Bool = ordered(value_{index}, value_{index})\nlet forwarded_{index}: Bool = forwarded(value_{index}, value_{index})\n"));
                }
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let output = Checker::check_arena(&parsed.arena, &source);
                assert!(output.diagnostics.is_empty(), "{operator}: {:?}", output.diagnostics);
                let selected = selected_language_operations(&output);
                let ordered_domains = selected.iter().filter_map(|metadata| match metadata.operation {
                    crate::sema::operation_graph::PreparedLanguageOperation::Ordering { left, right, .. } if left == right => Some(format!("{left:?}")),
                    _ => None,
                }).collect::<std::collections::BTreeSet<_>>();
                assert_eq!(ordered_domains, ["Int", "UInt", "Float", "Duration", "Str"].into_iter().map(str::to_owned).collect());
                drop(parsed);
                output.solved.validate().unwrap();
            }
            for arguments in ["true, false", "1, \"a\"", "1, 1.0"] {
                let source = format!("pure ordered(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ ordered(left, right) }}\nlet invalid = forwarded({arguments})\n");
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "{operator} {arguments}");
            }
        }
    }

    #[test]
    fn generalized_arithmetic_uses_definition_owned_sealed_operation_requirements() {
        for operator in ["-", "*", "/"] {
            for calls in ["let integer: Int = forwarded(12, 3)\nlet floating: Float = forwarded(12.0, 3.0)\n", "let floating: Float = forwarded(12.0, 3.0)\nlet integer: Int = forwarded(12, 3)\n"] {
                let source = format!("pure operate(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ operate(left, right) }}\n{calls}");
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{operator}: {:?}", checked.diagnostics);
                assert_eq!(checked.solved.operations.len(), 1);
                assert!(checked.solved.declarations.values().all(|declaration| !checked.solved.graph.scheme(declaration.scheme).unwrap().requirements.is_empty()));
                checked.solved.validate().unwrap();
            }
        }
    }

    #[test]
    fn generalized_membership_covers_every_receiver_and_map_key_domain() {
        for operator in ["in", "not in"] {
            let declarations = format!("pure member(needle, container) {{ needle {operator} container }}\npure forwarded(needle, container) {{ member(needle, container) }}\n");
            for reverse in [false, true] {
                let mut calls = vec![
                    "forwarded(1, [1, 2])".to_owned(),
                    "forwarded(\"a\", \"abc\")".to_owned(),
                    "forwarded(b\"a\", b\"abc\")".to_owned(),
                    "forwarded(\"name\", {name: 1})".to_owned(),
                    "forwarded(\"root\", Path(\"root\"))".to_owned(),
                    "forwarded(Path(\"root\"), Path(\"root\"))".to_owned(),
                ];
                let keys = [("Str", "\"key\""), ("Int", "1"), ("UInt", "1"), ("Bool", "true"), ("Bytes", "b\"key\""), ("Path", "Path(\"key\")"), ("Duration", "1s")];
                let mut source = declarations.clone();
                for (index, (ty, value)) in keys.iter().enumerate() {
                    source.push_str(&format!("let key_{index}: {ty} = {value}\nlet map_{index}: Map[{ty}, Int] = {{[key_{index}]: 1}}\n"));
                    calls.push(format!("forwarded(key_{index}, map_{index})"));
                }
                if reverse { calls.reverse(); }
                for (index, call) in calls.iter().enumerate() { source.push_str(&format!("let member_{index}: Bool = {call}\n")); }
                source.push_str("proc environment_member() [env, error] -> Bool { let paths: EnvPathList = env.PATH; forwarded(Path(\"root\"), paths) }\n");
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let output = Checker::check_arena(&parsed.arena, &source);
                assert!(output.diagnostics.is_empty(), "{operator}: {:?}", output.diagnostics);
                let selected = selected_language_operations(&output);
                let authorities = selected.iter().filter(|metadata| matches!(metadata.operation, crate::sema::operation_graph::PreparedLanguageOperation::Membership { .. })).map(|metadata| metadata.authority).collect::<std::collections::BTreeSet<_>>();
                let name = if operator == "in" { "In" } else { "NotIn" };
                let expected = ["List", "Map", "Str", "Bytes", "Record", "Path", "EnvPathList"].into_iter().map(|domain| format!("language.binary.{name}.{domain}")).collect::<std::collections::BTreeSet<_>>();
                assert_eq!(authorities.into_iter().map(str::to_owned).collect::<std::collections::BTreeSet<_>>(), expected);
                drop(parsed);
                output.solved.validate().unwrap();
            }
            for invalid in ["true, [1]", "1, \"abc\"", "\"a\", b\"abc\"", "1, {name: 1}", "true, Path(\"root\")", "1.0, {[1]: true}"] {
                let source = format!("{declarations}let invalid: Bool = forwarded({invalid})\n");
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "{operator} {invalid}");
            }
            let source = format!("{declarations}proc invalid_environment_member() [env, error] -> Bool {{ forwarded(\"root\", env.PATH) }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty());
        }
    }

    #[test]
    fn generalized_membership_forwards_each_container_domain_and_rejects_mismatched_items() {
        let declarations = "pure member(needle, container) { needle in container }\npure forwarded(needle, container) { member(needle, container) }\n";
        for (calls, valid) in [("let integers: Bool = forwarded(1, [1, 2])\nlet text: Bool = forwarded(\"w\", \"word\")\n", true), ("let invalid: Bool = forwarded(true, [1, 2])\n", false)] {
            let source = format!("{declarations}{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if valid { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); checked.solved.validate().unwrap(); }
            else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics); }
        }
    }

    #[test]
    fn generalized_map_construction_retains_all_key_guards_and_value_relationships() {
        for reverse in [false, true] {
            let mut keys = [("Str", "\"key\""), ("Int", "1"), ("UInt", "1"), ("Bool", "true"), ("Bytes", "b\"key\""), ("Path", "Path(\"key\")"), ("Duration", "1s")];
            if reverse { keys.reverse(); }
            let mut source = "pure keyed(key, value) { {[key]: value} }\npure forwarded(key, value) { keyed(key, value) }\n".to_owned();
            for (index, (ty, key)) in keys.iter().enumerate() {
                source.push_str(&format!("let key_{index}: {ty} = {key}\nlet value_{index}: Map[{ty}, Int] = forwarded(key_{index}, 1)\nlet word_{index}: Map[{ty}, Str] = forwarded(key_{index}, \"value\")\n"));
            }
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let output = Checker::check_arena(&parsed.arena, &source);
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            assert!(output.solved.declarations.values().all(|declaration| output.solved.graph.scheme(declaration.scheme).unwrap().requirements.iter().any(|requirement| matches!(requirement, crate::sema::inference::RequirementTemplate::Eligibility { predicate: crate::sema::inference::Eligibility::MapKey, .. }))));
            drop(parsed);
            output.solved.validate().unwrap();
        }
        for invalid in ["1.0", "[1]", "{name: 1}", "null"] {
            let source = format!("pure keyed(key, value) {{ {{[key]: value}} }}\npure forwarded(key, value) {{ keyed(key, value) }}\nlet invalid = forwarded({invalid}, 1)\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "{invalid}");
        }
    }

    #[test]
    fn generalized_boolean_negation_preserves_bool_and_status_domains() {
        let source = "pure invert(value) { !value }\npure forwarded(value) { invert(value) }\npure status_value(value: Status) -> Bool { forwarded(value) }\nlet value: Bool = forwarded(false)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let output = Checker::check_arena(&parsed.arena, source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let domains = selected_language_operations(&output).into_iter().filter_map(|metadata| match metadata.operation {
            crate::sema::operation_graph::PreparedLanguageOperation::Unary { operand, .. } => Some(format!("{operand:?}")), _ => None,
        }).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(domains, ["Bool", "Status"].into_iter().map(str::to_owned).collect());
        drop(parsed);
        output.solved.validate().unwrap();
        let source = "pure invert(value) { !value }\npure forwarded(value) { invert(value) }\nlet invalid = forwarded(1)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, source).diagnostics.is_empty());
    }

    #[test]
    fn generalized_negation_preserves_distinct_integer_and_float_instances() {
        let source = "pure negate(value) { -value }\nlet integer: Int = negate(1)\nlet floating: Float = negate(1.0)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 1);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn generalized_index_and_slice_keep_element_relationships_and_domain_requirements() {
        for calls in [
            "let list: Int = forwarded([1, 2], 0)\nlet mapped: Str = forwarded({[\"name\"]: \"value\"}, \"name\")\nlet numbers: List[Int] = portion([1, 2])\nlet text: Str = portion(\"word\")\nlet data: Bytes = portion(b\"word\")\n",
            "let data: Bytes = portion(b\"word\")\nlet text: Str = portion(\"word\")\nlet numbers: List[Int] = portion([1, 2])\nlet mapped: Str = forwarded({[\"name\"]: \"value\"}, \"name\")\nlet list: Int = forwarded([1, 2], 0)\n",
        ] {
            let source = format!("pure indexed(values, key) {{ values[key] }}\npure forwarded(values, key) {{ indexed(values, key) }}\npure portion(values) {{ values[..] }}\n{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.operations.len(), 2);
            checked.solved.validate().unwrap();
        }
        for source in [
            "pure indexed(values, key) { values[key] }\nlet bad = indexed([1], false)\n",
            "pure portion(values) { values[..] }\nlet bad = portion(1)\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn comparison_chains_and_fallbacks_retain_definition_owned_operation_facts() {
        let source = "pure ordered(first, middle, last) { first < middle <= last }\npure forwarded(first, middle, last) { ordered(first, middle, last) }\npure choose(value, fallback) { value ?? fallback }\nlet integers: Bool = forwarded(1, 2, 3)\nlet words: Bool = forwarded(\"a\", \"b\", \"c\")\nlet optional_number: Int? = 1\nlet optional_word: Str? = \"word\"\nlet success: Result[Int] = Ok(3)\nlet number: Int = choose(optional_number, 2)\nlet word: Str = choose(optional_word, \"fallback\")\nlet successful: Int = choose(success, 4)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let language_operations = checked.solved.operations.values().filter(|operation| operation.caller.is_some()).count();
        assert_eq!(language_operations, 3);
        checked.solved.validate().unwrap();
    }
}
