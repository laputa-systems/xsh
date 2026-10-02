use super::*;
use crate::sema::check::Checker;
use crate::source::SourceId;

const SOURCE: &str = "proc quoted(first: List[Str], second: List[Str], outside: Str) [io] {\n for left in first { print ${shlex.quote(left)} }\n for right in second { print ${shlex.quote(right)} }\n print ${shlex.quote(outside)}\n}\nquoted([\"one\"], [\"two\"], \"outside\")\n";

fn fixture() -> FullProgram {
    fixture_source(SOURCE, 2)
}

fn fixture_source(source: &str, bindings: usize) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "iteration-proof.xsh", crate::loader::entry_source_from_text("iteration-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
    drop(parsed); drop(checked); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none(), "iteration evidence retains immutable receipts without retaining the solved graph");
    FullVerifier::verify(&program).unwrap();
    assert_eq!(program.generic_evidence().unwrap().iteration_bindings().count(), bindings);
    assert_eq!(program.generic_evidence().unwrap().iteration_uses().count(), bindings);
    program
}

fn change_word(program: &mut FullProgram, instruction: u32, word: usize, value: u32) {
    let range = program.store.data[instruction as usize].range().bounds(program.store.extra.len()).unwrap();
    assert!(word < range.len());
    program.store.extra[range.start + word] = value;
}

#[test]
fn cold_iteration_bindings_reject_coupled_same_storage_loop_and_read_rewrites() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let _symbols = program.symbol_owner().enter();
        let generic = program.generic_evidence().unwrap();
        let bindings = generic.iteration_bindings().map(|(id, binding)| (id, binding.clone())).collect::<Vec<_>>();
        let (left_id, left) = &bindings[0];
        let (right_id, right) = &bindings[1];
        assert_eq!(left.item, right.item);
        assert_eq!(left.owner, right.owner);
        assert_ne!(left.slot, right.slot);
        let read = generic.iteration_uses().find(|use_| use_.binding == *left_id).unwrap().clone();

        let mut redirected = program.clone();
        change_word(&mut redirected, read.instruction, 0, right.slot);
        redirected.store.generic.as_mut().unwrap().test_iteration_use_mut(read.instruction).unwrap().binding = *right_id;
        let error = FullVerifier::verify(&redirected).unwrap_err();
        assert!(error.message.contains("original receipt"), "a same-storage read cannot acquire a sibling loop's binding: {}", error.message);

        let mut rewritten = program.clone();
        change_word(&mut rewritten, left.instruction, 0, right.slot);
        change_word(&mut rewritten, read.instruction, 0, right.slot);
        rewritten.store.generic.as_mut().unwrap().test_iteration_binding_mut(*left_id).unwrap().slot = right.slot;
        let error = FullVerifier::verify(&rewritten).unwrap_err();
        assert!(error.message.contains("original receipt"), "rewriting both physical words cannot change the original slot allocation: {}", error.message);
    });
}

#[test]
fn cold_iteration_bindings_reject_changed_source_owner_domain_and_missing_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let _symbols = program.symbol_owner().enter();
        let (id, binding) = program.generic_evidence().unwrap().iteration_bindings().next().unwrap();
        let read = program.generic_evidence().unwrap().iteration_uses().find(|use_| use_.binding == id).unwrap();
        for mutation in 0..4 {
            let mut changed = program.clone();
            let binding = changed.store.generic.as_mut().unwrap().test_iteration_binding_mut(id).unwrap();
            match mutation {
                0 => binding.statement.source = SourceId::new(123),
                1 => binding.owner = InstructionOwner::Driver(0),
                2 => {
                    let PreparedOperationAuthority::Language { operation, .. } = &mut binding.authority else { unreachable!() };
                    *operation = PreparedLanguageOperation::Iteration { domain: IterableDomain::Str, outer_result: false };
                }
                3 => binding.iterator_origin.source = SourceId::new(123),
                _ => unreachable!(),
            }
            assert!(FullVerifier::verify(&changed).is_err(), "original iteration source mutation {mutation} must reject");
        }
        let mut missing_binding = program.clone();
        missing_binding.store.generic.as_mut().unwrap().test_remove_iteration_bindings();
        assert!(FullVerifier::verify(&missing_binding).is_err(), "private original bindings cannot be replaced by an empty visible ledger");
        let mut missing_read = program.clone();
        missing_read.store.generic.as_mut().unwrap().test_remove_iteration_uses();
        assert!(FullVerifier::verify(&missing_read).is_err(), "native operands cannot lose their original use ledger");
        let mut foreign_read = program.clone();
        foreign_read.store.generic.as_mut().unwrap().test_iteration_use_mut(read.instruction).unwrap().owner = InstructionOwner::Driver(0);
        assert!(FullVerifier::verify(&foreign_read).is_err());
        assert_eq!(binding.binding.source, binding.statement.source);
    });
}

#[test]
fn cold_iteration_bindings_reject_changed_iterator_slot_body_and_producer() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let _symbols = program.symbol_owner().enter();
        let bindings = program.generic_evidence().unwrap().iteration_bindings().map(|(_, binding)| binding.clone()).collect::<Vec<_>>();
        let left = &bindings[0]; let right = &bindings[1];
        for (word, replacement) in [(0, right.slot), (1, right.iterator), (2, right.body.raw())] {
            let mut changed = program.clone();
            change_word(&mut changed, left.instruction, word, replacement);
            assert!(FullVerifier::verify(&changed).is_err(), "the actual For operand {word} must match its original receipt");
        }
        let mut wrong_producer = program.clone();
        change_word(&mut wrong_producer, left.iterator, 0, left.slot);
        assert!(FullVerifier::verify(&wrong_producer).is_err(), "the item cannot replace the original List[Str] iterator producer");
    });
}

#[test]
fn cold_iteration_iterator_keeps_its_original_same_typed_parameter_source() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let _symbols = program.symbol_owner().enter();
        let bindings = program.generic_evidence().unwrap().iteration_bindings().map(|(_, binding)| binding.clone()).collect::<Vec<_>>();
        let left = &bindings[0]; let right = &bindings[1];
        assert_eq!(left.input, right.input);
        assert_ne!(left.iterator_origin, right.iterator_origin);
        assert_eq!(program.store.tags[left.iterator as usize], FullTag::ExprParam);
        assert_eq!(program.store.tags[right.iterator as usize], FullTag::ExprParam);
        let alternative_slot = program.store.payload(program.store.data[right.iterator as usize].range()).unwrap()[0];
        let original_slot = program.store.payload(program.store.data[left.iterator as usize].range()).unwrap()[0];
        assert_ne!(original_slot, alternative_slot);
        let mut rewritten = program.clone();
        change_word(&mut rewritten, left.iterator, 0, alternative_slot);
        assert!(FullVerifier::verify(&rewritten).is_err(), "an original iteration source cannot be rewritten to another List[Str] parameter just because its storage type agrees");
    });
}

#[test]
fn cold_iteration_reads_require_the_actual_original_loop_body() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let _symbols = program.symbol_owner().enter();
        let bindings = program.generic_evidence().unwrap().iteration_bindings().map(|(_, binding)| binding.clone()).collect::<Vec<_>>();
        let left = &bindings[0]; let right = &bindings[1];
        let mut sibling = program.clone();
        let left_roots = sibling.store.blocks[left.body.index()].instructions;
        let right_roots = sibling.store.blocks[right.body.index()].instructions;
        sibling.store.blocks[left.body.index()].instructions = right_roots;
        sibling.store.blocks[right.body.index()].instructions = left_roots;
        let error = FullVerifier::verify(&sibling).unwrap_err();
        assert!(error.message.contains("outside its original loop body"), "same-owner sibling bodies cannot exchange read visibility: {}", error.message);

        let InstructionOwner::Function(function) = left.owner else { unreachable!() };
        let function_body = IrBlockId::from_raw(program.store.functions[function.index()].body).unwrap();
        let outer_range = program.store.blocks[function_body.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let loop_range = program.store.blocks[left.body.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let outside_position = (outer_range.start + 1..outer_range.end).find(|&position| program.store.tags[program.store.extra[position] as usize] == FullTag::StmtPrint).expect("the source keeps a print statement outside both loops");
        let outer_root = program.store.extra[outside_position];
        let item_root = program.store.extra[loop_range.start + 1];
        assert_ne!(outer_root, left.instruction);
        assert_ne!(outer_root, right.instruction);
        let mut outside = program.clone();
        outside.store.extra[loop_range.start + 1] = outer_root;
        outside.store.extra[outside_position] = item_root;
        let error = FullVerifier::verify(&outside).unwrap_err();
        assert!(error.message.contains("outside its original loop body"), "a loop's native operand moved outside its lexical body must reject: {}", error.message);
    });
}

#[test]
fn cold_iteration_item_read_cannot_move_into_its_own_literal_iterator() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc quoted() [io] { for value in [\"one\"] { print ${shlex.quote(value)} } }\nquoted()\n", 1);
        let _symbols = program.symbol_owner().enter();
        let binding = program.generic_evidence().unwrap().iteration_bindings().next().unwrap().1;
        assert!(binding.iterator_parameter.is_none());
        assert_eq!(program.store.tags[binding.iterator as usize], FullTag::ExprList);
        let iterator_block = IrBlockId::from_raw(program.store.payload(program.store.data[binding.iterator as usize].range()).unwrap()[0]).unwrap();
        let iterator_range = program.store.blocks[iterator_block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[iterator_range.start], 1);
        let literal = program.store.extra[iterator_range.start + 1];
        let body_roots = program.store.payload(program.store.blocks[binding.body.index()].instructions).unwrap();
        let print = body_roots[1];
        assert_eq!(program.store.tags[print as usize], FullTag::StmtPrint);
        let print_block = IrBlockId::from_raw(program.store.payload(program.store.data[print as usize].range()).unwrap()[0]).unwrap();
        let print_arguments = program.store.payload(program.store.blocks[print_block.index()].instructions).unwrap();
        assert_eq!(print_arguments[0], 1);
        let formatted = print_arguments[1];
        assert_eq!(program.store.tags[formatted as usize], FullTag::ExprFmtString);
        let parts = IrBlockId::from_raw(program.store.payload(program.store.data[formatted as usize].range()).unwrap()[0]).unwrap();
        let parts_range = program.store.blocks[parts.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[parts_range.start], 1);
        assert_eq!(program.store.extra[parts_range.start + 1], 1);
        let quote = program.store.extra[parts_range.start + 2];
        assert_eq!(program.store.tags[quote as usize], FullTag::ExprModuleCall);
        let mut changed = program.clone();
        changed.store.extra[iterator_range.start + 1] = quote;
        changed.store.extra[parts_range.start + 2] = literal;
        let error = FullVerifier::verify(&changed).unwrap_err();
        assert!(error.message.contains("outside its original loop body"), "a same-typed quote expression cannot read the item before its iterator is evaluated: {}", error.message);
    });
}
