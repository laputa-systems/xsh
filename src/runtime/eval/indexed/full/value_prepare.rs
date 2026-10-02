use super::*;
use super::super::generic::{PreparedValueBinding, ValueBindingContract, ValueBindingSource, ValueBindingUse, ValueInitializerWrapper, ValueInitializerWrapperKind, graph_ground_type};
use super::callable_prepare::CallableLexicalIndex;

fn value_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

fn value_owner(raw: Option<u32>) -> Result<InstructionOwner, IrBuildError> {
    let raw = raw.ok_or_else(|| value_problem("value_binding_owner_missing"))?;
    if let Some(index) = driver_owner_index(raw) { return Ok(InstructionOwner::Driver(index as u32)); }
    IrFunctionId::from_raw(raw).map(InstructionOwner::Function).ok_or_else(|| value_problem("value_binding_owner_invalid"))
}

impl FullBuilder {
    pub(super) fn stage_value_expression_use(&mut self, instruction: u32, expression: BuildExprId, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if self.store.tags[instruction as usize] != FullTag::ExprParam { return Ok(()); }
        let Some(&origin) = self.active_expression_origins.get(&expression) else { return Ok(()); };
        let Some(&binding) = scratch.value_binding_uses.get(&origin) else { return Ok(()); };
        if !scratch.value_binding_origins.contains_key(&binding) { return Err(value_problem("value_read_original_binding_missing")); }
        self.value_use_rows.push((origin, binding, instruction, value_owner(self.current_owner)?));
        Ok(())
    }

    pub(super) fn stage_value_statement_binding(&mut self, row: BuildStmtId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some((binding, original)) = self.active_value_bindings.get(&row).cloned() else { return Ok(()); };
        let Some(BuildStmtRow::Let { slot, value }) = scratch.statements.get(row.index()) else { return Err(value_problem("value_binding_statement_changed")); };
        if *slot != original.slot || *value != original.initializer { return Err(value_problem("value_binding_allocation_changed")); }
        let initializer = *self.active_encoded_expressions.get(value).ok_or_else(|| value_problem("value_binding_initializer_missing"))?;
        self.value_binding_rows.push((binding, original, instruction, initializer, value_owner(self.current_owner)?));
        Ok(())
    }

    pub(super) fn prepare_value_bindings(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let mut applications = BTreeMap::new();
        let saved_wrappers: BTreeMap<_, _> = self.prepared_saved_argument_bindings.values().map(|saved| (saved.wrapper, saved.clone())).collect();
        for (binding, original, instruction, initializer, owner) in self.value_binding_rows.clone() {
            solved.graph.validate_scoped(original.source_type).map_err(|_| value_problem("value_binding_original_scope"))?;
            solved.graph.validate_scoped(original.initializer_type).map_err(|_| value_problem("value_initializer_original_scope"))?;
            let definition = solved.bindings.get(&binding).ok_or_else(|| value_problem("value_binding_original_definition_missing"))?;
            let lexical = definition.owner.and_then(|owner| solved.declarations.get(&owner).map(|declaration| declaration.scheme));
            let initializer_scope = solved.expression_scope(original.initializer_source, definition.owner).map_err(|_| value_problem("value_initializer_original_scope"))?;
            if definition.mutable || definition.ty != original.source_type.ty
                || original.source_type.scope != definition.scheme.or(lexical) || original.initializer_type.scope != initializer_scope
                || solved.expressions.get(&original.initializer_source) != Some(&original.initializer_type.ty)
                || solved.expression_owners.get(&original.initializer_source).copied() != definition.owner {
                return Err(value_problem("value_binding_original_definition_changed"));
            }
            let binding_type = graph_ground_type(&solved.graph, original.source_type.ty).map_err(|_| value_problem("value_binding_requires_scope"))?;
            let initializer_type = graph_ground_type(&solved.graph, original.initializer_type.ty).map_err(|_| value_problem("value_initializer_requires_scope"))?;
            let binding_type = self.intern_generic_ground_type(&binding_type)?;
            let initializer_type = self.intern_generic_ground_type(&initializer_type)?;
            let scope = definition.owner.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let range = match owner { InstructionOwner::Function(function) => self.store.function_instruction_range(function.index()), InstructionOwner::Driver(driver) => self.store.driver_instruction_range(driver as usize) }.map_err(|_| value_problem("value_initializer_owner_range"))?;
            let mut initializer_source_instruction = initializer;
            let mut wrappers = Vec::new();
            loop {
                let tag = self.store.tags.get(initializer_source_instruction as usize);
                let saved = saved_wrappers.get(&initializer_source_instruction);
                if tag != Some(&FullTag::ExprCheckedValue) && saved.is_none() { break; }
                if !range.contains(&(initializer_source_instruction as usize)) || wrappers.len() >= 256
                    || wrappers.iter().any(|wrapper: &ValueInitializerWrapper| wrapper.instruction == initializer_source_instruction) { return Err(value_problem("value_initializer_wrapper_depth_or_owner")); }
                let payload = self.store.payload(self.store.data[initializer_source_instruction as usize].range()).map_err(|_| value_problem("value_initializer_wrapper_payload"))?.to_vec().into_boxed_slice();
                let (child, kind) = if tag == Some(&FullTag::ExprCheckedValue) {
                    (*payload.first().ok_or_else(|| value_problem("value_initializer_wrapper_empty"))?, ValueInitializerWrapperKind::CheckedValue)
                } else {
                    let saved = saved.ok_or_else(|| value_problem("value_initializer_saved_receipt_missing"))?;
                    let (argument, pattern, body) = super::argument_prepare::saved_argument_wrapper(&self.store, initializer_source_instruction).map_err(|_| value_problem("value_initializer_saved_wrapper_changed"))?;
                    if saved.owner != owner || saved.call != original.initializer_source || saved.initializer != argument || saved.pattern != pattern { return Err(value_problem("value_initializer_saved_source_changed")); }
                    (body, ValueInitializerWrapperKind::SavedArgument { call: saved.call, initializer: argument, pattern, body })
                };
                wrappers.push(ValueInitializerWrapper { instruction: initializer_source_instruction, payload, kind });
                initializer_source_instruction = child;
            }
            if !range.contains(&(initializer_source_instruction as usize)) { return Err(value_problem("value_initializer_source_owner")); }
            let contract = ValueBindingContract { instruction, owner, slot: u32::try_from(original.slot).map_err(|_| value_problem("value_binding_slot_overflow"))?, initializer,
                initializer_source_instruction, initializer_wrappers: wrappers.into_boxed_slice(), binding_type, initializer_type, scope };
            let source = self.generic_evidence_mut().add_value_binding_source(ValueBindingSource { binding, statement: original.statement, initializer_source: original.initializer_source,
                source_type: original.source_type, initializer_type: original.initializer_type, expected: contract.clone() }).map_err(|_| value_problem("value_binding_source_allocation"))?;
            let application = self.generic_evidence_mut().add_value_binding(PreparedValueBinding { source, contract }).map_err(|_| value_problem("value_binding_application_allocation"))?;
            if applications.insert(binding, application).is_some() { return Err(value_problem("value_binding_original_definition_duplicate")); }
        }
        for (origin, binding, instruction, owner) in self.value_use_rows.clone() {
            let application = *applications.get(&binding).ok_or_else(|| value_problem("value_read_original_application_missing"))?;
            self.generic_evidence_mut().add_value_binding_use(ValueBindingUse { origin, application, instruction, owner }).map_err(|_| value_problem("value_binding_read_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    fn verify_value_initializer_lineage(store: &FullStore, generic: &GenericEvidenceStore, contract: &ValueBindingContract) -> Result<(), IrVerifyError> {
        let mut instruction = contract.initializer;
        let mut seen = std::collections::BTreeSet::new();
        for wrapper in contract.initializer_wrappers.iter() {
            if instruction != wrapper.instruction || !seen.insert(instruction)
                || store.payload(store.data[instruction as usize].range())? != wrapper.payload.as_ref() {
                return Err(IrVerifyError::new("value initializer changes its original checked wrapper"));
            }
            instruction = match wrapper.kind {
                ValueInitializerWrapperKind::CheckedValue => {
                    if store.tags.get(instruction as usize) != Some(&FullTag::ExprCheckedValue) { return Err(IrVerifyError::new("value initializer changes its checked wrapper kind")); }
                    *wrapper.payload.first().ok_or_else(|| IrVerifyError::new("value initializer wrapper is empty"))?
                }
                ValueInitializerWrapperKind::SavedArgument { call, initializer, pattern, body } => {
                    let saved = generic.original_argument_wrapper(instruction).ok_or_else(|| IrVerifyError::new("value initializer lacks its original argument receipt"))?;
                    let actual = super::argument_prepare::saved_argument_wrapper(store, instruction)?;
                    if saved.owner != contract.owner || saved.call != call || saved.initializer != initializer || saved.pattern != pattern || actual != (initializer, pattern, body) { return Err(IrVerifyError::new("value initializer changes its original argument wrapper")); }
                    body
                }
            };
        }
        if instruction != contract.initializer_source_instruction || store.tags.get(instruction as usize) == Some(&FullTag::ExprCheckedValue) {
            return Err(IrVerifyError::new("value initializer loses its original material source"));
        }
        Ok(())
    }
    pub(super) fn verify_value_binding_dominance(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref().filter(|generic| generic.has_value_bindings()) else { return Ok(()); };
        let index = CallableLexicalIndex::new(store, tree)?;
        let owners = store.generic_instruction_owners()?;
        let mut writes = std::collections::BTreeSet::new();
        for (instruction, tag) in store.tags.iter().enumerate() {
            if matches!(tag, FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath) {
                if let (Some(owner), Some(&slot)) = (owners[instruction], store.payload(store.data[instruction].range())?.first()) {
                    let owner = match owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(step) => (true, step) };
                    writes.insert((owner, slot));
                }
            }
        }
        for (id, _) in generic.value_bindings() {
            let binding = generic.value_binding(id)?;
            let contract = &binding.contract;
            Self::verify_value_initializer_lineage(store, generic, contract)?;
            let owner = match contract.owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(step) => (true, step) };
            if writes.contains(&(owner, contract.slot)) { return Err(IrVerifyError::new("immutable value binding has an unprepared write")); }
            if store.tags.get(contract.instruction as usize) != Some(&FullTag::StmtLet)
                || store.payload(store.data[contract.instruction as usize].range())? != [contract.slot, contract.initializer] {
                return Err(IrVerifyError::new("value binding changes its original allocation or initializer"));
            }
        }
        for use_ in generic.value_binding_uses() {
            let use_ = generic.value_binding_use(use_.instruction)?.ok_or_else(|| IrVerifyError::new("original value read is missing"))?;
            let binding = generic.value_binding(use_.application)?;
            if store.tags.get(use_.instruction as usize) != Some(&FullTag::ExprParam)
                || store.payload(store.data[use_.instruction as usize].range())? != [binding.contract.slot]
                || !index.dominates(tree, binding.contract.instruction, use_.instruction)? {
                return Err(IrVerifyError::new("immutable value read is outside its original binding scope"));
            }
        }
        Ok(())
    }

    pub(super) fn verify_value_bindings(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (id, _) in generic.value_bindings() {
            let binding = generic.value_binding(id)?;
            Self::verify_value_initializer_lineage(store, generic, &binding.contract)?;
            Self::verify_generic_source(store, generic, binding.contract.initializer, binding.contract.owner,
                &store.semantic.to_type(binding.contract.initializer_type)?, None, &mut Vec::new())?;
        }
        Ok(())
    }

    pub(super) fn verify_value_binding_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(use_) = generic.value_binding_use(instruction)? else { return Ok(false); };
        let binding = generic.value_binding(use_.application)?;
        let contract = &binding.contract;
        Self::verify_value_initializer_lineage(store, generic, contract)?;
        if use_.owner != owner || contract.owner != owner || store.semantic.to_type(contract.binding_type)? != *expected {
            return Err(IrVerifyError::new("immutable value read changes its original owner or checked type"));
        }
        Self::verify_generic_source(store, generic, contract.initializer, owner, &store.semantic.to_type(contract.initializer_type)?, None, active)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::operation_prepare::tests::source_fixture;
    use crate::sema::operation_graph::PreparedLanguageOperation;
    use crate::sema::check::Checker;

    #[test]
    fn original_nested_immutable_native_result_binding_survives_frontend_disposal() {
        let source = "test nested_result [error] { |ctx| { let result = test.run_script(ctx, \"let value = 1\\n\")?; result.stdout == \"\" } }\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        assert_eq!(program.generic_evidence().unwrap().ground_native_calls().count(), 1);
        assert_eq!(program.generic_evidence().unwrap().ground_projections().count(), 1);
        let generic = program.generic_evidence().unwrap();
        assert_eq!(generic.value_bindings().count(), 1);
        assert_eq!(generic.value_binding_uses().count(), 1);
    }

    fn record_pair() -> FullProgram {
        source_fixture("pure compare(text: Str) -> Bool { let first = {text: text}; let second = {text: \"other\"}; first.text == second.text }\nlet result = compare(\"chosen\")\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
    }

    #[test]
    fn original_value_bindings_reject_missing_foreign_and_same_typed_coforged_initializers() {
        let program = record_pair();
        let foreign = record_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, application) = generic.value_bindings().next().unwrap();
            let (_, alternate) = generic.value_bindings().nth(1).unwrap();
            assert_eq!(application.contract.binding_type, alternate.contract.binding_type);
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_value_bindings();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut missing_reads = program.clone();
            missing_reads.store.generic.as_deref_mut().unwrap().test_remove_value_binding_uses();
            assert!(FullVerifier::verify(&missing_reads).is_err());
            let mut swapped = program.clone();
            let range = swapped.store.data[application.contract.instruction as usize].range();
            swapped.store.extra[range.start as usize + 1] = alternate.contract.initializer;
            assert!(FullVerifier::verify(&swapped).is_err());
            let evidence = swapped.store.generic.as_deref_mut().unwrap();
            evidence.test_value_binding_mut(id).unwrap().contract.initializer = alternate.contract.initializer;
            evidence.test_value_binding_source_mut(application.source).unwrap().expected.initializer = alternate.contract.initializer;
            let failure = FullVerifier::verify_generic_evidence(&swapped.store).unwrap_err();
            assert!(failure.message.contains("original receipt"), "{}", failure.message);
            let mut other_root = program.clone();
            other_root.store.generic.as_deref_mut().unwrap().test_value_binding_mut(id).unwrap().source = foreign.generic_evidence().unwrap().value_bindings().next().unwrap().1.source;
            assert!(FullVerifier::verify(&other_root).is_err());
        });
    }

    #[test]
    fn original_value_reads_reject_changed_owner_slot_source_and_lexical_visibility() {
        let program = record_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let use_ = generic.value_binding_uses().next().unwrap();
            let application = generic.value_binding(use_.application).unwrap();
            let (_, alternate) = generic.value_bindings().nth(1).unwrap();
            let mut changed_slot = program.clone();
            let range = changed_slot.store.data[use_.instruction as usize].range();
            changed_slot.store.extra[range.start as usize] = alternate.contract.slot;
            assert!(FullVerifier::verify(&changed_slot).is_err());
            let mut changed_owner = program.clone();
            changed_owner.store.generic.as_deref_mut().unwrap().test_value_binding_use_mut(use_.instruction).unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify(&changed_owner).is_err());
            let mut changed_source = program.clone();
            changed_source.store.generic.as_deref_mut().unwrap().test_value_binding_use_mut(use_.instruction).unwrap().origin.source = SourceId::new(99);
            assert!(FullVerifier::verify(&changed_source).is_err());
            let mut changed_scope = program.clone();
            let source = generic.value_binding_source(application.source).unwrap();
            let replacement = generic.value_binding_source(alternate.source).unwrap();
            changed_scope.store.generic.as_deref_mut().unwrap().test_value_binding_source_mut(application.source).unwrap().source_type.scope = replacement.initializer_type.scope;
            assert!(FullVerifier::verify(&changed_scope).is_err());
            let mut reordered = program.clone();
            let block = reordered.store.blocks.iter().find(|block| block.flags & BLOCK_SEQUENCE_KIND_MASK == BLOCK_STATEMENTS
                && reordered.store.payload(block.instructions).unwrap().contains(&application.contract.instruction)
                && reordered.store.payload(block.instructions).unwrap().contains(&alternate.contract.instruction)).unwrap().instructions;
            let words = reordered.store.payload(block).unwrap();
            let allocation = words.iter().position(|&word| word == application.contract.instruction).unwrap();
            let last = words.len() - 1;
            reordered.store.extra.swap(block.start as usize + allocation, block.start as usize + last);
            assert!(FullVerifier::verify(&reordered).is_err(), "a later allocation cannot dominate an earlier read merely because its numeric instruction ID is smaller");
            assert_eq!(source.binding.source, use_.origin.source);
        });
    }

    #[test]
    fn immutable_record_values_execute_original_lexical_bindings_after_frontend_disposal_on_both_routes() {
        use crate::runtime::eval::Evaluator;
        use crate::syntax::parser::Parser;
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(text: Str) -> Str { let record = {first: \"wrong\", text: text}; record.text }\nprint ${selected(\"chosen\")}\n";
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("nested-value-routes.xsh", source);
                let parsed = Parser::parse_source_arena_only(source_id, source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let symbols = parsed.arena.symbol_owner().clone();
                symbols.with_current(|| {
                    let checked = Checker::check_arena(&parsed.arena, source);
                    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                    let solved = Arc::downgrade(&checked.solved);
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                    let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                    let evidence = evaluator.indexed_program.as_deref().unwrap().generic_evidence().unwrap();
                    assert_eq!(evidence.value_bindings().count(), 1);
                    assert_eq!(evidence.value_binding_uses().count(), 1);
                    assert_eq!(evidence.ground_projections().count(), 1);
                    drop(checked); drop(parsed);
                    assert!(solved.upgrade().is_none(), "ordinary value execution cannot retain the inference bundle");
                    let execute = || evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the prepared value program remains installed"));
                    let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
                    assert_eq!(output.status, 0, "{:?}", output.diagnostics);
                    assert_eq!(output.stdout, b"chosen\n");
                    assert!(output.stderr.is_empty());
                    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                });
            }
        });
    }
}
