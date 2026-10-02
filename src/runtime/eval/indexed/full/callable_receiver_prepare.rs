use super::*;
use super::super::generic::OriginalCallableReceiver;
use super::super::generic::{ConditionalBody, ConditionalKind, ConditionalResultSource, ConditionalTerminalValue, UserCallableContract};
use crate::sema::inference::TypeNode;

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
    pub(super) fn stage_original_callable_capture(&mut self, target: IrFunctionId, declaration: Option<crate::sema::check::DeclarationIdentity>, header_index: u32, original: &LoweredTopLevelSlot) -> Result<(), IrBuildError> {
        let (Some(declaration), Some(binding), Some(root)) = (declaration, original.lexical_binding, original.source_type) else { return Ok(()); };
        if original.mutable || original.host_binding.is_some() { return Ok(()); }
        let solved = self.solved.clone().ok_or_else(|| IrBuildError::format("callable_capture_original_graph_missing", None, 0, 0))?;
        let resolved = solved.graph.resolved(root.ty).map_err(|_| IrBuildError::format("callable_capture_original_type_owner", None, 0, 0))?;
        let user = match solved.graph.node(resolved).map_err(|_| IrBuildError::format("callable_capture_original_type_owner", None, 0, 0))? {
            TypeNode::Arrow(_) => true,
            TypeNode::NativeCallable(callable) => !callable.alternatives.is_empty() && callable.alternatives.iter().all(|authority| matches!(authority, crate::sema::inference::CallableAuthority::User { .. })),
            _ => false,
        };
        if !user { return Ok(()); }
        let header_type = self.store.captures.get(header_index as usize).ok_or_else(|| IrBuildError::format("callable_capture_original_header_missing", None, 0, 0))?.type_id;
        if self.store.semantic.callable_descriptor(header_type).map_err(|_| IrBuildError::format("callable_capture_original_descriptor", None, 0, 0))?.is_none() { return Ok(()); }
        let definition = solved.bindings.get(&binding).ok_or_else(|| IrBuildError::format("callable_capture_original_binding_missing", None, 0, 0))?;
        if definition.mutable || definition.owner == Some(declaration) || definition.ty != root.ty { return Err(IrBuildError::format("callable_capture_original_binding_changed", None, 0, 0)); }
        solved.graph.validate_scoped(root).map_err(|_| IrBuildError::format("callable_capture_original_scope", None, 0, 0))?;
        let ty = self.intern_checked_callable_type(&solved.graph, root.ty)?;
        let header = self.store.captures.get(header_index as usize).ok_or_else(|| IrBuildError::format("callable_capture_original_header_missing", None, 0, 0))?;
        if header.slot_and_flags != original.slot as u32 || header.type_id != ty
            || self.store.string(header.name).map_err(|_| IrBuildError::format("callable_capture_original_name", None, 0, 0))? != original.name.as_str().as_str() {
            return Err(IrBuildError::format("callable_capture_original_header_changed", None, 0, 0));
        }
        self.original_callable_capture_rows.push((target, declaration, header_index, original.clone()));
        Ok(())
    }
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
            let (wrapper, initializer, pattern) = resolved.ok_or_else(|| problem("callable_receiver_original_wrapper_missing"))?;
            if origins.contains_key(&instruction) || origins.get(&initializer) != Some(&(original.origin, owner)) {
                return Err(problem("callable_receiver_original_source_changed"));
            }
            let binding = solved.bindings.get(&original.binding).ok_or_else(|| problem("callable_receiver_original_binding_missing"))?;
            let receiver_owner = original.capture.as_ref().map(|capture| capture.caller).or(binding.owner);
            if binding.mutable || solved.expression_owners.get(&original.origin).copied() != receiver_owner { return Err(problem("callable_receiver_original_owner_changed")); }
            let ty = *solved.expressions.get(&original.origin).ok_or_else(|| problem("callable_receiver_original_type_missing"))?;
            let scope = solved.expression_scope(original.origin, receiver_owner).map_err(|_| problem("callable_receiver_original_scope"))?;
            solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope }).map_err(|_| problem("callable_receiver_original_scope"))?;
            let descriptor = self.intern_checked_callable_type(&solved.graph, ty)?;
            let (contract, capture) = if let Some(read) = &original.capture {
                let InstructionOwner::Function(target) = owner else { return Err(problem("callable_receiver_capture_owner")); };
                let allocations = self.original_callable_capture_rows.iter().filter(|(actual_target, declaration, _, allocation)| *actual_target == target && *declaration == read.caller && allocation.slot == read.slot).collect::<Vec<_>>();
                if allocations.len() != 1 { return Err(problem("callable_receiver_capture_allocation_missing")); }
                let (_, _, header_index, allocation) = allocations[0];
                if allocation.lexical_binding != Some(original.binding) || allocation.name != read.name || allocation.mutable
                    || read.source_type.ty != ty || read.source_type.scope != scope || binding.owner == Some(read.caller) {
                    return Err(problem("callable_receiver_capture_allocation_changed"));
                }
                let flow = *solved.expression_producer_flows.get(&original.origin).ok_or_else(|| problem("callable_receiver_capture_flow_missing"))?;
                let node = solved.producer_flows.node(flow).map_err(|_| problem("callable_receiver_capture_flow_owner"))?;
                let crate::sema::check::ProducerFlowKind::CapturedBinding { identity, version, input } = node.kind else { return Err(problem("callable_receiver_capture_flow_changed")); };
                if identity != original.binding || node.source != crate::sema::check::ProducerFlowSource::Expression(original.origin)
                    || solved.binding_producer_flows.get(&(identity, version)) != Some(&input) { return Err(problem("callable_receiver_capture_flow_changed")); }
                let header_index = *header_index;
                let allocation = allocation.clone();
                let contract = self.checked_user_callable(solved, original.origin)?;
                let header = &self.store.captures[header_index as usize];
                let capture = super::super::generic::CapturedCallableReceiver {
                    declaration: read.caller, definition_owner: binding.owner, header_index,
                    slot: u32::try_from(allocation.slot).map_err(|_| problem("callable_receiver_capture_slot_overflow"))?,
                    name: header.name, ty: header.type_id,
                    source_type: allocation.source_type.ok_or_else(|| problem("callable_receiver_capture_type_missing"))?,
                };
                (contract, Some(capture))
            } else { (*bindings.get(&original.binding).ok_or_else(|| problem("callable_receiver_original_binding_missing"))?, None) };
            if self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("callable_receiver_original_descriptor"))? != Some((contract.kind, contract.signature)) { return Err(problem("callable_receiver_original_contract_changed")); }
            self.generic_evidence_mut().add_original_callable_receiver(OriginalCallableReceiver {
                origin: original.origin, binding: original.binding, instruction, initializer,
                slot: u32::try_from(original.slot).map_err(|_| problem("callable_receiver_slot_overflow"))?, wrapper, pattern, owner, capture, contract,
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
    fn saved_captured_callable_receiver_retains_original_allocation_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture_source("pure combine(left: Int = 5, right: Int = 2) -> Int { left + right }\nlet alias = combine\nlet other = combine\npure selected() -> Int { alias.call(right: 7) }\n");
            FullVerifier::verify(&program).unwrap();
            let generic = program.store.generic.as_deref().unwrap();
            let receiver = generic.original_callable_receivers().find(|receiver| receiver.capture.is_some()).unwrap().clone();
            let capture = receiver.capture.as_ref().unwrap();
            let InstructionOwner::Function(target) = receiver.owner else { panic!("the receiving declaration is retained"); };
            let captures = program.store.functions[target.index()].captures.bounds(program.store.captures.len()).unwrap();
            let other = program.store.captures[captures].iter().find(|header| header.type_id == capture.ty && header.slot_and_flags != capture.slot).unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_original_callable_receivers();
            assert!(FullVerifier::verify(&missing).is_err(), "a compiler temporary cannot replace its original captured receiver receipt");
            let mut changed_read = program.clone();
            let raw = changed_read.store.data[receiver.initializer as usize].range().start as usize;
            changed_read.store.extra[raw] = other.slot_and_flags;
            assert!(FullVerifier::verify(&changed_read).is_err(), "another same-signature capture cannot replace the original binding allocation");
            let mut changed_header = program.clone();
            changed_header.store.captures[capture.header_index as usize].name = other.name;
            assert!(FullVerifier::verify(&changed_header).is_err(), "a same-signature header name cannot replace the original captured allocation");
            let mut changed_receipt = program.clone();
            changed_receipt.store.generic.as_deref_mut().unwrap().test_original_callable_receiver_mut(receiver.instruction).unwrap().capture.as_mut().unwrap().header_index += 1;
            assert!(FullVerifier::verify(&changed_receipt).is_err(), "the original captured receipt survives independently of a rewritten receipt");
        });
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
    pub(super) fn verify_saved_callable_receiver_authority(store: &FullStore, generic: &GenericEvidenceStore, receiver: &OriginalCallableReceiver, contract: UserCallableContract, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner) -> Result<(), IrVerifyError> {
        if receiver.contract != contract || receiver.origin != origin || receiver.owner != owner {
            return Err(IrVerifyError::new("saved callable receiver changes its original declaration authority"));
        }
        if let Some(capture) = &receiver.capture {
            if receiver.owner != InstructionOwner::Function(generic.checked_function(capture.declaration)?.target)
                || store.semantic.callable_descriptor(capture.ty)? != Some((contract.kind, contract.signature)) {
                return Err(IrVerifyError::new("saved callable receiver changes its captured declaration authority"));
            }
        } else {
            let use_ = generic.original_callable_use(receiver.instruction).ok_or_else(|| IrVerifyError::new("saved callable receiver lost its original use"))?;
            let binding = generic.original_callable_binding(receiver.binding).ok_or_else(|| IrVerifyError::new("saved callable receiver lost its original binding"))?;
            if use_.binding != receiver.binding || use_.origin != origin || use_.owner != owner || binding.contract != contract {
                return Err(IrVerifyError::new("saved callable receiver changes its original binding authority"));
            }
        }
        Ok(())
    }

    pub(super) fn verify_captured_saved_callable_receiver(store: &FullStore, tree: &super::super::pattern::PatternTree, receiver: &OriginalCallableReceiver) -> Result<(), IrVerifyError> {
        let capture = receiver.capture.as_ref().ok_or_else(|| IrVerifyError::new("saved callable receiver has no original captured allocation"))?;
        let InstructionOwner::Function(target) = receiver.owner else { return Err(IrVerifyError::new("captured saved callable receiver has no receiving function")); };
        let generic = store.generic.as_deref().ok_or_else(|| IrVerifyError::new("captured saved callable receiver has no original evidence"))?;
        if generic.checked_function(capture.declaration)?.target != target || capture.definition_owner == Some(capture.declaration) {
            return Err(IrVerifyError::new("captured saved callable receiver changes its original receiving declaration"));
        }
        let function = store.functions.get(target.index()).ok_or_else(|| IrVerifyError::new("captured saved callable receiver function is missing"))?;
        let captures = function.captures.bounds(store.captures.len()).ok_or_else(|| IrVerifyError::new("captured saved callable receiver header is invalid"))?;
        if !captures.contains(&(capture.header_index as usize)) { return Err(IrVerifyError::new("captured saved callable receiver allocation belongs to another header")); }
        let header = &store.captures[capture.header_index as usize];
        let (initializer, pattern, body) = argument_prepare::saved_argument_wrapper(store, receiver.wrapper)?;
        if header.slot_and_flags != capture.slot || header.name != capture.name || header.type_id != capture.ty
            || store.semantic.callable_descriptor(capture.ty)? != Some((receiver.contract.kind, receiver.contract.signature))
            || initializer != receiver.initializer || pattern != receiver.pattern || capture.slot == receiver.slot
            || store.tags.get(receiver.instruction as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[receiver.instruction as usize].range())? != [receiver.slot]
            || store.tags.get(initializer as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[initializer as usize].range())? != [capture.slot]
            || store.patterns.get(pattern as usize) != Some(&FullPatternTag::Bind)
            || store.payload(store.pattern_data.get(pattern as usize).ok_or_else(|| IrVerifyError::new("saved callable receiver pattern is missing"))?.range())? != [receiver.slot]
            || !tree.is_descendant(body, receiver.instruction)? {
            return Err(IrVerifyError::new("captured saved callable receiver changes its original allocation or initialization"));
        }
        for instruction in store.function_instruction_range(target.index())? {
            if matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath | FullTag::StmtAssignInt | FullTag::StmtAssignBool)
                && store.payload(store.data[instruction].range())?.first().is_some_and(|slot| *slot == capture.slot || *slot == receiver.slot) {
                return Err(IrVerifyError::new("captured saved callable receiver or its immutable allocation is written"));
            }
        }
        Ok(())
    }
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
