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
        let quote = print_arguments[1];
        assert_eq!(program.store.tags[quote as usize], FullTag::ExprModuleCall);
        let mut changed = program.clone();
        changed.store.extra[iterator_range.start + 1] = quote;
        let print_range = program.store.blocks[print_block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        changed.store.extra[print_range.start + 1] = literal;
        let error = FullVerifier::verify(&changed).unwrap_err();
        assert!(error.message.contains("outside its original loop body") || error.message.contains("container changes its original operand order"), "the original iterator or body authority must reject an item read before iterator evaluation: {}", error.message);
    });
}

#[test]
fn cold_comprehension_producers_reject_altered_generator_qualifier_and_owner() {
    crate::runtime::eval::run_eval(|| {
        let source = "proc collected() [] -> List[Int] { [octet for character in \"ab\" for octet in b\"\\x01\\x02\" if character == \"a\"] }\n";
        let program = fixture_source(source, 0);
        let _symbols = program.symbol_owner().enter();
        let proof = program.generic_evidence().unwrap().comprehensions().next().expect("the list producer owns its original qualifier proof");
        assert_eq!(proof.generators.len(), 2);
        assert_eq!(proof.filters.len(), 1);
        assert_eq!(proof.generators[0].origin.qualifier, 0);
        assert_eq!(proof.generators[1].origin.qualifier, 1);
        assert_eq!(proof.filters[0].0, 2);
        assert_eq!(proof.reads.len(), 2);
        let mut generator = program.clone();
        let range = generator.store.blocks[proof.qualifier_block.index()].instructions.bounds(generator.store.extra.len()).unwrap();
        generator.store.extra[range.start + 4] = proof.generators[1].iterator.instruction;
        assert!(FullVerifier::verify(&generator).is_err(), "a selected Str generator cannot acquire the Bytes generator's input");
        let mut qualifier = program.clone();
        qualifier.store.extra[range.start] -= 1;
        assert!(FullVerifier::verify(&qualifier).is_err(), "an original filter cannot disappear from the qualifier sequence");
        let mut foreign = program.clone();
        foreign.store.generic.as_mut().unwrap().test_comprehension_mut(proof.instruction).unwrap().owner = InstructionOwner::Driver(0);
        assert!(FullVerifier::verify(&foreign).is_err(), "a foreign owner cannot replace the original producer's lexical environment");
        let filter = &proof.filters[0].1;
        let (operation_id, operation) = program.generic_evidence().unwrap().operations().find(|(_, operation)| program.generic_evidence().unwrap().operation_source(operation.source).unwrap().instruction == filter.instruction).unwrap();
        let mut rewritten_filter = program.clone();
        let operator = rewritten_filter.store.payload(rewritten_filter.store.data[filter.instruction as usize].range()).unwrap()[0] as usize;
        rewritten_filter.store.binary_ops[operator] = BinaryOp::Ne;
        let mut changed_authority = operation.authority.clone();
        let PreparedOperationAuthority::Language { operation: selected, .. } = &mut changed_authority else { unreachable!() };
        *selected = PreparedLanguageOperation::Equality { op: BinaryOp::Ne };
        let generic = rewritten_filter.store.generic.as_mut().unwrap();
        generic.test_operation_mut(operation_id).unwrap().authority = changed_authority.clone();
        generic.test_operation_source_mut(operation.source).unwrap().expected = changed_authority;
        let error = FullVerifier::verify(&rewritten_filter).unwrap_err();
        assert!(error.message.contains("comprehension filter") || error.message.contains("prepared operation differs from its original receipt"), "matching mutable operation copies cannot replace the original filter selection: {}", error.message);
        let mut ordinal = program.clone();
        ordinal.store.generic.as_mut().unwrap().test_comprehension_mut(proof.instruction).unwrap().generators[1].origin.qualifier = 0;
        assert!(FullVerifier::verify(&ordinal).is_err(), "jointly matching qualifier metadata cannot change an original generator ordinal");
    });
}

#[test]
fn cold_int_list_item_bindings_keep_original_parameter_and_lexical_read_identity() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc masked(first: List[Int], second: List[Int]) [io] {\n for left in first { print ${left.bit_and(1)} }\n for right in second { print ${right.bit_and(1)} }\n}\nmasked([2], [3])\n", 2);
        let _symbols = program.symbol_owner().enter();
        let bindings = program.generic_evidence().unwrap().iteration_bindings().map(|(id, binding)| (id, binding.clone())).collect::<Vec<_>>();
        let (left_id, left) = &bindings[0];
        let (right_id, right) = &bindings[1];
        assert_eq!(program.store.semantic.to_type(left.item).unwrap(), Type::Int);
        assert_eq!(left.item, right.item);
        assert_ne!(left.slot, right.slot);
        let alternative = program.store.payload(program.store.data[right.iterator as usize].range()).unwrap()[0];
        let mut changed_parameter = program.clone();
        change_word(&mut changed_parameter, left.iterator, 0, alternative);
        assert!(FullVerifier::verify(&changed_parameter).is_err(), "another List[Int] parameter cannot replace the original producer port");
        let read = program.generic_evidence().unwrap().iteration_uses().find(|use_| use_.binding == *left_id).unwrap();
        let mut redirected = program.clone();
        change_word(&mut redirected, read.instruction, 0, right.slot);
        redirected.store.generic.as_mut().unwrap().test_iteration_use_mut(read.instruction).unwrap().binding = *right_id;
        assert!(FullVerifier::verify(&redirected).is_err(), "an Int read cannot acquire another original binding through matching storage kinds");
    });
}

#[test]
fn cold_specialized_int_iteration_reads_keep_original_opcode_and_bytes_producer() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc total() [] -> Int { var sum: Int = 0\n for octet in b\"\\x01\\x02\" { sum = sum + octet }\n sum }\n", 1);
        let _symbols = program.symbol_owner().enter();
        let binding = program.generic_evidence().unwrap().iteration_bindings().next().unwrap().1;
        assert_eq!(program.store.semantic.to_type(binding.input).unwrap(), Type::Bytes);
        assert_eq!(program.store.semantic.to_type(binding.item).unwrap(), Type::Int);
        let read = program.generic_evidence().unwrap().iteration_uses().find(|read| read.tag == FullTag::IntSlot).expect("typed arithmetic retains the original specialized item read");
        let mut opcode = program.clone();
        opcode.store.tags[read.instruction as usize] = FullTag::ExprParam;
        assert!(FullVerifier::verify(&opcode).is_err(), "matching Int storage cannot change the original specialized read opcode");
        let mut own_iterator = program.clone();
        change_word(&mut own_iterator, binding.instruction, 1, read.instruction);
        assert!(FullVerifier::verify(&own_iterator).is_err(), "a Bytes item read cannot become its own original iterator producer");
    });
}

#[test]
fn cold_path_list_items_keep_original_opaque_types_parameter_ports_and_read_bindings() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc displayed(first: List[Path], second: List[Path]) [io] {\n for left in first { print ${left.display()} }\n for right in second { print ${right.display()} }\n}\ndisplayed([p\"first\"], [p\"second\"])\n", 2);
        let _symbols = program.symbol_owner().enter();
        let bindings = program.generic_evidence().unwrap().iteration_bindings().map(|(id, binding)| (id, binding.clone())).collect::<Vec<_>>();
        let (left_id, left) = &bindings[0];
        let (right_id, right) = &bindings[1];
        assert_eq!(program.store.semantic.to_type(left.input).unwrap(), Type::List(Box::new(Type::Path)));
        assert_eq!(program.store.semantic.to_type(left.item).unwrap(), Type::Path);
        assert_eq!(left.item, right.item);
        let alternative = program.store.payload(program.store.data[right.iterator as usize].range()).unwrap()[0];
        let mut changed_parameter = program.clone();
        change_word(&mut changed_parameter, left.iterator, 0, alternative);
        assert!(FullVerifier::verify(&changed_parameter).is_err(), "another opaque Path container cannot replace the original parameter producer");
        let read = program.generic_evidence().unwrap().iteration_uses().find(|read| read.binding == *left_id).unwrap();
        let mut redirected = program.clone();
        change_word(&mut redirected, read.instruction, 0, right.slot);
        redirected.store.generic.as_mut().unwrap().test_iteration_use_mut(read.instruction).unwrap().binding = *right_id;
        assert!(FullVerifier::verify(&redirected).is_err(), "matching Path types cannot replace the original lexical item binding");
        let mut changed_item = program.clone();
        let altered = changed_item.store.generic.as_mut().unwrap().test_iteration_binding_mut(*left_id).unwrap();
        altered.item = left.input;
        altered.binding_type = left.input;
        assert!(FullVerifier::verify(&changed_item).is_err(), "matching altered item and binding types cannot replace the original opaque Path contract");
    });
}

#[test]
fn cold_result_bytes_iteration_keeps_original_carrier_and_generated_projection() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc total(first: Result[Bytes], second: Result[Bytes]) [error] -> Result[Int] { var sum: Int = 0\n for left in first { sum = sum + left }\n for right in second { sum = sum + right }\n sum }\n", 2);
        let _symbols = program.symbol_owner().enter();
        let bindings = program.generic_evidence().unwrap().iteration_bindings().map(|(id, binding)| (id, binding.clone())).collect::<Vec<_>>();
        let (left_id, left) = &bindings[0];
        let (_, right) = &bindings[1];
        let left_carrier = left.iterator_carrier.expect("the compiler projection retains its authored Result carrier");
        let right_carrier = right.iterator_carrier.unwrap();
        assert_eq!(program.store.tags[left.iterator as usize], FullTag::ExprTry);
        assert_eq!(program.store.tags[left_carrier as usize], FullTag::ExprParam);
        assert_eq!(program.store.semantic.to_type(left.input).unwrap(), Type::Result(Box::new(Type::Bytes), Box::new(Type::Error)));
        assert_eq!(program.store.semantic.to_type(left.item).unwrap(), Type::Int);
        let mut changed_child = program.clone();
        change_word(&mut changed_child, left.iterator, 0, right_carrier);
        assert!(FullVerifier::verify(&changed_child).is_err(), "another Result Bytes source cannot replace the original generated projection child");
        changed_child.store.generic.as_mut().unwrap().test_iteration_binding_mut(*left_id).unwrap().iterator_carrier = Some(right_carrier);
        assert!(FullVerifier::verify(&changed_child).is_err(), "agreeing receipt metadata cannot replace the protected original Result producer");
        let read = program.generic_evidence().unwrap().iteration_uses().find(|read| read.binding == *left_id).unwrap();
        let mut own_iterator = program.clone();
        change_word(&mut own_iterator, left.iterator, 0, read.instruction);
        assert!(FullVerifier::verify(&own_iterator).is_err(), "a Result Bytes item cannot become its own carrier producer");
        let mut erased_projection = program.clone();
        erased_projection.store.tags[left.iterator as usize] = FullTag::ExprParam;
        assert!(FullVerifier::verify(&erased_projection).is_err(), "a generated success projection cannot acquire authored parameter identity");
    });
}

#[test]
fn cold_map_comprehension_preserves_original_key_value_target_and_qualifier_continuations() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc collected() [] -> Map[Str, Int] {\n {inner_key: inner_value for {key, value} in {[\"outer\"]: 1} if key == \"outer\" for {key: inner_key, value: inner_value} in {[key]: value}}\n}\n", 0);
        let _symbols = program.symbol_owner().enter();
        let proof = program.generic_evidence().unwrap().comprehensions().find(|proof| proof.key.is_some()).unwrap();
        let key = proof.key.as_ref().unwrap();
        assert_eq!(proof.generators.len(), 2);
        assert_eq!(proof.generators[0].bindings.len(), 2);
        assert_eq!(proof.generators[1].origin.qualifier, 2);
        assert_eq!(proof.filters[0].0, 1);
        assert_eq!(program.store.tags[proof.instruction as usize], FullTag::ExprMapComp);
        assert_eq!(program.store.semantic.to_type(proof.result).unwrap(), Type::Map(Box::new(Type::Str), Box::new(Type::Int)));
        let mut changed_key = program.clone();
        change_word(&mut changed_key, proof.instruction, 0, proof.value.instruction);
        assert!(FullVerifier::verify(&changed_key).is_err(), "the original key cannot acquire value authority through a payload rewrite");
        let mut changed_value = program.clone();
        change_word(&mut changed_value, proof.instruction, 1, key.instruction);
        assert!(FullVerifier::verify(&changed_value).is_err(), "the original value cannot acquire key authority through a payload rewrite");
        let mut changed_path = program.clone();
        changed_path.store.generic.as_mut().unwrap().test_comprehension_mut(proof.instruction).unwrap().generators[0].bindings[0].path = vec![Name::intern("value")];
        assert!(FullVerifier::verify(&changed_path).is_err(), "matching item storage cannot replace the original projected entry field");
        let second = &proof.generators[1];
        let inner_key = second.bindings.iter().find(|binding| binding.path == [Name::intern("key")]).unwrap();
        let outer_key = proof.generators[0].bindings.iter().find(|binding| binding.path == [Name::intern("key")]).unwrap();
        assert_eq!(inner_key.ty, outer_key.ty);
        let inner_key_read = proof.reads.iter().position(|(_, binding)| *binding == inner_key.identity).unwrap();
        let mut same_typed_key = program.clone();
        change_word(&mut same_typed_key, proof.reads[inner_key_read].0.instruction, 0, outer_key.slot);
        same_typed_key.store.generic.as_mut().unwrap().test_comprehension_mut(proof.instruction).unwrap().reads[inner_key_read].1 = outer_key.identity;
        assert!(FullVerifier::verify(&same_typed_key).is_err(), "matching Str keys cannot exchange their original captured generator bindings");
        let read = proof.reads.iter().find(|(_, binding)| second.bindings.iter().any(|candidate| candidate.identity == *binding)).unwrap().0.instruction;
        let mut own_iterator = program.clone();
        let qualifier = own_iterator.store.blocks[proof.qualifier_block.index()].instructions.bounds(own_iterator.store.extra.len()).unwrap();
        fn skip_target(input: &mut FullCursor<'_>) {
            match input.raw().unwrap() {
                0 => { input.raw().unwrap(); }
                1 => { let count = input.raw().unwrap(); for _ in 0..count { input.raw().unwrap(); skip_target(input); input.raw().unwrap(); } }
                2 => {},
                _ => panic!("fixture owns a checked comprehension target"),
            }
        }
        let mut cursor = FullCursor::new(&proof.qualifier_payload);
        assert_eq!(cursor.raw().unwrap(), 3);
        for _ in 0..second.origin.qualifier {
            if cursor.raw().unwrap() == 0 { skip_target(&mut cursor); }
            cursor.raw().unwrap();
            cursor.raw().unwrap();
        }
        assert_eq!(cursor.raw().unwrap(), 0);
        skip_target(&mut cursor);
        let iterator_word = qualifier.start + cursor.index;
        assert_eq!(cursor.raw().unwrap(), second.iterator.instruction);
        own_iterator.store.extra[iterator_word] = read;
        assert!(FullVerifier::verify(&own_iterator).is_err(), "a later generator item cannot become its own iterator producer");
        let mut rewritten_qualifier = program.clone();
        rewritten_qualifier.store.generic.as_mut().unwrap().test_comprehension_mut(proof.instruction).unwrap().generators[1].origin.qualifier = 0;
        assert!(FullVerifier::verify(&rewritten_qualifier).is_err(), "matching target metadata cannot change an original generator continuation ordinal");
    });
}

#[test]
fn cold_stream_iteration_producers_reject_changed_call_and_binding_initializer() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("stream rows(value: Int) [] -> Stream[Int] { yield value }\nproc shown() [io] { for value in rows(2) { print ${value.bit_and(1)} } }\nshown()\nlet first = rows(1)\nlet second = rows(2)\nfor value in first { print ${value.bit_and(1)} }\n", 2);
        let _symbols = program.symbol_owner().enter();
        let bindings = program.generic_evidence().unwrap().iteration_bindings().map(|(id, binding)| (id, binding.clone())).collect::<Vec<_>>();
        let (call_id, call) = bindings.iter().find(|(_, binding)| binding.producer.as_ref().is_some_and(|producer| producer.declaration.is_some())).unwrap();
        let (read_id, read) = bindings.iter().find(|(_, binding)| binding.producer.as_ref().is_some_and(|producer| producer.initializer.is_some())).unwrap();
        let mut changed = program.clone();
        change_word(&mut changed, call.iterator, 0, u32::MAX);
        assert!(FullVerifier::verify(&changed).is_err(), "the actual stream callee cannot change");
        let mut coupled = program.clone();
        coupled.store.generic.as_mut().unwrap().test_iteration_binding_mut(*call_id).unwrap().producer.as_mut().unwrap().words[0] = u32::MAX;
        change_word(&mut coupled, call.iterator, 0, u32::MAX);
        assert!(FullVerifier::verify(&coupled).is_err(), "an altered visible receipt cannot authorize another callee");
        let mut changed = program.clone();
        change_word(&mut changed, read.iterator, 0, read.slot);
        assert!(FullVerifier::verify(&changed).is_err(), "an item cannot replace the immutable stream binding");
        let mut coupled = program.clone();
        coupled.store.generic.as_mut().unwrap().test_iteration_binding_mut(*read_id).unwrap().producer.as_mut().unwrap().initializer.as_mut().unwrap().2 = call.iterator;
        assert!(FullVerifier::verify(&coupled).is_err(), "the original binding cannot acquire a same-typed initializer");
        let (_, step, _, _) = read.producer.as_ref().unwrap().initializer.unwrap();
        let mut changed = program.clone();
        let range = changed.store.driver_steps[step as usize].data.range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range.start] = Name::intern("second").symbol().raw();
        assert!(FullVerifier::verify(&changed).is_err(), "the earlier lexical binding cannot change its original name");
        let InstructionOwner::Driver(current) = read.owner else { unreachable!() };
        let mut changed = program.clone();
        let range = changed.store.driver_steps[current as usize].slots.bounds(changed.store.driver_slots.len()).unwrap();
        let iterator_slot = read.producer.as_ref().unwrap().words[0];
        let lexical = changed.store.driver_slots[range].iter_mut().find(|slot| slot.slot == iterator_slot).unwrap();
        lexical.flags |= DRIVER_SLOT_MUTABLE;
        assert!(FullVerifier::verify(&changed).is_err(), "the original immutable iterator cannot become mutable");

    });
}

#[test]
fn cold_fs_children_iteration_keeps_original_record_and_native_result_transport() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc shown() [io, fs, error] -> Result[Unit] { for entry in fs.children(p\".\") { print ${entry.name} } }\nshown()?\n", 1);
        let _symbols = program.symbol_owner().enter();
        let (id, binding) = program.generic_evidence().unwrap().iteration_bindings().next().unwrap();
        let binding = binding.clone();
        assert!(matches!(program.store.semantic.to_type(binding.input).unwrap(), Type::Result(success, _) if matches!(*success, Type::Stream(_))));
        assert!(matches!(program.store.semantic.to_type(binding.item).unwrap(), Type::Record(_)));
        assert!(binding.iterator_carrier.is_none(), "stream adapters retain their original outer failure transport");
        let mut changed = program.clone();
        changed.store.tags[binding.iterator as usize] = FullTag::ExprTry;
        assert!(FullVerifier::verify(&changed).is_err(), "native Result stream iteration cannot acquire a scalar projection");
        let mut coupled = program.clone();
        let original = coupled.store.generic.as_mut().unwrap().test_iteration_binding_mut(id).unwrap();
        original.item = original.input; original.binding_type = original.input;
        assert!(FullVerifier::verify(&coupled).is_err(), "a coupled type rewrite cannot change the original record item");
        let mut changed = program.clone();
        change_word(&mut changed, binding.iterator, 0, u32::MAX);
        assert!(FullVerifier::verify(&changed).is_err(), "the selected native operation cannot change");
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_ground_native_calls();
        assert!(FullVerifier::verify(&missing).is_err(), "the item receipt cannot replace independent native call authority");
    });
}

#[test]
fn cold_immutable_driver_record_list_items_keep_original_initializer_and_record_roots() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("let first = [{name: \"one\", count: 2}]\nlet second = [{name: \"two\", count: 3}]\nfor row in first { print ${row.name} }\n", 1);
        let _symbols = program.symbol_owner().enter();
        let (id, binding) = program.generic_evidence().unwrap().iteration_bindings().next().unwrap();
        let binding = binding.clone();
        assert!(matches!(program.store.semantic.to_type(binding.input).unwrap(), Type::List(item) if matches!(*item, Type::Record(_))));
        let (_, step, _, _) = binding.producer.as_ref().unwrap().initializer.unwrap();
        let mut changed = program.clone();
        let range = changed.store.driver_steps[step as usize].data.range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range.start] = Name::intern("second").symbol().raw();
        assert!(FullVerifier::verify(&changed).is_err(), "a same-typed record list cannot replace the original immutable allocation");
        let mut coupled = program.clone();
        let receipt = coupled.store.generic.as_mut().unwrap().test_iteration_binding_mut(id).unwrap();
        receipt.item = receipt.input; receipt.binding_type = receipt.input;
        assert!(FullVerifier::verify(&coupled).is_err(), "item and binding roots cannot jointly acquire a container type");
        let mut changed = program.clone();
        change_word(&mut changed, binding.iterator, 0, binding.slot);
        assert!(FullVerifier::verify(&changed).is_err(), "a record item cannot replace its source list");
    });
}

#[test]
fn cold_literal_record_list_items_keep_original_children_and_independent_container_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc shown() [io] { for first in [{name: \"one\", count: 2}] { print ${first.name} }; for second in [{name: \"two\", count: 3}] { print ${second.name} } }\nshown()\n", 2);
        let _symbols = program.symbol_owner().enter();
        let bindings = program.generic_evidence().unwrap().iteration_bindings().map(|(id, binding)| (id, binding.clone())).collect::<Vec<_>>();
        let (id, binding) = &bindings[0];
        let sibling = &bindings[1].1;
        assert!(matches!(program.store.semantic.to_type(binding.input).unwrap(), Type::List(item) if matches!(*item, Type::Record(_))));
        let producer = binding.producer.as_ref().unwrap();
        assert_eq!(producer.tag, FullTag::ExprList);
        assert!(producer.initializer.is_none() && producer.declaration.is_none() && producer.lines.is_none());
        assert_eq!(binding.item, sibling.item);
        let mut changed = program.clone();
        change_word(&mut changed, binding.instruction, 1, sibling.iterator);
        assert!(FullVerifier::verify(&changed).is_err(), "a same-layout literal cannot replace the authored source list");
        let mut changed = program.clone();
        let sibling_words = sibling.producer.as_ref().unwrap().words.clone();
        let range = changed.store.data[binding.iterator as usize].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range].copy_from_slice(&sibling_words);
        assert!(FullVerifier::verify(&changed).is_err(), "same-layout record children cannot replace the original literal children");
        let mut coupled = changed;
        coupled.store.generic.as_mut().unwrap().test_iteration_binding_mut(*id).unwrap().producer.as_mut().unwrap().words = sibling_words;
        assert!(FullVerifier::verify(&coupled).is_err(), "copied dense words cannot grant a new original literal source");
        let mut coupled = program.clone();
        let receipt = coupled.store.generic.as_mut().unwrap().test_iteration_binding_mut(*id).unwrap();
        receipt.item = receipt.input; receipt.binding_type = receipt.input;
        assert!(FullVerifier::verify(&coupled).is_err(), "item and binding roots cannot jointly acquire a container type");
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_ground_containers();
        assert!(FullVerifier::verify(&missing).is_err(), "iteration authority cannot replace independent list construction receipts");
        let mut missing = program.clone();
        assert_eq!(program.generic_evidence().unwrap().constructors().len(), 2);
        missing.store.generic.as_mut().unwrap().test_remove_iteration_record_constructor_layouts();
        assert!(FullVerifier::verify(&missing).is_err(), "iteration authority cannot replace independent record constructor layouts");
    });
}

#[test]
fn cold_optimized_line_items_keep_original_receiver_slot_member_and_body() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc shown(first: Str, second: Str) [io] { for line in first.lines() { print ${line} } }\nshown(\"one\", \"two\")\n", 1);
        let _symbols = program.symbol_owner().enter();
        let (id, binding) = program.generic_evidence().unwrap().iteration_bindings().next().unwrap();
        let binding = binding.clone();
        assert_eq!(program.store.tags[binding.instruction as usize], FullTag::StmtForStrLines);
        assert_eq!(program.store.semantic.to_type(binding.item).unwrap(), Type::Str);
        let lines = binding.producer.as_ref().unwrap().lines.as_ref().unwrap();
        assert_eq!(program.store.semantic.to_type(lines.receiver_type).unwrap(), Type::Str);
        assert_eq!(lines.parameter.1, 0);
        let mut changed = program.clone();
        change_word(&mut changed, binding.iterator, 0, 1);
        assert!(FullVerifier::verify(&changed).is_err(), "a same-typed scalar receiver cannot replace the authored line source");
        let mut coupled = program.clone();
        let producer = coupled.store.generic.as_mut().unwrap().test_iteration_binding_mut(id).unwrap().producer.as_mut().unwrap();
        producer.words[0] = 1; producer.lines.as_mut().unwrap().parameter.1 = 1;
        change_word(&mut coupled, binding.iterator, 0, 1);
        assert!(FullVerifier::verify(&coupled).is_err(), "matching dense words and copied receipt cannot change the formal producer port");
        let mut changed = program.clone();
        changed.store.tags[binding.instruction as usize] = FullTag::StmtFor;
        assert!(FullVerifier::verify(&changed).is_err(), "line allocation cannot acquire an ordinary Stream iterator opcode");
        let mut coupled = program.clone();
        let source = coupled.store.generic.as_mut().unwrap().test_iteration_binding_mut(id).unwrap().producer.as_mut().unwrap().lines.as_mut().unwrap();
        let PreparedOperationAuthority::Registry { operation, .. } = &mut source.authority else { unreachable!() };
        *operation = RuntimeOp::BytesStreamLines;
        assert!(FullVerifier::verify(&coupled).is_err(), "copied native authority cannot change the original item representation");
        let read = program.generic_evidence().unwrap().iteration_uses().next().unwrap();
        let mut changed = program.clone();
        change_word(&mut changed, binding.instruction, 1, read.instruction);
        assert!(FullVerifier::verify(&changed).is_err(), "a body item cannot become the optimized source receiver");
    });
}

#[test]
fn cold_line_scanner_keeps_original_members_prefixes_counter_allocations_and_source() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture_source("proc shown(first: Bytes, second: Bytes) [io] { var blanks = 0; var comments = 0; for line in first.lines() { let trimmed = line.trim(); if trimmed == b\"\" { blanks += 1 } else if trimmed.starts_with(b\"#\") { comments += 1 } }; print ${blanks} ${comments} }\nshown(b\"#one\\n\\n\", b\"other\")\n", 0);
        let _symbols = program.symbol_owner().enter();
        let scan = program.generic_evidence().unwrap().line_scans().next().expect("the authored counter scanner retains an original fused receipt").clone();
        assert_eq!(scan.checks.len(), 2);
        assert_eq!(program.store.tags[scan.instruction as usize], FullTag::StmtScanLines);
        assert!(matches!(scan.trim.as_ref().unwrap().authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesTrim, .. }));
        assert!(matches!(scan.checks[1].predicate.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesStartsWith, .. }));
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_line_scans();
        assert!(FullVerifier::verify(&missing).is_err(), "a fused scan cannot recreate its removed source members and counter writes");
        let mut changed = program.clone();
        change_word(&mut changed, scan.instruction, 0, 1);
        assert!(FullVerifier::verify(&changed).is_err(), "a same-typed formal parameter cannot replace the original scan receiver");
        let mut coupled = program.clone();
        let copied = coupled.store.generic.as_mut().unwrap().test_line_scan_mut(scan.instruction).unwrap();
        copied.text_slot = 1; copied.payload[0] = 1; copied.lines.parameter.1 = 1;
        change_word(&mut coupled, scan.instruction, 0, 1);
        assert!(FullVerifier::verify(&coupled).is_err(), "matching emitted and copied words cannot rewrite original receiver provenance");
        let block = IrBlockId::from_raw(scan.checks_block.0).unwrap();
        let bounds = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(scan.checks_block.2.len(), 6);
        let mut changed = program.clone();
        changed.store.extra[bounds.start + 5] = scan.checks[0].slot;
        assert!(FullVerifier::verify(&changed).is_err(), "counter order and dense storage remain tied to each original assignment");
        let mut coupled = program.clone();
        coupled.store.extra[bounds.start + 5] = scan.checks[0].slot;
        let copied = coupled.store.generic.as_mut().unwrap().test_line_scan_mut(scan.instruction).unwrap();
        copied.checks_block.2[5] = scan.checks[0].slot;
        copied.checks[1].slot = scan.checks[0].slot; copied.checks[1].counter = scan.checks[0].counter; copied.checks[1].allocation = scan.checks[0].allocation;
        assert!(FullVerifier::verify(&coupled).is_err(), "joint same-typed counter rewrites cannot replace the protected original scan");
        let needle = scan.checks_block.2[4];
        let bytes = program.store.bytes[super::super::super::IrBytesId::from_raw(needle).unwrap().index()].bounds(program.store.byte_data.len()).unwrap();
        let mut changed = program.clone();
        changed.store.byte_data[bytes.start] = b'!';
        assert!(FullVerifier::verify(&changed).is_err(), "an unchanged pool ID cannot lend authority to different prefix bytes");
        let mut coupled = program.clone();
        let copied = coupled.store.generic.as_mut().unwrap().test_line_scan_mut(scan.instruction).unwrap();
        let PreparedOperationAuthority::Registry { operation, .. } = &mut copied.checks[1].predicate.authority else { unreachable!() };
        *operation = RuntimeOp::BytesEndsWith;
        assert!(FullVerifier::verify(&coupled).is_err(), "a copied native selection cannot replace the original predicate member");
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_mutable_binding_receipt(scan.checks[0].allocation);
        assert!(FullVerifier::verify(&missing).is_err(), "scan counter writes require independently prepared original mutable storage");
        let mut changed = program.clone();
        changed.store.tags[scan.instruction as usize] = FullTag::StmtForStrLines;
        assert!(FullVerifier::verify(&changed).is_err(), "a removed scanner body cannot acquire an ordinary line-loop operation");
        let mut changed = program.clone();
        let location = super::super::super::IrLocationId::from_raw(scan.payload[3]).unwrap();
        changed.store.locations[location.index()].len += 1;
        assert!(FullVerifier::verify(&changed).is_err(), "an unchanged location ID cannot replace the original fused source span");
        let mut coupled = program.clone();
        coupled.store.generic.as_mut().unwrap().test_line_scan_mut(scan.instruction).unwrap().checks[0].increment_span = scan.checks[1].increment_span;
        assert!(FullVerifier::verify(&coupled).is_err(), "counter faults retain their own authored assignment span");
    });
}

#[test]
fn line_scanner_counter_overflow_keeps_authored_assignment_span_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure overflow(text: Bytes) -> Int { var count = 9223372036854775807; for line in text.lines() { if line.starts_with(b\"x\") { count += 1 } }; return count }\n";
        let program = Arc::new(fixture_source(source, 0));
        program.symbol_owner().with_current(|| {
            let scan = program.generic_evidence().unwrap().line_scans().next().expect("overflow uses the actual prepared scanner");
            assert_eq!(program.store.tags[scan.instruction as usize], FullTag::StmtScanLines);
            assert_eq!(&source[scan.checks[0].increment_span.range()], "count += 1");
            for recursive in [false, true] {
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let function = LoweredFunctionKey::Name(Name::intern("overflow"));
                let arguments = [crate::runtime::value::Value::Bytes(b"x\n".to_vec())];
                let execute = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).expect("the prepared overflow scanner exists");
                let error = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, execute).expect_err("checked scanner counters refuse overflow");
                assert_eq!(error.kind, "integer-overflow");
                assert_eq!(error.span, Some(scan.checks[0].increment_span));
                assert_eq!(&source[error.span.unwrap().range()], "count += 1");
            }
        });
    });
}
