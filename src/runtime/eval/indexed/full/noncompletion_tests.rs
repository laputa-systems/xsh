use super::*;
use crate::sema::check::Checker;
use std::path::PathBuf;

fn checked_program(name: &str, source: &str) -> FullProgram {
    let module_roots = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core")];
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        name,
        crate::loader::entry_source_from_text(name, source.to_owned()),
        module_roots,
    );
    let source_id = sources
        .files()
        .first()
        .map(crate::source::SourceFile::id)
        .expect("entry source is present");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena_with_sources_and_options(
        &parsed.arena,
        &sources,
        source_id,
        crate::sema::check::CheckOptions {
            reveal_types: true,
            embedded_bodies: true,
            ..crate::sema::check::CheckOptions::default()
        },
    );
    let declarations = Checker::compact_declarations(&parsed.arena, checked);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    FullBuilder::build_compact(
        &parsed.arena,
        &declarations,
        source,
        Arc::new(sources),
        source_id,
    )
    .expect("source should pass full IR verification")
}

fn function_index(program: &FullProgram, name: &str) -> usize {
    program
        .store
        .functions
        .iter()
        .position(|function| program.store.string(function.name).unwrap_or("") == name)
        .expect("fixture function is present")
}

#[test]
fn text_a2_import_verifies_discarded_collect_body() {
    checked_program("text-a2-discarded-collect.xsh", "use lib.text_a2\n");
}

#[test]
fn completed_tail_less_value_block_has_unit_contract() {
    let program = checked_program(
        "collect-unit-contract.xsh",
        "proc collected() -> List[Int] { collect { yield 1 } }\n",
    );
    let owner = function_index(&program, "collected");
    let decoder = function_view_decoder(&program.store, owner);
    let block = decoder
        .instruction_range
        .clone()
        .find(|&instruction| {
            if program.store.tags[instruction] != FullTag::ExprValueBlock {
                return false;
            }
            let Ok(schema::ExprView::ValueBlock { mut body, .. }) =
                decoder.expression_view(instruction)
            else {
                return false;
            };
            decoder
                .derive_checked_block_tail_expression(&mut body)
                .is_ok_and(|tail| tail.is_none())
        })
        .expect("collect body has a tail-less value block");
    assert_eq!(
        decoder.derive_checked_expression_type(block).unwrap(),
        Some(Type::Unit)
    );
    let check_expected = |expected: &Type| {
        let schema::ExprView::ValueBlock { body, .. } =
            decoder.expression_view(block).unwrap()
        else {
            unreachable!()
        };
        decoder.checked_block_tail_matches_type(body, expected)
    };
    check_expected(&Type::Any).unwrap();
    assert!(check_expected(&Type::Str).is_err());
}

#[test]
fn tail_less_unit_value_block_fits_any() {
    let program = checked_program(
        "unit-value-block-as-any.xsh",
        "pure choose_unit() -> Any { return { let first = 1 } }\n",
    );
    let owner = function_index(&program, "choose_unit");
    let decoder = function_view_decoder(&program.store, owner);
    let block = decoder
        .instruction_range
        .clone()
        .find(|&instruction| {
            if program.store.tags[instruction] != FullTag::ExprValueBlock {
                return false;
            }
            let Ok(schema::ExprView::ValueBlock { mut body, .. }) =
                decoder.expression_view(instruction)
            else {
                return false;
            };
            decoder
                .derive_checked_block_tail_expression(&mut body)
                .is_ok_and(|tail| tail.is_none())
        })
        .expect("explicit return contains a tail-less value block");
    assert_eq!(
        decoder.derive_checked_expression_type(block).unwrap(),
        Some(Type::Unit)
    );
}

#[test]
fn unresolved_value_block_tail_does_not_become_unit() {
    let program = checked_program(
        "unknown-value-block-tail.xsh",
        "proc invoke(callback: Proc) -> Result[Any] { { callback.call() } }\n",
    );
    let owner = function_index(&program, "invoke");
    let decoder = function_view_decoder(&program.store, owner);
    let block = decoder
        .instruction_range
        .clone()
        .find(|&instruction| {
            if program.store.tags[instruction] != FullTag::ExprValueBlock {
                return false;
            }
            let Ok(schema::ExprView::ValueBlock { mut body, .. }) =
                decoder.expression_view(instruction)
            else {
                return false;
            };
            decoder
                .derive_checked_block_tail_expression(&mut body)
                .is_ok_and(|tail| tail.is_some())
        })
        .expect("fixture has a value block with a tail");
    assert_eq!(decoder.derive_checked_expression_type(block).unwrap(), None);
}

#[test]
fn exit_status_is_int_and_exit_requires_process_effect() {
    let program = checked_program(
        "exit-status-contract.xsh",
        "proc terminate() [process] -> Unit { let message = \"bad\"; exit 1 }\n",
    );
    let owner = function_index(&program, "terminate");
    let decoder = function_view_decoder(&program.store, owner);
    let execution = FullExecution::shared_function(Arc::new(program.clone()), owner).unwrap();
    let body = IrBlockId::from_raw(program.store.functions[owner].body).unwrap();
    let mut statements = execution.statement_block_view_id(body).unwrap();
    let execution_decoder = execution.decoder();
    assert!(matches!(
        execution_decoder
            .checked_value_block_result(&mut statements)
            .unwrap(),
        CheckedValueBlockResult::DoesNotComplete
    ));
    let abort = decoder
        .instruction_range
        .clone()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprAbort)
        .expect("exit lowers to an abort expression");
    assert!(!indexed_expr_can_complete(&decoder, abort, 0).unwrap());
    let schema::ExprView::Abort { status, .. } = decoder.expression_view(abort).unwrap() else {
        unreachable!()
    };
    let status = status.raw() as usize;
    let string = decoder
        .instruction_range
        .clone()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr)
        .expect("fixture has a string expression");
    let payload = program.store.payload(program.store.data[string].range()).unwrap().to_vec();
    let mut wrong_status = program.clone();
    let start = wrong_status.store.extra.len() as u32;
    wrong_status.store.extra.extend(payload);
    wrong_status.store.tags[status] = FullTag::ExprStr;
    wrong_status.store.data[status] = IrData::from_range(IrRange::new(start, 1));
    assert!(FullVerifier::verify(&wrong_status)
        .unwrap_err()
        .message
        .contains("ground producer value disagrees with its checked contract"));

    let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else {
        unreachable!()
    };
    let mut missing_process = program.clone();
    let mut callable = missing_process
        .store
        .semantic
        .to_signature_with_budget(
            signature,
            0,
            &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default(),
        )
        .unwrap();
    callable.effects = Some(Vec::new());
    let mut semantic = SemanticPoolBuilder::default();
    missing_process.store.functions[owner].contract = FullCallableContract::Ground(
        semantic
            .intern_signature(&mut missing_process.store.semantic, &callable)
            .unwrap(),
    );
    assert!(FullVerifier::verify(&missing_process)
        .unwrap_err()
        .message
        .contains("requires effect process"));
}
