use super::*;
use crate::runtime::eval::lower::hash_arguments::BuildHashPolicyPacket;
use super::super::super::generic::PreparedHashPolicyCall;

fn hash_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

fn hash_allocation(store: &FullStore, instruction: u32) -> Result<(Option<IrFunctionId>, Box<[u32]>, (u32, Box<[u32]>), [u32; 3]), IrVerifyError> {
    let payload = store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("hash policy instruction missing"))?.range())?;
    let (target, block) = match store.tags.get(instruction as usize) {
        Some(FullTag::ExprCall) => (Some(IrFunctionId::from_raw(*payload.first().ok_or_else(|| IrVerifyError::new("hash policy target missing"))?).ok_or_else(|| IrVerifyError::new("hash policy target invalid"))?), *payload.get(1).ok_or_else(|| IrVerifyError::new("hash policy arguments missing"))?),
        Some(FullTag::ExprExternalCall) => (None, *payload.get(2).ok_or_else(|| IrVerifyError::new("hash policy external arguments missing"))?),
        _ => return Err(IrVerifyError::new("hash policy implementation opcode changed")),
    };
    let body = IrBlockId::from_raw(block).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("hash policy block invalid"))?;
    if body.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("hash policy argument block kind changed")); }
    let arguments = store.payload(body.instructions)?;
    if arguments.len() != 7 || arguments[0] != 3 || [arguments[1], arguments[3], arguments[5]] != [0, 0, 0] { return Err(IrVerifyError::new("hash policy implementation requires three supplied values")); }
    Ok((target, payload.into(), (block, arguments.into()), [arguments[2], arguments[4], arguments[6]]))
}

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn stage_hash_policy_packet(&mut self, expression: BuildExprId, instruction: u32, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.hash_policy_packets.get(&expression) else { return Ok(()); };
        let operands = [original.public_operands[0], original.public_operands[1], original.algorithm_operand].map(|value| self.active_encoded_expressions.get(&value).copied());
        let [Some(path), Some(checksum), Some(algorithm)] = operands else { return Err(hash_problem("hash_policy_original_operand_missing")); };
        let (_, _, _, actual) = hash_allocation(&self.store, instruction).map_err(|_| hash_problem("hash_policy_original_allocation_changed"))?;
        if actual != [path, checksum, algorithm] { return Err(hash_problem("hash_policy_original_packet_changed")); }
        self.hash_policy_rows.push((instruction, owner, original.clone(), actual));
        match self.active_expression_origins.get(&expression) {
            Some(origin) if *origin != original.origin => return Err(hash_problem("hash_policy_original_instruction_origin_changed")),
            Some(_) => {},
            None => self.generic_expression_rows.push((instruction, original.origin, owner)),
        }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn prepare_hash_policy_calls(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        for (instruction, owner, original, operands) in self.hash_policy_rows.clone() {
            let operation = solved.operations.get(&original.origin).ok_or_else(|| hash_problem("hash_policy_original_operation_missing"))?;
            let selected = solved.graph.candidate_evidence(operation.requirement).map_err(|_| hash_problem("hash_policy_original_candidate"))?.ok_or_else(|| hash_problem("hash_policy_original_candidate_missing"))?;
            let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| hash_problem("hash_policy_original_authority"))? else { return Err(hash_problem("hash_policy_original_authority")); };
            let boundary = solved.registry_boundaries.get(&original.origin).ok_or_else(|| hash_problem("hash_policy_original_boundary_missing"))?;
            if metadata.operation != RuntimeOp::HashVerifyFile || boundary.hash_algorithm() != Some(original.algorithm)
                || boundary.requirement != Some(operation.requirement) || operation.caller != original.caller
                || boundary.caller != original.caller || operation.binding.supplied_slots != [0, 1]
                || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() { return Err(hash_problem("hash_policy_original_contract_changed")); }
            let (kind, signature) = self.intern_checked_callable_signature(&solved.graph, selected.signature)?;
            let definition = solved.declarations.get(&original.implementation).ok_or_else(|| hash_problem("hash_policy_original_implementation_missing"))?;
            let (implementation_kind, implementation_signature) = self.intern_checked_callable_signature(&solved.graph, definition.signature)?;
            if kind != super::super::super::generic::CallableKind::Proc || implementation_kind != kind { return Err(hash_problem("hash_policy_original_effectful_callable_kind")); }
            let (target, payload, argument_block, actual) = hash_allocation(&self.store, instruction).map_err(|_| hash_problem("hash_policy_original_allocation_changed"))?;
            if actual != operands || target.is_some_and(|target| self.declaration_functions.get(&original.implementation) != Some(&target)) { return Err(hash_problem("hash_policy_original_target_changed")); }
            if target.is_none() {
                let namespace = Name::from_symbol(Symbol::from_raw(payload[0]));
                let member = Name::from_symbol(Symbol::from_raw(payload[1]));
                if namespace != original.implementation_key.namespace || member != original.implementation_key.member { return Err(hash_problem("hash_policy_original_external_target_changed")); }
            }
            let recipes = solved.argument_sources.get(&original.origin).ok_or_else(|| hash_problem("hash_policy_original_recipes_missing"))?;
            if recipes.len() != 2 || operation.actual_arguments.len() != 2 { return Err(hash_problem("hash_policy_original_public_arity")); }
            let mut arguments = Vec::with_capacity(2);
            for ordinal in 0..2 {
                self.original_argument_expression(operands[ordinal], original.origin, ordinal, &recipes[ordinal], owner)?;
                let ty = graph_ground_type(&solved.graph, operation.actual_arguments[ordinal]).map_err(|_| hash_problem("hash_policy_original_argument_type"))?;
                arguments.push(PreparedInvocationArgument { original: recipes[ordinal].clone(), instruction: operands[ordinal], ty: TypeRef::Ground(self.intern_generic_ground_type(&ty)?) });
            }
            let argument_lineages = self.prepare_native_argument_lineages(&mut arguments, owner, signature, &[0, 1])?;
            let algorithm_payload = self.store.payload(self.store.data[operands[2] as usize].range()).map_err(|_| hash_problem("hash_policy_original_algorithm_payload"))?.into();
            if self.store.tags.get(operands[2] as usize) != Some(&FullTag::ExprStr) { return Err(hash_problem("hash_policy_original_algorithm_opcode")); }
            let authority = PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding, argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() };
            self.generic_evidence_mut().add_hash_policy_call(PreparedHashPolicyCall { origin: original.origin, instruction, owner, caller: original.caller, implementation: original.implementation, target, authority, signature, implementation_signature, arguments: arguments.into_boxed_slice(), argument_lineages, algorithm: original.algorithm, algorithm_operand: operands[2], algorithm_payload, payload, argument_block }).map_err(|_| hash_problem("hash_policy_receipt_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed::full) fn hash_policy_result(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Type, IrVerifyError> {
        let source = generic.hash_policy_call(instruction)?.ok_or_else(|| IrVerifyError::new("hash policy is missing its original invocation receipt"))?;
        if source.arguments.len() != 2 || source.argument_lineages.len() != 2 { return Err(IrVerifyError::new("hash policy loses its two original public arguments")); }
        let (target, payload, argument_block, operands) = hash_allocation(store, instruction)?;
        if source.owner != owner || target != source.target || payload != source.payload || argument_block != source.argument_block
            || operands != [source.arguments[0].instruction, source.arguments[1].instruction, source.algorithm_operand]
            || store.tags.get(source.algorithm_operand as usize) != Some(&FullTag::ExprStr) { return Err(IrVerifyError::new("hash policy allocation changes its original target or operands")); }
        let algorithm = store.payload(store.data[source.algorithm_operand as usize].range())?;
        if algorithm != source.algorithm_payload.as_ref() || algorithm.len() != 1 || store.string(algorithm[0])? != source.algorithm.as_str().as_str() { return Err(IrVerifyError::new("hash policy algorithm changes its original source selector")); }
        for (argument, lineage) in source.arguments.iter().zip(&source.argument_lineages) {
            Self::verify_argument_initializer_lineage(store, generic, argument.instruction, lineage.source_instruction, &lineage.wrappers, owner)?;
            let TypeRef::Ground(ty) = argument.ty else { return Err(IrVerifyError::new("hash policy argument needs its original ground type")); };
            Self::verify_generic_source(store, generic, argument.instruction, owner, &store.semantic.to_type(ty)?, None, &mut Vec::new())?;
        }
        Ok(store.semantic.to_type(store.semantic.signature_return_type(source.signature)?)?)
    }
    pub(in crate::runtime::eval::indexed::full) fn verify_hash_policy_calls(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for source in generic.hash_policy_calls() { Self::hash_policy_result(store, generic, source.instruction, source.owner)?; }
        Ok(())
    }
    pub(in crate::runtime::eval::indexed::full) fn verify_hash_policy_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        if generic.hash_policy_call(instruction)?.is_none() { return Ok(false); }
        if Self::hash_policy_result(store, generic, instruction, owner)? != *expected { return Err(IrVerifyError::new("hash policy result differs from its consumer")); }
        Ok(true)
    }
}

impl FullExecution<'_> {
    pub(in crate::runtime::eval) fn verify_hash_policy_call(&self, instruction: u32) -> Result<(), IrVerifyError> {
        self.decoder.store.verify_generic_owner()?;
        let Some(generic) = self.generic_evidence() else { return Ok(()); };
        if !generic.hash_policy_instruction_originally_prepared(instruction)? { return Ok(()); }
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("hash policy call belongs to another execution body")); }
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("hash policy call owner is invalid"))?) };
        FullVerifier::hash_policy_result(self.decoder.store, generic, instruction, owner)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_fixture(algorithm: &str) -> FullProgram {
        let source = format!("proc selected(file: Path) [fs] -> Result[Unit] {{ hash.verify_file(file, {algorithm}: \"00\") }}\n");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("hash-policy.xsh", crate::loader::entry_source_from_text("hash-policy.xsh", source.clone()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let symbols = parsed.arena.symbol_owner().clone();
        symbols.with_current(|| {
            let checked = crate::sema::check::Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let graph = Arc::downgrade(&checked.solved);
            let entry = sources.files().first().unwrap().id();
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
            evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, entry, &checked).unwrap();
            let program = (**evaluator.indexed_program.as_ref().unwrap()).clone();
            drop(evaluator); drop(checked); drop(parsed);
            assert!(graph.upgrade().is_none(), "the policy receipt does not retain the frontend graph");
            FullVerifier::verify(&program).unwrap();
            program
        })
    }

    #[test]
    fn original_hash_policy_survives_frontend_disposal_and_both_workers_observe_the_algorithm() {
        crate::runtime::eval::run_eval(|| {
            use crate::runtime::eval::{Evaluator, LoweredFunctionKey, LoweredFunctionKind};
            use crate::runtime::value::{PathValue, ResultValue, Value};
            let temp = tempfile::tempdir().unwrap();
            let file = temp.path().join("digest-input");
            std::fs::write(&file, b"checked-policy").unwrap();
            for (algorithm, length) in [("md5", 32), ("sha1", 40), ("sha256", 64), ("sha512", 128)] {
                let program = Arc::new(hash_fixture(algorithm));
                program.symbol_owner().with_current(|| {
                    for recursive in [false, true] {
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                        evaluator.indexed_program = Some(program.clone());
                        let key = LoweredFunctionKey::Name(Name::intern("selected"));
                        let argument = Value::Path(PathValue::new(file.to_str().unwrap().as_bytes().to_vec()).unwrap());
                        let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[argument], Span::at(program.store.source_id, 0)).unwrap();
                        let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
                        let Value::Result(ResultValue::Err(error)) = value else { panic!("the short checksum is an ordinary declared error"); };
                        assert_eq!(error.error_message().unwrap(), format!("{algorithm} checksum must be {length} hex characters"));
                    }
                });
            }
        });
    }

    #[test]
    fn original_hash_policy_refuses_missing_foreign_and_jointly_rewritten_packets() {
        crate::runtime::eval::run_eval(|| {
            let program = hash_fixture("sha512");
            let foreign = hash_fixture("sha512");
            program.symbol_owner().with_current(|| {
                let source = program.generic_evidence().unwrap().hash_policy_calls().next().unwrap();
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_hash_policy_calls();
                assert!(FullVerifier::verify(&missing).is_err());
                let mut substituted = program.clone();
                substituted.store.generic.as_deref_mut().unwrap().test_replace_hash_policy_calls(foreign.generic_evidence().unwrap());
                assert!(FullVerifier::verify(&substituted).is_err());
                let mut changed = program.clone();
                let literal = changed.store.data[source.algorithm_operand as usize].range();
                let alternate = (0..changed.store.strings.len()).map(|index| IrStringId::new(index).unwrap().raw()).find(|&raw| changed.store.string(raw).unwrap() == "md5").unwrap();
                changed.store.extra[literal.start as usize] = alternate;
                assert!(FullVerifier::verify(&changed).is_err());
                let receipt = changed.store.generic.as_deref_mut().unwrap().test_hash_policy_call_mut(source.instruction);
                receipt.algorithm = Name::intern("md5");
                receipt.algorithm_payload[0] = alternate;
                assert!(FullVerifier::verify(&changed).is_err(), "matching physical and semantic rewrites cannot replace the original source policy");
                for altered in [missing, substituted, changed] {
                    for recursive in [false, true] {
                        use crate::runtime::eval::{Evaluator, LoweredFunctionKey, LoweredFunctionKind};
                        use crate::runtime::value::{PathValue, Value};
                        let altered = Arc::new(altered.clone());
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*altered.sources).clone());
                        evaluator.indexed_program = Some(altered.clone());
                        let key = LoweredFunctionKey::Name(Name::intern("selected"));
                        let argument = Value::Path(PathValue::new(b"missing-policy-refusal-file".to_vec()).unwrap());
                        let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[argument], Span::at(altered.store.source_id, 0)).unwrap();
                        let error = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err();
                        assert_eq!(error.kind, "indexed-ir", "the policy receipt refuses before native file hashing");
                    }
                }
            });
        });
    }

    #[test]
    fn original_hash_policy_refuses_missing_boundary_changed_public_absence_and_foreign_caller() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc selected(file: Path) [fs] -> Result[Unit] { hash.verify_file(file, sha256: \"00\") }\n";
            for control in 0..3 {
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("hash-authority.xsh", crate::loader::entry_source_from_text("hash-authority.xsh", source.to_owned()), Vec::new());
                assert!(parsed.diagnostics.is_empty());
                let symbols = parsed.arena.symbol_owner().clone();
                symbols.with_current(|| {
                    let mut checked = crate::sema::check::Checker::check_arena(&parsed.arena, source);
                    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                    let origin = *checked.solved.registry_boundaries.iter().find(|(_, boundary)| boundary.hash_algorithm().is_some()).unwrap().0;
                    let solved = Arc::get_mut(&mut checked.solved).unwrap();
                    match control {
                        0 => { solved.registry_boundaries.remove(&origin); }
                        1 => { solved.operations.get_mut(&origin).unwrap().binding.default_slots.push(1); }
                        _ => { solved.registry_boundaries.get_mut(&origin).unwrap().caller = None; }
                    }
                    let entry = sources.files().first().unwrap().id();
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                    assert!(evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, entry, &checked).is_err(), "rewritten original Hash policy cannot re-enter a legacy argument rebinder");
                });
            }
        });
    }

    #[test]
    fn original_hash_algorithm_is_a_third_implementation_operand_without_a_public_default() {
        crate::runtime::eval::run_eval(|| {
            for algorithm in ["md5", "sha1", "sha256", "sha512"] {
                let source = format!("let outcome = hash.verify_file(path: p\"missing-hash-packet-file\", {algorithm}: \"00\")\n");
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("hash-packet.xsh", crate::loader::entry_source_from_text("hash-packet.xsh", source.clone()), Vec::new());
                assert!(parsed.diagnostics.is_empty());
                let symbols = parsed.arena.symbol_owner().clone();
                symbols.with_current(|| {
                    let checked = crate::sema::check::Checker::check_arena(&parsed.arena, &source);
                    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                    let (origin, boundary) = checked.solved.registry_boundaries.iter().find(|(_, boundary)| boundary.hash_algorithm().is_some()).unwrap();
                    let operation = &checked.solved.operations[origin];
                    assert_eq!(operation.binding.supplied_slots, [0, 1]);
                    assert!(operation.binding.default_slots.is_empty());
                    assert_eq!(operation.actual_arguments.len(), 2);
                    let selected = boundary.hash_algorithm().unwrap();
                    assert_eq!(selected, algorithm);
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                    let source_id = origin.source;
                    evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                    let program = evaluator.indexed_program.as_ref().unwrap();
                    let call = program.store.tags.iter().enumerate().find(|(instruction, tag)| {
                        if **tag != FullTag::ExprCall { return false; }
                        let words = program.store.payload(program.store.data[*instruction].range()).unwrap();
                        let target = IrFunctionId::from_raw(words[0]).unwrap();
                        program.store.string(program.store.functions[target.index()].name).unwrap() == "verify_file"
                    }).unwrap().0;
                    let words = program.store.payload(program.store.data[call].range()).unwrap();
                    let target = IrFunctionId::from_raw(words[0]).unwrap();
                    let parameters = program.store.functions[target.index()].params.bounds(program.store.params.len()).unwrap();
                    assert_eq!(parameters.len(), 3);
                    let block = &program.store.blocks[IrBlockId::from_raw(words[1]).unwrap().index()];
                    let arguments = program.store.payload(block.instructions).unwrap();
                    assert_eq!(arguments[0], 3, "the original named algorithm is present in the actual implementation packet");
                    assert_eq!(arguments.len(), 7);
                    assert_eq!([arguments[1], arguments[3], arguments[5]], [0, 0, 0]);
                    let algorithm_operand = arguments[6] as usize;
                    assert_eq!(program.store.tags[algorithm_operand], FullTag::ExprStr);
                    let literal = program.store.payload(program.store.data[algorithm_operand].range()).unwrap();
                    assert_eq!(program.store.string(literal[0]).unwrap(), algorithm);
                });
            }
        });
    }
}
