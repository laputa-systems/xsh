use super::*;
use crate::runtime::value::Value;
use crate::sema::operation_graph::PreparedLanguageOperation;

fn fixture() -> FullProgram {
    super::operation_prepare::tests::source_fixture(
        "pure pass(value) { value }\npure forwarded(left: Str, right: Str) -> Bool { pass(left == right) }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq },
    )
}

#[test]
fn original_language_comparison_result_passes_as_an_argument_on_both_routes_after_frontend_disposal() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture());
        program.symbol_owner().with_current(|| {
            let key = LoweredFunctionKey::Name(Name::intern("forwarded"));
            for recursive in [false, true] {
                for (right, expected) in [("same", true), ("other", false)] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let arguments = [Value::Str(Arc::from("same")), Value::Str(Arc::from(right))];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    let observed = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
                    assert_eq!(observed, Value::Bool(expected));
                }
            }
        });
    });
}

#[test]
fn original_language_comparison_argument_refuses_missing_foreign_and_rewritten_result_authority() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let foreign = fixture();
        let generic = program.generic_evidence().unwrap();
        let (id, operation) = generic.operations().find(|(_, operation)| matches!(operation.authority,
            super::super::generic::PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Equality { .. }, .. })).unwrap();
        let mut missing = program.clone();
        missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
        assert!(FullVerifier::verify(&missing).is_err());
        let mut replaced = program.clone();
        replaced.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().next().unwrap().1.source;
        assert!(FullVerifier::verify(&replaced).is_err());
        let mut changed = program.clone();
        changed.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().result = operation.arguments[0].unwrap();
        assert!(FullVerifier::verify(&changed).is_err());
    });
}

#[test]
fn prepared_program_refuses_missing_and_foreign_complete_evidence_before_execution() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure parsed(text: Str) -> Result[Int] { text.parse_int() }\npure compared(left: Str, right: Str) -> Bool { left == right }\n";
        let program = super::operation_prepare::tests::source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let foreign = super::operation_prepare::tests::source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let mut missing = program.clone();
        missing.store.generic = None;
        assert!(FullVerifier::verify(&missing).is_err(), "a prepared native operation cannot lose the complete evidence owner");
        assert!(missing.function_view_at(0).unwrap().execution().is_err(), "worker entry rejects missing evidence before native execution");
        let mut replaced = program.clone();
        replaced.store.generic = foreign.store.generic.clone();
        assert!(FullVerifier::verify(&replaced).is_err(), "a whole foreign store cannot replace the prepared program's original evidence");
        assert!(replaced.function_view_at(0).unwrap().execution().is_err());
    });
}
