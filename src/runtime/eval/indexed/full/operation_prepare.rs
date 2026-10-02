use super::*;
use super::super::generic::{OperationSource, OperationSourceOrigin, PreparedOperation, PreparedOperationAuthority, PreparedOperationBinding, PreparedOperationEffects, graph_ground_type};
use crate::sema::inference::{Atom, EffectSummary, OperationBinding, RequirementTemplate};
use crate::sema::operation_graph::{ArithmeticDomain, PreparedLanguageOperation};

fn unprepared(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

type IntegerSlotKey = (bool, u32, u32);

fn integer_slot_key(owner: InstructionOwner, slot: u32) -> IntegerSlotKey {
    match owner { InstructionOwner::Function(function) => (false, function.raw(), slot), InstructionOwner::Driver(step) => (true, step, slot) }
}

#[derive(Default)]
struct IntegerSlotContract {
    declaration: Option<(u32, u32)>,
    conflicting_declaration: bool,
    untyped_write: bool,
    writes: Vec<u32>,
}

struct IntegerStorageIndex {
    owners: Vec<Option<InstructionOwner>>,
    slots: FxHashMap<IntegerSlotKey, IntegerSlotContract>,
    verified_slots: rustc_hash::FxHashSet<IntegerSlotKey>,
    verified_operands: rustc_hash::FxHashSet<u32>,
    #[cfg(test)]
    indexed_instructions: usize,
    #[cfg(test)]
    operand_visits: usize,
}

impl IntegerStorageIndex {
    fn build(store: &FullStore) -> Result<Self, IrVerifyError> {
        let owners = store.generic_instruction_owners()?;
        let mut slots: FxHashMap<IntegerSlotKey, IntegerSlotContract> = FxHashMap::default();
        for (instruction, owner) in owners.iter().copied().enumerate() {
            let Some(owner) = owner else { continue; };
            let tag = store.tags[instruction];
            if !matches!(tag, FullTag::StmtLetInt | FullTag::StmtLet | FullTag::StmtLetBool | FullTag::StmtAssignInt | FullTag::StmtAssign | FullTag::StmtAssignBool | FullTag::StmtAssignPath | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt) { continue; }
            let words = store.payload(store.data[instruction].range())?;
            let slot = *words.first().ok_or_else(|| IrVerifyError::new("integer storage statement lacks a slot"))?;
            let contract = slots.entry(integer_slot_key(owner, slot)).or_default();
            match tag {
                FullTag::StmtLetInt => {
                    let initializer = *words.get(1).ok_or_else(|| IrVerifyError::new("integer storage initializer is missing"))?;
                    if contract.declaration.replace((instruction as u32, initializer)).is_some() { contract.conflicting_declaration = true; }
                }
                FullTag::StmtLet | FullTag::StmtLetBool => contract.conflicting_declaration = true,
                FullTag::StmtAssignInt => contract.writes.push(instruction as u32),
                _ => contract.untyped_write = true,
            }
        }
        #[cfg(test)]
        let indexed_instructions = owners.len();
        Ok(Self { owners, slots, verified_slots: rustc_hash::FxHashSet::default(), verified_operands: rustc_hash::FxHashSet::default(),
            #[cfg(test)] indexed_instructions,
            #[cfg(test)] operand_visits: 0 })
    }
}

impl FullBuilder {
    pub(super) fn prepare_source_operations(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.as_ref().cloned() else { return Ok(()); };
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let Some(operation) = solved.operations.get(&expression) else { continue; };
            let graph = &solved.graph;
            let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| unprepared("operation_candidate_owner"))? else { continue; };
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| unprepared("operation_candidate_authority"))? else { continue; };
            let supported = match metadata.operation {
                PreparedLanguageOperation::Arithmetic { domain: ArithmeticDomain::Float | ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int }, .. }
                | PreparedLanguageOperation::Ordering { left: Atom::Str, right: Atom::Str, .. } => true,
                PreparedLanguageOperation::Equality { op: BinaryOp::Eq | BinaryOp::Ne } => selected.actual_arguments.len() == 2
                    && selected.actual_arguments.iter().all(|argument| argument.is_some_and(|ty| graph_ground_type(graph, ty).is_ok_and(|ty| ty == Type::Str))),
                _ => false,
            };
            if !supported { continue; }
            let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority,
                operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
            let scope = operation.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let source = self.generic_evidence_mut().add_operation_source(OperationSource {
                origin: OperationSourceOrigin::Expression(expression), identity: metadata.identity, expected: authority.clone(), instruction, owner, scope,
            }).map_err(|_| unprepared("operation_source_allocation"))?;
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| unprepared("operation_requirement"))? else { return Err(unprepared("operation_requirement_kind")); };
            let call = graph.operation_call(call).map_err(|_| unprepared("operation_call_owner"))?;
            if call.binding != OperationBinding::Slots || !selected.callback_invocations.is_empty()
                || operation.binding.dynamic.is_some() || operation.binding.rest_slot.is_some()
                || !operation.argument_coercions.is_empty() { return Err(unprepared("operation_binding_not_prepared")); }
            let ground = |builder: &mut FullBuilder, ty| {
                let ty = graph_ground_type(graph, ty).map_err(|_| unprepared("operation_ground_type"))?;
                builder.intern_generic_ground_type(&ty).map(TypeRef::Ground)
            };
            let receiver = call.receiver.map(|ty| ground(self, ty)).transpose()?;
            let arguments = selected.actual_arguments.iter().map(|ty| ty.map(|ty| ground(self, ty)).transpose()).collect::<Result<Vec<_>, _>>()?;
            let result = ground(self, selected.result)?;
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| unprepared("operation_effect_owner"))? {
                EffectSummary::Closed(bits) => Ok(bits), _ => Err(unprepared("operation_latent_effect_not_prepared")),
            };
            let effects = PreparedOperationEffects {
                creation: closed(selected.effects)?,
                inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
                outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
            };
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("operation_instruction_payload"))?;
            let tag = self.store.tags[instruction as usize];
            if (tag != FullTag::ExprBinary && !(tag == FullTag::IntBinary && matches!(metadata.operation, PreparedLanguageOperation::Arithmetic { domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int }, .. }))) || words.len() < 3 { return Err(unprepared("operation_instruction_not_prepared")); }
            let operands = Box::new([words[1], words[2]]);
            let slots = |slots: &[usize]| slots.iter().map(|&slot| u32::try_from(slot).map_err(|_| unprepared("operation_binding_slot_overflow"))).collect::<Result<Vec<_>, _>>().map(Vec::into_boxed_slice);
            self.generic_evidence_mut().add_operation(PreparedOperation {
                source,
                authority,
                receiver, arguments: arguments.into_boxed_slice(), result, effects,
                binding: PreparedOperationBinding { supplied_slots: slots(&operation.binding.supplied_slots)?,
                    default_slots: slots(&operation.binding.default_slots)?, rest_slot: None, dynamic: None, operands },
            }).map_err(|_| unprepared("operation_proof_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_source_operations(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        let mut storage = IntegerStorageIndex::build(store)?;
        for (_, operation) in generic.operations() {
            let source = generic.operation_source(operation.source)?;
            let PreparedOperationAuthority::Language { operation: language_operation, .. } = &operation.authority else { return Err(IrVerifyError::new("operation authority lacks an instruction verifier")); };
            let op = match language_operation {
                PreparedLanguageOperation::Arithmetic { op, domain: ArithmeticDomain::Float | ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } }
                | PreparedLanguageOperation::Ordering { op, left: Atom::Str, right: Atom::Str }
                | PreparedLanguageOperation::Equality { op } => op,
                _ => return Err(IrVerifyError::new("operation authority lacks an instruction verifier")),
            };
            let tag = store.tags.get(source.instruction as usize).copied();
            let integer = matches!(language_operation, PreparedLanguageOperation::Arithmetic { domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int }, .. });
            if tag != Some(FullTag::ExprBinary) && !(tag == Some(FullTag::IntBinary) && integer) { return Err(IrVerifyError::new("source operation proof is attached to another instruction kind")); }
            let words = store.payload(store.data[source.instruction as usize].range())?;
            let OperationSourceOrigin::Expression(expression) = source.origin else { return Err(IrVerifyError::new("source operation origin has another kind")); };
            if generic.registered_instruction_origin(source.instruction, false) != Some((source.origin, source.owner)) { return Err(IrVerifyError::new("source operation origin disagrees with its registered instruction")); }
            if tag == Some(FullTag::ExprBinary) && words.get(3).and_then(|&location| store.location_sources.get(location as usize)) != Some(&expression.source) { return Err(IrVerifyError::new("source operation origin disagrees with encoded source location")); }
            if words.first().and_then(|&index| store.binary_ops.get(index as usize)) != Some(op) { return Err(IrVerifyError::new("source operation proof disagrees with encoded operator")); }
            if words.get(1..3) != Some(operation.binding.operands.as_ref()) { return Err(IrVerifyError::new("source operation operand origins disagree with instruction")); }
            for (&operand, expected) in operation.binding.operands.iter().zip(operation.arguments.iter()) {
                let Some(TypeRef::Ground(expected)) = expected else { return Err(IrVerifyError::new("source operation operand is not ground")); };
                if tag == Some(FullTag::IntBinary) {
                    if store.semantic.to_type(*expected)? != Type::Int { return Err(IrVerifyError::new("specialized integer operation has another operand domain")); }
                    Self::verify_specialized_integer_operand(store, generic, operand, source.owner, &mut storage, &mut Vec::new())?;
                } else { Self::verify_generic_source(store, generic, operand, source.owner, &store.semantic.to_type(*expected)?, None, &mut Vec::new())?; }
            }
        }
        Ok(())
    }

    fn verify_specialized_integer_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, storage: &mut IntegerStorageIndex, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        if storage.owners.get(instruction as usize) != Some(&Some(owner)) || active.len() >= 256 { return Err(IrVerifyError::new("specialized integer operand is foreign, cyclic, or too deep")); }
        if active.contains(&instruction) {
            let words = store.payload(store.data[instruction as usize].range())?;
            let slot = words.first().copied().ok_or_else(|| IrVerifyError::new("specialized integer operand slot is missing"))?;
            let key = integer_slot_key(owner, slot);
            // An initialized slot anchors reads from its own typed writes;
            // instruction cycles outside that established slot remain invalid.
            if store.tags[instruction as usize] != FullTag::IntSlot || !storage.verified_slots.contains(&key)
                || storage.slots.get(&key).and_then(|contract| contract.declaration).is_some_and(|(declaration, _)| declaration >= instruction) {
                return Err(IrVerifyError::new("specialized integer operand is foreign, cyclic, or too deep"));
            }
            return Ok(());
        }
        if storage.verified_operands.contains(&instruction) { return Ok(()); }
        #[cfg(test)]
        { storage.operand_visits += 1; }
        active.push(instruction);
        let words = store.payload(store.data[instruction as usize].range())?;
        match store.tags[instruction as usize] {
            FullTag::IntInt => {},
            FullTag::IntSlot => {
                if Self::verify_pattern_operand(store, generic, instruction, owner, &Type::Int)? {
                    active.pop();
                    storage.verified_operands.insert(instruction);
                    return Ok(());
                }
                let slot = *words.first().ok_or_else(|| IrVerifyError::new("specialized integer operand slot is missing"))? as usize;
                let InstructionOwner::Function(function) = owner else { return Err(IrVerifyError::new("specialized integer slot lacks a fixed function contract")); };
                let callable = &store.functions[function.index()];
                let key = integer_slot_key(owner, slot as u32);
                if slot >= callable.params.len as usize && storage.slots.get(&key).and_then(|contract| contract.declaration).is_some_and(|(declaration, _)| declaration >= instruction) { return Err(IrVerifyError::new("specialized integer local is read before its storage declaration")); }
                if !storage.verified_slots.contains(&key) {
                    let (declaration, conflicting, untyped, writes) = storage.slots.get(&key).map(|contract| (contract.declaration, contract.conflicting_declaration, contract.untyped_write, contract.writes.clone())).unwrap_or((None, false, false, Vec::new()));
                    if conflicting || untyped { return Err(IrVerifyError::new("specialized integer storage has an untyped or conflicting write")); }
                    if slot < callable.params.len as usize {
                        if declaration.is_some() { return Err(IrVerifyError::new("specialized integer parameter has a conflicting storage declaration")); }
                        let params = callable.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("specialized integer parameter range is invalid"))?;
                        let ty = TypeId::from_raw(store.params[params.start + slot].type_id).ok_or_else(|| IrVerifyError::new("specialized integer parameter lacks a ground type"))?;
                        if store.semantic.to_type(ty)? != Type::Int { return Err(IrVerifyError::new("specialized integer operand disagrees with its parameter contract")); }
                    } else {
                        let (declaration, initializer) = declaration.ok_or_else(|| IrVerifyError::new("specialized integer local lacks a prepared storage contract"))?;
                        if declaration >= instruction { return Err(IrVerifyError::new("specialized integer local is read before its storage declaration")); }
                        Self::verify_specialized_integer_operand(store, generic, initializer, owner, storage, active)?;
                    }
                    // The initial fixed contract anchors self-references in
                    // subsequent typed writes; every write is still checked.
                    storage.verified_slots.insert(key);
                    for write in writes {
                        let words = store.payload(store.data[write as usize].range())?;
                        if !matches!(words.get(1).and_then(|&index| store.assign_ops.get(index as usize)), Some(AssignOp::Set | AssignOp::Add | AssignOp::Sub | AssignOp::Mul | AssignOp::Div | AssignOp::Rem)) { return Err(IrVerifyError::new("specialized integer storage has an invalid assignment operator")); }
                        let value = *words.get(2).ok_or_else(|| IrVerifyError::new("specialized integer storage write lacks its value"))?;
                        Self::verify_specialized_integer_operand(store, generic, value, owner, storage, active)?;
                    }
                }
            }
            FullTag::IntBinary => {
                if !matches!(words.first().and_then(|&index| store.binary_ops.get(index as usize)), Some(BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem)) { return Err(IrVerifyError::new("specialized integer operand has another operator")); }
                if let Some(operation) = generic.operation_at(instruction)? {
                    let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("specialized integer child result is not ground")); };
                    if store.semantic.to_type(result)? != Type::Int { return Err(IrVerifyError::new("specialized integer child has another result domain")); }
                }
                for &child in words.get(1..3).ok_or_else(|| IrVerifyError::new("specialized integer operand children are missing"))? { Self::verify_specialized_integer_operand(store, generic, child, owner, storage, active)?; }
            }
            _ => return Err(IrVerifyError::new("specialized integer operand lacks a prepared type contract")),
        }
        active.pop();
        storage.verified_operands.insert(instruction);
        Ok(())
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::sema::check::Checker;

    fn fixture() -> FullProgram {
        let source = "pure subtract(left: Float, right: Float) -> Float { left - right }\nlet difference = subtract(7.0, 2.0)\n";
        source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Float })
    }

    // Native sources cannot mutate prepared handles or instruction payloads;
    // these fixtures isolate the retained executable proof boundary.
    pub(in crate::runtime::eval::indexed::full) fn source_fixture(source: &str, expected: PreparedLanguageOperation) -> FullProgram {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "operation-proof.xsh", crate::loader::entry_source_from_text("operation-proof.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        assert!(bodies.solved.operations.values().any(|operation| {
            let graph = &bodies.solved.graph;
            let Some(evidence) = graph.candidate_evidence(operation.requirement).unwrap() else { return false; };
            matches!(bodies.solved.operation_catalog.candidate(graph, evidence.candidate).unwrap(),
                crate::sema::check::SolvedOperationAuthority::Language(metadata)
                    if metadata.operation == expected)
        }), "the original source must supply the selected operation {expected:?}");
        let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
        drop(parsed); drop(declarations); drop(bodies);
        let program = prepared.unwrap();
        program.symbol_owner().with_current(|| FullVerifier::verify(&program).unwrap());
        program
    }

    #[test]
    fn selected_integer_source_operation_requires_prepared_instruction_proof_after_frontend_drop() {
        let source = "pure subtract(left: Int, right: Int) -> Int { left - right }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int } });
        assert!(program.generic_evidence().is_some(), "selected integer source operation has no prepared proof store");
        assert_eq!(program.generic_evidence().unwrap().operations().count(), 1, "the original selected integer operation must survive preparation");
    }

    #[test]
    fn selected_specialized_integer_source_operation_keeps_the_original_prepared_proof() {
        let source = "pure subtract(left: Int, right: Int) -> Int { let difference = left - right; difference }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int } });
        assert!(program.store.tags.contains(&FullTag::IntBinary), "the fixture must exercise the specialized integer codec");
        assert!(program.generic_evidence().is_some(), "selected specialized integer operation has no prepared proof store");
        assert_eq!(program.generic_evidence().unwrap().operations().count(), 1, "integer specialization cannot drop its original source contract");
    }

    #[test]
    fn selected_specialized_integer_operations_preserve_checked_local_storage_contracts() {
        let source = "pure subtract(left: Int, right: Int) -> Int { let first = left - right; let second = first - right; second }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        assert_eq!(program.generic_evidence().unwrap().operations().count(), 2, "each selected operation retains its original source contract through fixed local storage");
    }

    #[test]
    fn selected_specialized_integer_local_storage_rejects_untyped_conflicting_and_foreign_writes() {
        let source = "pure other(value: Int) -> Int { let result = value - 1; result }\npure subtract(left: Int, right: Int) -> Int { var first = left - right; first = 3; first = first - right; let second = first - 1; second }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let storage = IntegerStorageIndex::build(&program.store).unwrap();
            let (&key, contract) = storage.slots.iter().find(|(_, contract)| !contract.writes.is_empty()).unwrap();
            let (declaration, _) = contract.declaration.unwrap();
            let write = contract.writes[0];
            let owner = storage.owners[declaration as usize].unwrap();
            let local_read = program.store.tags.iter().enumerate().find_map(|(instruction, &tag)| {
                (tag == FullTag::IntSlot && storage.owners[instruction] == Some(owner)
                    && program.store.payload(program.store.data[instruction].range()).unwrap().first() == Some(&key.2)).then_some(instruction)
            }).unwrap();
            let mut untyped_declaration = program.store.clone();
            untyped_declaration.tags[declaration as usize] = FullTag::StmtLet;
            assert!(FullVerifier::verify_generic_evidence(&untyped_declaration).unwrap_err().message.contains("untyped or conflicting"));
            let mut untyped_write = program.store.clone();
            untyped_write.tags[write as usize] = FullTag::StmtAssign;
            assert!(FullVerifier::verify_generic_evidence(&untyped_write).unwrap_err().message.contains("untyped or conflicting"));
            let mut conflicting = program.store.clone();
            conflicting.tags[write as usize] = FullTag::StmtLetInt;
            assert!(FullVerifier::verify_generic_evidence(&conflicting).unwrap_err().message.contains("untyped or conflicting"));
            let mut wrong_rhs = program.store.clone();
            let words = wrong_rhs.payload(wrong_rhs.data[write as usize].range()).unwrap();
            let rhs = words[2] as usize;
            wrong_rhs.tags[rhs] = FullTag::ExprParam;
            assert!(FullVerifier::verify_generic_evidence(&wrong_rhs).unwrap_err().message.contains("prepared type contract"));
            let mut invalid_assignment = program.store.clone();
            let range = invalid_assignment.data[write as usize].range();
            invalid_assignment.extra[range.start as usize + 1] = u32::MAX;
            assert!(FullVerifier::verify_generic_evidence(&invalid_assignment).unwrap_err().message.contains("invalid assignment operator"));
            let mut foreign_rhs = program.store.clone();
            let outsider = storage.owners.iter().enumerate().find_map(|(instruction, &actual)| {
                (actual.is_some() && actual != Some(owner) && program.store.tags[instruction] == FullTag::IntSlot).then_some(instruction as u32)
            }).unwrap();
            let range = foreign_rhs.data[write as usize].range();
            foreign_rhs.extra[range.start as usize + 2] = outsider;
            assert!(FullVerifier::verify_generic_evidence(&foreign_rhs).unwrap_err().message.contains("foreign, cyclic, or too deep"));
            let mut early_read = program.store.clone();
            let declaration_slot = early_read.payload(early_read.data[declaration as usize].range()).unwrap()[0];
            let (_, first_operation) = generic.operations().find(|(_, operation)| {
                let source = generic.operation_source(operation.source).unwrap();
                source.owner == owner && source.instruction < declaration
            }).unwrap();
            let operand = first_operation.binding.operands[0] as usize;
            assert_eq!(early_read.tags[operand], FullTag::IntSlot);
            let range = early_read.data[operand].range();
            early_read.extra[range.start as usize] = declaration_slot;
            assert!(FullVerifier::verify_generic_evidence(&early_read).unwrap_err().message.contains("read before its storage declaration"));
            assert!(local_read > declaration as usize);
        });
    }

    #[test]
    fn selected_specialized_integer_storage_checks_each_reachable_operand_once() {
        let mut source = String::from("pure subtract(left: Int, right: Int) -> Int { let first = left - 1;\n");
        for index in 0..32 { source.push_str(&format!("let value{index} = first - right;\n")); }
        source.push_str("value31 }\nlet difference = subtract(7, 2)\n");
        let program = source_fixture(&source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.operations().count(), 33);
            let mut storage = IntegerStorageIndex::build(&program.store).unwrap();
            let mut requests = 0;
            for _ in 0..8 {
                for (_, operation) in generic.operations() {
                    let owner = generic.operation_source(operation.source).unwrap().owner;
                    for &operand in operation.binding.operands.iter() {
                        FullVerifier::verify_specialized_integer_operand(&program.store, generic, operand, owner, &mut storage, &mut Vec::new()).unwrap();
                        requests += 1;
                    }
                }
            }
            assert_eq!(storage.indexed_instructions, program.store.tags.len());
            assert_eq!(storage.operand_visits, storage.verified_operands.len());
            assert!(storage.operand_visits < requests / 4, "repeated local reads must reuse their retained storage proof");
        });
    }

    #[test]
    fn selected_specialized_integer_nested_and_literal_operands_retain_typed_contracts() {
        let source = "pure subtract(left: Int, right: Int) -> Int { let result = (left - 1) - right; let second = result - 2; second }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let owners = program.store.generic_instruction_owners().unwrap();
            assert_eq!(generic.operations().count(), 3);
            let source = generic.operations().find_map(|(_, operation)| {
                let source = generic.operation_source(operation.source).unwrap();
                (program.store.tags[source.instruction as usize] == FullTag::IntBinary).then_some(source)
            }).unwrap();
            let literal = program.store.tags.iter().enumerate().find_map(|(instruction, &tag)| {
                (tag == FullTag::IntInt && owners[instruction] == Some(source.owner)).then_some(instruction)
            }).unwrap();
            let mut wrong_literal = program.store.clone();
            wrong_literal.tags[literal] = FullTag::ExprInt;
            assert!(FullVerifier::verify_generic_evidence(&wrong_literal).unwrap_err().message.contains("prepared type contract"));
            let mut cyclic = program.store.clone();
            let range = cyclic.data[source.instruction as usize].range();
            cyclic.extra[range.start as usize + 1] = source.instruction;
            let (id, _) = generic.operations().find(|(_, operation)| generic.operation_source(operation.source).unwrap().instruction == source.instruction).unwrap();
            cyclic.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.operands[0] = source.instruction;
            assert!(FullVerifier::verify_generic_evidence(&cyclic).unwrap_err().message.contains("foreign, cyclic, or too deep"));
        });
    }

    #[test]
    fn selected_specialized_integer_proof_rejects_rewritten_origin_parameter_domain_and_codec() {
        let source = "pure subtract(left: Int, right: Int, unused: Str) -> Int { let difference = left - right; difference }\nlet difference = subtract(7, 2, \"unused\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, operation) = generic.operations().next().unwrap();
            let source = generic.operation_source(operation.source).unwrap();
            assert_eq!(program.store.tags[source.instruction as usize], FullTag::IntBinary);
            let mut wrong_origin = program.store.clone();
            let OperationSourceOrigin::Expression(mut expression) = source.origin else { unreachable!() };
            expression.expression = crate::syntax::arena::ExprId::from_index(expression.expression.index() + 1);
            wrong_origin.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().origin = OperationSourceOrigin::Expression(expression);
            assert!(FullVerifier::verify_generic_evidence(&wrong_origin).unwrap_err().message.contains("original instruction"));
            let mut wrong_slot = program.store.clone();
            let operand = operation.binding.operands[0] as usize;
            assert_eq!(wrong_slot.tags[operand], FullTag::IntSlot);
            let range = wrong_slot.data[operand].range();
            wrong_slot.extra[range.start as usize] = 2;
            assert!(FullVerifier::verify_generic_evidence(&wrong_slot).unwrap_err().message.contains("parameter contract"));
            let mut wrong_codec = program.store.clone();
            wrong_codec.tags[operand] = FullTag::ExprParam;
            assert!(FullVerifier::verify_generic_evidence(&wrong_codec).unwrap_err().message.contains("prepared type contract"));
            let mut wrong_operator = program.store.clone();
            let words = wrong_operator.payload(wrong_operator.data[source.instruction as usize].range()).unwrap();
            let opcode = words[0] as usize;
            wrong_operator.binary_ops[opcode] = BinaryOp::Lt;
            assert!(FullVerifier::verify_generic_evidence(&wrong_operator).unwrap_err().message.contains("encoded operator"));
        });
    }

    #[test]
    fn selected_string_ordering_and_equality_require_prepared_source_proofs() {
        for (operator, opcode) in [("<", BinaryOp::Lt), ("<=", BinaryOp::Le), (">", BinaryOp::Gt), (">=", BinaryOp::Ge), ("==", BinaryOp::Eq), ("!=", BinaryOp::Ne)] {
            let source = format!("pure compare(left: Str, right: Str) -> Bool {{ left {operator} right }}\nlet comparison = compare(\"first\", \"second\")\n");
            let expected = if matches!(opcode, BinaryOp::Eq | BinaryOp::Ne) { PreparedLanguageOperation::Equality { op: opcode } }
                else { PreparedLanguageOperation::Ordering { op: opcode, left: crate::sema::inference::Atom::Str, right: crate::sema::inference::Atom::Str } };
            let program = source_fixture(&source, expected);
            assert!(program.generic_evidence().is_some(), "selected string {operator} has no prepared proof store");
            assert_eq!(program.generic_evidence().unwrap().operations().count(), 1, "selected string {operator} loses its source proof");
        }
    }

    #[test]
    fn selected_primitive_operations_reject_missing_or_rewritten_operand_result_and_effect_proofs() {
        for (source, expected) in [
            ("pure subtract(left: Int, right: Int) -> Int { left - right }\nlet value = subtract(7, 2)\n", PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } }),
            ("pure compare(left: Str, right: Str) -> Bool { left < right }\nlet value = compare(\"first\", \"second\")\n", PreparedLanguageOperation::Ordering { op: BinaryOp::Lt, left: Atom::Str, right: Atom::Str }),
            ("pure compare(left: Str, right: Str) -> Bool { left == right }\nlet value = compare(\"first\", \"second\")\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq }),
        ] {
            let program = source_fixture(source, expected);
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let (id, operation) = generic.operations().next().unwrap();
                let instruction = generic.operation_source(operation.source).unwrap().instruction;
                let mut missing = program.store.clone();
                missing.generic.as_deref_mut().unwrap().test_remove_operations();
                assert!(FullVerifier::verify_generic_evidence(&missing).unwrap_err().message.contains("missing its prepared proof"));
                let mut swapped = program.store.clone();
                swapped.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.operands.swap(0, 1);
                assert!(FullVerifier::verify_generic_evidence(&swapped).unwrap_err().message.contains("operand origins"));
                let mut wrong_result = program.store.clone();
                let result = SemanticPoolBuilder::default().intern_type(&mut wrong_result.semantic, &Type::Str).unwrap();
                wrong_result.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().result = TypeRef::Ground(result);
                assert!(FullVerifier::verify_generic_evidence(&wrong_result).unwrap_err().message.contains("result domain"));
                let mut wrong_argument = program.store.clone();
                let argument = SemanticPoolBuilder::default().intern_type(&mut wrong_argument.semantic, &Type::Bool).unwrap();
                wrong_argument.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().arguments[0] = Some(TypeRef::Ground(argument));
                assert!(FullVerifier::verify_generic_evidence(&wrong_argument).unwrap_err().message.contains("operand domain"));
                let mut wrong_effects = program.store.clone();
                wrong_effects.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().effects.creation = crate::sema::inference::EffectSet::TIME;
                assert!(FullVerifier::verify_generic_evidence(&wrong_effects).unwrap_err().message.contains("contract is inconsistent"));
                let mut rewritten = program.store.clone();
                let words = rewritten.payload(rewritten.data[instruction as usize].range()).unwrap();
                let opcode = words[0] as usize;
                rewritten.binary_ops[opcode] = BinaryOp::Add;
                let PreparedOperationAuthority::Language { operation, .. } = &mut rewritten.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().authority else { unreachable!() };
                *operation = PreparedLanguageOperation::Arithmetic { op: BinaryOp::Add, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } };
                assert!(FullVerifier::verify_generic_evidence(&rewritten).unwrap_err().message.contains("rewrites its original selected source contract"));
            });
        }
    }

    #[test]
    fn selected_source_operation_requires_prepared_instruction_proof() {
        let program = fixture();
        assert!(program.generic_evidence().is_some(), "selected source operation has no prepared proof store");
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let operations = generic.operations().collect::<Vec<_>>();
            assert_eq!(operations.len(), 1);
            assert_eq!(generic.operation_sources().count(), 1);
            let (_, operation) = operations[0];
            let source = generic.operation_source(operation.source).unwrap();
            assert!(generic.operation_at(source.instruction).unwrap().is_some());
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_operations();
            assert!(FullVerifier::verify_generic_evidence(&missing).unwrap_err().message.contains("missing its prepared proof"));
        });
    }

    #[test]
    fn selected_source_operation_rejects_wrong_instruction_operands_binding_and_owner() {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, operation) = generic.operations().next().unwrap();
            let source = generic.operation_source(operation.source).unwrap();
            let mut wrong_tag = program.store.clone();
            wrong_tag.tags[source.instruction as usize] = FullTag::ExprFloat;
            assert!(FullVerifier::verify_generic_evidence(&wrong_tag).unwrap_err().message.contains("another instruction kind"));
            let mut wrong_operator = program.store.clone();
            let words = wrong_operator.payload(wrong_operator.data[source.instruction as usize].range()).unwrap();
            let opcode = words[0] as usize;
            wrong_operator.binary_ops[opcode] = BinaryOp::Add;
            assert!(FullVerifier::verify_generic_evidence(&wrong_operator).unwrap_err().message.contains("encoded operator"));
            let mut wrong_operand = program.store.clone();
            let proof = wrong_operand.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap();
            proof.binding.operands[0] = proof.binding.operands[1];
            assert!(FullVerifier::verify_generic_evidence(&wrong_operand).unwrap_err().message.contains("operand origins"));
            let mut wrong_binding = program.store.clone();
            wrong_binding.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.supplied_slots.swap(0, 1);
            assert!(FullVerifier::verify_generic_evidence(&wrong_binding).unwrap_err().message.contains("contract is inconsistent"));
            let mut wrong_owner = program.store.clone();
            wrong_owner.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify_generic_evidence(&wrong_owner).unwrap_err().message.contains("original instruction"));
            let mut outsider = GenericEvidenceBuilder::default();
            let foreign_scope = outsider.add_scope(super::super::super::generic::SchemeScope {
                owner: IrFunctionId::new(0).unwrap(), quantifiers: Box::new([]), parameters: Box::new([]),
                parameter_names: Box::new([]), parameter_flags: Box::new([]), kind: super::super::super::generic::CallableKind::Pure,
                result: operation.result, requirements: Box::new([]), return_plan: GenericReturnPlan::Value,
            }).unwrap();
            let mut wrong_scope = program.store.clone();
            wrong_scope.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().scope = Some(foreign_scope);
            assert!(FullVerifier::verify_generic_evidence(&wrong_scope).unwrap_err().message.contains("scope disagrees"));
            let mut wrong_effects = program.store.clone();
            wrong_effects.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().effects.creation = crate::sema::inference::EffectSet(1);
            assert!(FullVerifier::verify_generic_evidence(&wrong_effects).unwrap_err().message.contains("contract is inconsistent"));
            let mut misplaced = program.store.clone();
            misplaced.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().instruction = operation.binding.operands[0];
            assert!(FullVerifier::verify_generic_evidence(&misplaced).is_err());
            let mut wrong_origin = program.store.clone();
            let OperationSourceOrigin::Expression(mut expression) = source.origin else { unreachable!() };
            expression.source = SourceId::new(123);
            wrong_origin.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().origin = OperationSourceOrigin::Expression(expression);
            assert!(FullVerifier::verify_generic_evidence(&wrong_origin).unwrap_err().message.contains("original instruction"));
            let mut wrong_location = program.store.clone();
            let words = wrong_location.payload(wrong_location.data[source.instruction as usize].range()).unwrap();
            let location = words[3] as usize;
            wrong_location.location_sources[location] = SourceId::new(123);
            assert!(FullVerifier::verify_generic_evidence(&wrong_location).unwrap_err().message.contains("encoded source location"));
        });
    }

    #[test]
    fn selected_source_operation_cannot_rewrite_proof_and_instruction_together() {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, operation) = generic.operations().next().unwrap();
            let instruction = generic.operation_source(operation.source).unwrap().instruction;
            let mut rewritten = program.store.clone();
            let words = rewritten.payload(rewritten.data[instruction as usize].range()).unwrap();
            let opcode = words[0] as usize;
            rewritten.binary_ops[opcode] = BinaryOp::Add;
            let PreparedOperationAuthority::Language { operation, .. } = &mut rewritten.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().authority else { unreachable!() };
            *operation = PreparedLanguageOperation::Arithmetic { op: BinaryOp::Add, domain: ArithmeticDomain::Float };
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "rewritten execution cannot replace the original selected source contract");
        });
    }

    #[test]
    fn selected_source_operation_ids_cannot_cross_programs_or_rewind() {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, operation) = generic.operations().next().unwrap();
            let source = generic.operation_source(operation.source).unwrap().clone();
            let mut builder = GenericEvidenceBuilder::default();
            builder.register_instruction_origin(source.instruction, source.origin, source.owner).unwrap();
            let checkpoint = builder.checkpoint();
            let old_source = builder.add_operation_source(source.clone()).unwrap();
            let mut proof = operation.clone(); proof.source = old_source;
            let old_operation = builder.add_operation(proof.clone()).unwrap();
            let retired_checkpoint = builder.checkpoint();
            builder.rewind(checkpoint).unwrap();
            let current_source = builder.add_operation_source(source.clone()).unwrap();
            proof.source = current_source;
            let current_operation = builder.add_operation(proof.clone()).unwrap();
            assert!(builder.rewind(retired_checkpoint).is_err());
            let owners = program.store.generic_instruction_owners().unwrap();
            let store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
            assert!(store.operation_source(old_source).is_err());
            assert!(store.operation(old_operation).is_err());
            assert!(store.operation(current_operation).is_ok());
            assert!(store.operation_source(operation.source).is_err());
            let mut foreign = program.store.clone();
            let id = generic.operations().next().unwrap().0;
            foreign.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = current_source;
            assert!(FullVerifier::verify_generic_evidence(&foreign).unwrap_err().message.contains("foreign program"));
            let mut stale_builder = GenericEvidenceBuilder::default();
            stale_builder.register_instruction_origin(source.instruction, source.origin, source.owner).unwrap();
            let checkpoint = stale_builder.checkpoint();
            let stale = stale_builder.add_operation_source(source.clone()).unwrap();
            stale_builder.rewind(checkpoint).unwrap();
            stale_builder.add_operation_source(source).unwrap();
            proof.source = stale;
            stale_builder.add_operation(proof).unwrap();
            assert!(stale_builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap_err().message.contains("retired by rewind"));
        });
    }
}
