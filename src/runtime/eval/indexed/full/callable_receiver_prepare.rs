use super::*;
use super::super::generic::OriginalCallableReceiver;
use super::super::generic::{ConditionalBody, ConditionalKind, ConditionalResultSource, ConditionalTerminalValue, UserCallableContract};

enum ConditionalCallableAuthority {
    Creation(UserCallableContract),
    Branches(UserCallableContract),
}

impl ConditionalCallableAuthority {
    fn contract(&self) -> UserCallableContract {
        match self { Self::Creation(contract) | Self::Branches(contract) => *contract }
    }
}

fn callable_branch_value(body: &ConditionalBody) -> Option<&super::super::generic::ConditionalValue> {
    match body {
        ConditionalBody::Authored { value, terminal: None } => Some(value),
        ConditionalBody::Authored { terminal: Some((_, _, ConditionalTerminalValue::Expression(value))), .. } => Some(value),
        _ => None,
    }
}

fn original_conditional_callable_contract(
    source: &ConditionalResultSource,
    origin: crate::sema::check::ExpressionIdentity,
    owner: InstructionOwner,
    mut branch: impl FnMut(u32, crate::sema::check::ExpressionIdentity, InstructionOwner) -> Result<ConditionalCallableAuthority, IrVerifyError>,
) -> Result<UserCallableContract, IrVerifyError> {
    let invalid = || IrVerifyError::new("conditional callable lacks one original completing declaration authority");
    if source.origin != origin || source.owner != owner || source.kind != ConditionalKind::If || source.arms.is_empty() || source.fallback.is_none() { return Err(invalid()); }
    let mut expected = None;
    for body in source.arms.iter().map(|arm| &arm.body).chain(source.fallback.iter()) {
        let value = callable_branch_value(body).ok_or_else(invalid)?;
        let authority = branch(value.material, value.origin, owner)?;
        let actual = authority.contract();
        if value.original_callable.is_some_and(|declaration| declaration != actual.declaration)
            || matches!(authority, ConditionalCallableAuthority::Creation(_)) && value.original_callable.is_none() {
            return Err(IrVerifyError::new("conditional callable changes its original completing branch declaration"));
        }
        if expected.is_some_and(|expected| expected != actual) { return Err(invalid()); }
        expected = Some(actual);
    }
    expected.ok_or_else(invalid)
}

impl FullBuilder {
    pub(super) fn original_conditional_callable_contract(&self, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner) -> Result<Option<UserCallableContract>, IrBuildError> {
        fn visit(builder: &FullBuilder, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner, active: &mut Vec<u32>) -> Result<ConditionalCallableAuthority, IrVerifyError> {
            if active.len() >= 256 || active.contains(&instruction) { return Err(IrVerifyError::new("conditional callable creation is cyclic or too deep")); }
            if let Some(contract) = builder.generic.as_ref().map(|generic| generic.original_callable_creation_contract(instruction, origin, owner)).transpose()?.flatten() { return Ok(ConditionalCallableAuthority::Creation(contract)); }
            let source = builder.conditional_result_rows.iter().find(|source| source.instruction == instruction).ok_or_else(|| IrVerifyError::new("conditional callable branch loses its original creation proof"))?;
            active.push(instruction);
            let contract = original_conditional_callable_contract(source, origin, owner, |instruction, origin, owner| visit(builder, instruction, origin, owner, active));
            active.pop();
            contract.map(ConditionalCallableAuthority::Branches)
        }
        if !self.conditional_result_rows.iter().any(|source| source.instruction == instruction) { return Ok(None); }
        visit(self, instruction, origin, owner, &mut Vec::new()).map(|authority| Some(authority.contract())).map_err(|_| IrBuildError::format("conditional_callable_original_authority_missing", None, 0, 0))
    }

    pub(super) fn prepare_callable_receivers(&mut self, solved: &crate::sema::check::SolvedTypes, origins: &FxHashMap<u32, (crate::sema::check::ExpressionIdentity, InstructionOwner)>, bindings: &BTreeMap<crate::sema::check::BindingIdentity, super::super::generic::UserCallableContract>) -> Result<(), IrBuildError> {
        let problem = |message| IrBuildError::format(message, None, 0, 0);
        for (original, instruction, owner, resolved) in self.callable_receiver_rows.clone() {
            let contract = bindings.get(&original.binding).ok_or_else(|| problem("callable_receiver_original_binding_missing"))?;
            let (wrapper, initializer, pattern) = resolved.ok_or_else(|| problem("callable_receiver_original_wrapper_missing"))?;
            if origins.contains_key(&instruction) || origins.get(&initializer) != Some(&(original.origin, owner)) {
                return Err(problem("callable_receiver_original_source_changed"));
            }
            let binding = solved.bindings.get(&original.binding).ok_or_else(|| problem("callable_receiver_original_binding_missing"))?;
            if binding.mutable || solved.expression_owners.get(&original.origin).copied() != binding.owner { return Err(problem("callable_receiver_original_owner_changed")); }
            let ty = *solved.expressions.get(&original.origin).ok_or_else(|| problem("callable_receiver_original_type_missing"))?;
            let scope = solved.expression_scope(original.origin, binding.owner).map_err(|_| problem("callable_receiver_original_scope"))?;
            solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope }).map_err(|_| problem("callable_receiver_original_scope"))?;
            let descriptor = self.intern_checked_callable_type(&solved.graph, ty)?;
            if self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("callable_receiver_original_descriptor"))? != Some((contract.kind, contract.signature)) { return Err(problem("callable_receiver_original_contract_changed")); }
            self.generic_evidence_mut().add_original_callable_receiver(OriginalCallableReceiver {
                origin: original.origin, binding: original.binding, instruction, initializer,
                slot: u32::try_from(original.slot).map_err(|_| problem("callable_receiver_slot_overflow"))?, wrapper, pattern, owner,
            }).map_err(|_| problem("callable_receiver_allocation"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    fn fixture() -> FullProgram {
        let source = "pure combine(left: Int = 5, right: Int = 2) -> Int { left + right }\npure selected() -> Int { let alias = combine; alias.call(right: 7) }\n";
        fixture_source(source)
    }

    fn fixture_source(source: &str) -> FullProgram {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "saved-callable-receiver.xsh", crate::loader::entry_source_from_text("saved-callable-receiver.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let solved = Arc::downgrade(&bodies.solved);
        let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
        drop(parsed); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none());
        program
    }

    #[test]
    fn saved_conditional_callable_receiver_refuses_missing_creation_and_changed_completing_branches_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture_source("pure combine(left: Int = 5, right: Int = 2) -> Int { left + right }\npure selected(choice: Bool) -> Int { let alias = if choice { (combine) } else { (combine) }; alias.call(right: 7) }\n");
            FullVerifier::verify(&program).unwrap();
            let generic = program.store.generic.as_deref().unwrap();
            let (conditional, source) = generic.conditional_sources().find(|(_, source)| source.kind == ConditionalKind::If).unwrap();
            let fallback = callable_branch_value(source.fallback.as_ref().unwrap()).unwrap().material;
            let mut missing_branch = program.clone();
            missing_branch.store.generic.as_deref_mut().unwrap().test_remove_callable_values();
            assert!(FullVerifier::verify(&missing_branch).is_err(), "equal branch signatures cannot replace original creation proofs");
            let mut missing_conditional = program.clone();
            missing_conditional.store.generic.as_deref_mut().unwrap().test_clear_conditionals();
            assert!(FullVerifier::verify(&missing_conditional).is_err());
            let mut changed_branch = program.clone();
            let receipt = changed_branch.store.generic.as_deref_mut().unwrap().test_conditional_source_mut(conditional).unwrap();
            let ConditionalBody::Authored { terminal: Some((_, _, ConditionalTerminalValue::Expression(value))), .. } = &mut receipt.arms[0].body else { panic!("the completing function value is retained"); };
            value.material = fallback;
            assert!(FullVerifier::verify(&changed_branch).is_err(), "a completing branch cannot borrow another branch's creation proof");
        });
    }

    #[test]
    fn saved_conditional_callable_receiver_refuses_jointly_rewritten_same_signature_branch_declarations() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture_source("pure combine(left: Int = 5, right: Int = 2) -> Int { left + right }\npure alternate(left: Int = 99, right: Int = 2) -> Int { left + right }\npure selected(choice: Bool) -> Int { let alias = if choice { (combine) } else { (combine) }; alias.call(right: 7) }\npure other() -> Int { let alias = alternate; alias.call(right: 7) }\n");
            program.symbol_owner().with_current(|| {
                let generic = program.store.generic.as_deref().unwrap();
                let (_, conditional) = generic.conditional_sources().find(|(_, source)| source.kind == ConditionalKind::If).unwrap();
                let conditional = conditional.clone();
                let alternate = generic.callable_values().find_map(|(_, proof)| {
                    let name = program.store.string(program.store.functions[proof.contract.target.index()].name).unwrap();
                    (name == "alternate").then_some(proof.contract)
                }).unwrap();
                let originals = conditional.arms.iter().map(|arm| &arm.body).chain(conditional.fallback.iter()).map(|body| {
                    let value = callable_branch_value(body).unwrap();
                    assert_ne!(value.original_callable, Some(alternate.declaration));
                    let id = generic.callable_value_at(value.material).unwrap().unwrap();
                    let proof = generic.callable_value(id).unwrap();
                    assert_eq!(proof.contract.signature, alternate.signature);
                    (value.material, id, proof.source)
                }).collect::<Vec<_>>();
                let mut changed = program.clone();
                for (instruction, id, source) in originals {
                    let raw = changed.store.data[instruction as usize].range().start as usize;
                    changed.store.extra[raw + 1] = Name::intern("alternate").symbol().raw();
                    let generic = changed.store.generic.as_deref_mut().unwrap();
                    generic.test_callable_value_mut(id).unwrap().contract = alternate;
                    *generic.test_callable_creation_expected_mut(source).unwrap() = alternate;
                }
                let error = FullVerifier::verify_conditional_callable_initializer(&changed.store, changed.store.generic.as_deref().unwrap(),
                    conditional.instruction, conditional.origin, conditional.owner, alternate).unwrap_err();
                assert!(error.message.contains("original completing branch declaration"));
                assert!(FullVerifier::verify(&changed).is_err());
            });
        });
    }

    #[test]
    fn saved_callable_receiver_refuses_missing_foreign_and_changed_receipts_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture();
            let generic = program.store.generic.as_deref().unwrap();
            let receiver = generic.original_callable_uses().find_map(|use_| generic.original_callable_receiver(use_.instruction).unwrap()).unwrap().clone();
            assert!(generic.registered_instruction_origin(receiver.instruction, false).is_none(), "a saved receiver read has compiler identity only");
            assert_eq!(generic.registered_instruction_origin(receiver.initializer, false), Some((super::super::super::generic::OperationSourceOrigin::Expression(receiver.origin), receiver.owner)));
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_original_callable_receivers();
            assert!(FullVerifier::verify(&missing).is_err());
            let other = fixture();
            let mut foreign = program.clone();
            foreign.store.generic.as_deref_mut().unwrap().test_replace_original_callable_receivers(other.store.generic.as_deref().unwrap());
            assert!(FullVerifier::verify(&foreign).unwrap_err().message.contains("foreign program"));
            for mutation in 0..6 {
                let mut changed = program.clone();
                let proof = changed.store.generic.as_deref_mut().unwrap().test_original_callable_receiver_mut(receiver.instruction).unwrap();
                match mutation {
                    0 => proof.slot += 1,
                    1 => proof.initializer = proof.instruction,
                    2 => proof.wrapper = proof.instruction,
                    3 => proof.pattern += 1,
                    4 => proof.owner = InstructionOwner::Driver(u32::MAX),
                    _ => proof.binding.target = crate::syntax::arena::BindingTargetId::from_index(proof.binding.target.index() + 1),
                }
                assert!(FullVerifier::verify(&changed).is_err(), "receiver mutation {mutation} was accepted");
            }
            let mut wrong_read = program.clone();
            let raw = wrong_read.store.data[receiver.instruction as usize].range().start as usize;
            wrong_read.store.extra[raw] += 1;
            assert!(FullVerifier::verify(&wrong_read).is_err());
            let mut wrong_initializer = program.clone();
            let raw = wrong_initializer.store.data[receiver.wrapper as usize].range().start as usize;
            wrong_initializer.store.extra[raw] = receiver.instruction;
            assert!(FullVerifier::verify(&wrong_initializer).is_err());
        });
    }
}

impl FullVerifier {
    pub(super) fn verify_conditional_callable_initializer(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner, expected: UserCallableContract) -> Result<bool, IrVerifyError> {
        fn visit(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner, active: &mut Vec<u32>) -> Result<ConditionalCallableAuthority, IrVerifyError> {
            if active.len() >= 256 || active.contains(&instruction) { return Err(IrVerifyError::new("conditional callable creation is cyclic or too deep")); }
            if let Some(id) = generic.callable_value_at(instruction)? {
                let proof = generic.callable_value(id)?;
                let source = generic.callable_source(proof.source)?;
                if store.tags.get(instruction as usize) != Some(&FullTag::ExprFunctionRef) || source.origin != origin || source.owner != owner || source.expected != proof.contract { return Err(IrVerifyError::new("conditional callable changes its original branch creation authority")); }
                return Ok(ConditionalCallableAuthority::Creation(proof.contract));
            }
            let id = generic.conditional_source_at(instruction)?.ok_or_else(|| IrVerifyError::new("conditional callable branch loses its original creation proof"))?;
            let source = generic.conditional_source(id)?;
            active.push(instruction);
            let contract = original_conditional_callable_contract(source, origin, owner, |instruction, origin, owner| visit(store, generic, instruction, origin, owner, active));
            active.pop();
            contract.map(ConditionalCallableAuthority::Branches)
        }
        if generic.conditional_source_at(instruction)?.is_none() { return Ok(false); }
        if visit(store, generic, instruction, origin, owner, &mut Vec::new())?.contract() != expected { return Err(IrVerifyError::new("conditional callable initializer changes its original declaration authority")); }
        Ok(true)
    }

    pub(super) fn verify_saved_callable_receiver(store: &FullStore, tree: &super::super::pattern::PatternTree, index: &callable_prepare::CallableLexicalIndex, binding: &super::super::generic::OriginalCallableBinding, receiver: &OriginalCallableReceiver) -> Result<(), IrVerifyError> {
        let (initializer, pattern, body) = argument_prepare::saved_argument_wrapper(store, receiver.wrapper)?;
        if binding.binding != receiver.binding || binding.owner != receiver.owner || initializer != receiver.initializer || pattern != receiver.pattern
            || store.tags.get(receiver.instruction as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[receiver.instruction as usize].range())? != [receiver.slot]
            || store.tags.get(initializer as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[initializer as usize].range())? != [binding.slot]
            || store.patterns.get(pattern as usize) != Some(&FullPatternTag::Bind)
            || store.payload(store.pattern_data.get(pattern as usize).ok_or_else(|| IrVerifyError::new("saved callable receiver pattern is missing"))?.range())? != [receiver.slot]
            || !tree.is_descendant(body, receiver.instruction)? || !index.dominates(tree, binding.instruction, initializer)? {
            return Err(IrVerifyError::new("saved callable receiver changes its original initialization or binding scope"));
        }
        let range = match receiver.owner {
            InstructionOwner::Function(owner) => store.function_instruction_range(owner.index())?,
            InstructionOwner::Driver(owner) => store.driver_instruction_range(owner as usize)?,
        };
        for instruction in range {
            if matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath | FullTag::StmtAssignInt | FullTag::StmtAssignBool)
                && store.payload(store.data[instruction].range())?.first().is_some_and(|slot| *slot == binding.slot || *slot == receiver.slot) {
                return Err(IrVerifyError::new("saved callable receiver or its original binding is written"));
            }
        }
        Ok(())
    }
}
