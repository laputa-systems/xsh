use super::*;
use super::super::generic::{OperationSourceOrigin, OriginalArgumentBinding, OriginalCompilerArgumentWrapper, ValueInitializerWrapper, ValueInitializerWrapperKind};

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

// Compiler wrappers preserve evaluation order. Their receipt is independent of
// the original material expression and of generated saved-value reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CompilerArgumentWrapper {
    pub(super) initializer: u32,
    pub(super) pattern: u32,
    pub(super) body: u32,
    pub(super) slot: u32,
    pub(super) owner: InstructionOwner,
}

pub(super) fn saved_argument_wrapper(store: &FullStore, instruction: u32) -> Result<(u32, u32, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprMatch) { return Err(IrVerifyError::new("saved argument has another binding wrapper")); }
    let words = store.payload(store.data[instruction as usize].range())?;
    let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("saved argument wrapper arms are missing"))?;
    let arms = store.payload(block.instructions)?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || arms.len() != 4 || arms[0] != 1 || arms[2] != 0 {
        return Err(IrVerifyError::new("saved argument wrapper changes its single binding arm"));
    }
    Ok((*words.first().ok_or_else(|| IrVerifyError::new("saved argument initializer is missing"))?, arms[1], arms[3]))
}

impl FullBuilder {
    pub(super) fn argument_initializer_lineage(&self, initializer: u32, owner: InstructionOwner) -> Result<(u32, Box<[ValueInitializerWrapper]>), IrBuildError> {
        let range = match owner {
            InstructionOwner::Function(function) => self.store.function_instruction_range(function.index()),
            InstructionOwner::Driver(driver) => self.store.driver_instruction_range(driver as usize),
        }.map_err(|_| problem("argument_initializer_owner_range"))?;
        let mut source = initializer;
        let mut wrappers = Vec::new();
        loop {
            if !range.contains(&(source as usize)) { return Err(problem("argument_initializer_owner_changed")); }
            let checked = self.store.tags.get(source as usize) == Some(&FullTag::ExprCheckedValue);
            let compiler = self.compiler_argument_wrapper(source, owner)?;
            if !checked && compiler.is_none() { break; }
            if wrappers.len() >= 256 { return Err(problem("argument_initializer_wrapper_depth")); }
            let payload = self.store.payload(self.store.data[source as usize].range()).map_err(|_| problem("argument_initializer_wrapper_payload"))?.to_vec().into_boxed_slice();
            let (child, kind) = if checked {
                (*payload.first().ok_or_else(|| problem("argument_initializer_wrapper_empty"))?, ValueInitializerWrapperKind::CheckedValue)
            } else {
                let compiler = compiler.ok_or_else(|| problem("argument_initializer_compiler_receipt_missing"))?;
                (compiler.body, ValueInitializerWrapperKind::CompilerArgument {
                    initializer: compiler.initializer, pattern: compiler.pattern, body: compiler.body, slot: compiler.slot,
                })
            };
            if child >= source { return Err(problem("argument_initializer_wrapper_cycle")); }
            wrappers.push(ValueInitializerWrapper { instruction: source, payload, kind });
            source = child;
        }
        Ok((source, wrappers.into_boxed_slice()))
    }

    pub(super) fn prepare_compiler_argument_wrappers(&mut self) -> Result<(), IrBuildError> {
        let mut wrappers = self.compiler_argument_wrappers.iter().map(|(&instruction, &wrapper)| (instruction, wrapper)).collect::<Vec<_>>();
        wrappers.sort_unstable_by_key(|&(instruction, _)| instruction);
        for (instruction, wrapper) in wrappers {
            FullVerifier::verify_compiler_argument_wrapper(&self.store, instruction, wrapper.initializer, wrapper.pattern, wrapper.body, wrapper.slot)
                .map_err(|_| problem("compiler_argument_wrapper_changed"))?;
            let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("compiler_argument_wrapper_payload"))?.to_vec().into_boxed_slice();
            let block = IrBlockId::from_raw(payload[1]).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| problem("compiler_argument_wrapper_arms"))?;
            let arms_flags = block.flags;
            let arms_payload = self.store.payload(block.instructions).map_err(|_| problem("compiler_argument_wrapper_arms"))?.to_vec().into_boxed_slice();
            let pattern_payload = self.store.payload(self.store.pattern_data[wrapper.pattern as usize].range()).map_err(|_| problem("compiler_argument_wrapper_pattern"))?.to_vec().into_boxed_slice();
            self.generic_evidence_mut().add_original_compiler_argument_wrapper(OriginalCompilerArgumentWrapper {
                instruction, initializer: wrapper.initializer, pattern: wrapper.pattern, body: wrapper.body, slot: wrapper.slot, owner: wrapper.owner,
                payload, arms_flags, arms_payload, pattern_payload, optional_receiver_guard: None,
            }).map_err(|_| problem("compiler_argument_wrapper_allocation"))?;
        }
        self.prepare_optional_receiver_guards()?;
        Ok(())
    }

    pub(super) fn stage_compiler_argument_wrapper(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(&(initializer, pattern, body, slot)) = scratch.compiler_argument_wrappers.get(&expression) else { return Ok(()); };
        if !matches!(scratch.expressions.get(expression.index()), Some(BuildExprRow::MatchExpr { value, arms, .. })
            if *value == initializer && arms.as_slice() == [(pattern, None, body)])
            || !matches!(scratch.patterns.get(pattern.index()), Some(BuildPatternRow::Bind { slot: actual }) if *actual == slot) {
            return Err(problem("compiler_argument_wrapper_source_changed"));
        }
        let initializer = *self.active_encoded_expressions.get(&initializer).ok_or_else(|| problem("compiler_argument_initializer_missing"))?;
        let body = *self.active_encoded_expressions.get(&body).ok_or_else(|| problem("compiler_argument_body_missing"))?;
        let (_, pattern, _) = saved_argument_wrapper(&self.store, instruction).map_err(|_| problem("compiler_argument_wrapper_encoding_changed"))?;
        let slot = u32::try_from(slot).map_err(|_| problem("compiler_argument_slot_overflow"))?;
        FullVerifier::verify_compiler_argument_wrapper(&self.store, instruction, initializer, pattern, body, slot).map_err(|_| problem("compiler_argument_wrapper_encoding_changed"))?;
        let raw = self.current_owner.ok_or_else(|| problem("compiler_argument_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("compiler_argument_owner_invalid"))?) };
        if self.compiler_argument_wrappers.insert(instruction, CompilerArgumentWrapper { initializer, pattern, body, slot, owner }).is_some() {
            return Err(problem("compiler_argument_wrapper_duplicate"));
        }
        Ok(())
    }

    pub(super) fn compiler_argument_wrapper(&self, instruction: u32, owner: InstructionOwner) -> Result<Option<CompilerArgumentWrapper>, IrBuildError> {
        let Some(&wrapper) = self.compiler_argument_wrappers.get(&instruction) else { return Ok(None); };
        if wrapper.owner != owner { return Err(problem("compiler_argument_wrapper_owner_changed")); }
        FullVerifier::verify_compiler_argument_wrapper(&self.store, instruction, wrapper.initializer, wrapper.pattern, wrapper.body, wrapper.slot)
            .map_err(|_| problem("compiler_argument_wrapper_changed"))?;
        Ok(Some(wrapper))
    }

    pub(super) fn prepare_original_argument_bindings(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        self.prepared_argument_origins = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect();
        for (original, instruction, owner, resolved) in self.argument_binding_rows.clone() {
            let crate::sema::arguments::ArgumentValueSource::Expression(expression) = original.recipe.value else { continue; };
            let recipes = solved.argument_sources.get(&original.call).ok_or_else(|| problem("saved_argument_original_recipes_missing"))?;
            if recipes.get(original.ordinal) != Some(&original.recipe) { return Err(problem("saved_argument_original_recipe_changed")); }
            let (wrapper, initializer, pattern) = resolved.ok_or_else(|| problem("saved_argument_original_wrapper_missing"))?;
            let (initializer_source_instruction, initializer_wrappers) = self.argument_initializer_lineage(initializer, owner)?;
            let origin = crate::sema::check::ExpressionIdentity { expression, ..original.call };
            if self.prepared_argument_origins.get(&initializer_source_instruction) != Some(&(origin, owner)) { return Err(problem("saved_argument_initializer_origin_changed")); }
            let caller = solved.expression_owners.get(&original.call).copied();
            if solved.expression_owners.get(&origin).copied() != caller { return Err(problem("saved_argument_initializer_lexical_owner_changed")); }
            let ty = *solved.expressions.get(&origin).ok_or_else(|| problem("saved_argument_original_type_missing"))?;
            let source_scope = solved.expression_scope(origin, caller).map_err(|_| problem("saved_argument_type_scope_invalid"))?;
            solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope: source_scope }).map_err(|_| problem("saved_argument_type_scope_invalid"))?;
            let scope = caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let ty = if let Some(scope) = scope {
                if source_scope != self.generic_schemes.get(&scope).copied() { return Err(problem("saved_argument_original_caller_scope_changed")); }
                self.call_reference(&solved, Some(scope), ty, false)?
            } else {
                let ty = super::super::generic::graph_ground_type(&solved.graph, ty).map_err(|_| problem("saved_argument_symbolic_scope_not_prepared"))?;
                TypeRef::Ground(self.intern_generic_ground_type(&ty)?)
            };
            let saved = OriginalArgumentBinding { call: original.call, ordinal: u32::try_from(original.ordinal).map_err(|_| problem("saved_argument_ordinal_overflow"))?, recipe: original.recipe,
                instruction, initializer, initializer_source_instruction, initializer_wrappers,
                slot: u32::try_from(original.slot).map_err(|_| problem("saved_argument_slot_overflow"))?, wrapper, pattern, owner, scope, ty };
            if self.prepared_saved_argument_bindings.insert(instruction, saved.clone()).is_some() { return Err(problem("saved_argument_read_ambiguous")); }
            self.generic_evidence_mut().add_original_argument_binding(saved).map_err(|_| problem("saved_argument_capacity"))?;
        }
        Ok(())
    }

    pub(super) fn original_argument_expression(&self, instruction: u32, call: crate::sema::check::ExpressionIdentity, ordinal: usize, recipe: &crate::sema::check::SolvedArgumentSource, owner: InstructionOwner) -> Result<crate::sema::check::ExpressionIdentity, IrBuildError> {
        let crate::sema::arguments::ArgumentValueSource::Expression(expression) = recipe.value else { return Err(problem("argument_recipe_protocol_not_prepared")); };
        let origin = crate::sema::check::ExpressionIdentity { expression, ..call };
        if self.prepared_argument_origins.get(&instruction) == Some(&(origin, owner)) { return Ok(origin); }
        let (source, _) = self.argument_initializer_lineage(instruction, owner)?;
        if self.prepared_argument_origins.get(&source) == Some(&(origin, owner)) { return Ok(origin); }
        if self.prepared_saved_argument_bindings.get(&source).is_some_and(|saved| saved.call == call && saved.ordinal as usize == ordinal && saved.recipe == *recipe && saved.owner == owner) { return Ok(origin); }
        Err(IrBuildError::verification("argument_operand_original_source_missing", IrVerifyError::new(format!(
            "original call {call:?} argument {ordinal} recipe {recipe:?} owner {owner:?} has operand {instruction} {:?}, lineage source {source} {:?}, saved source {:?}",
            self.store.tags.get(instruction as usize), self.store.tags.get(source as usize), self.prepared_saved_argument_bindings.get(&source).map(|saved| (saved.call, saved.ordinal)),
        ))))
    }
}

impl FullVerifier {
    pub(super) fn verify_argument_initializer_lineage(store: &FullStore, generic: &GenericEvidenceStore,
        initializer: u32, source: u32, wrappers: &[ValueInitializerWrapper], owner: InstructionOwner,
    ) -> Result<(), IrVerifyError> {
        let range = match owner {
            InstructionOwner::Function(function) => store.function_instruction_range(function.index()),
            InstructionOwner::Driver(driver) => store.driver_instruction_range(driver as usize),
        }?;
        let mut instruction = initializer;
        if wrappers.len() > 256 { return Err(IrVerifyError::new("argument initializer wrapper lineage is too deep")); }
        for wrapper in wrappers {
            if wrapper.instruction != instruction || !range.contains(&(instruction as usize))
                || store.payload(store.data[instruction as usize].range())? != wrapper.payload.as_ref() {
                return Err(IrVerifyError::new("argument initializer changes its original physical wrapper"));
            }
            let child = match wrapper.kind {
                ValueInitializerWrapperKind::CheckedValue => {
                    if store.tags.get(instruction as usize) != Some(&FullTag::ExprCheckedValue) { return Err(IrVerifyError::new("argument initializer changes its checked wrapper kind")); }
                    *wrapper.payload.first().ok_or_else(|| IrVerifyError::new("argument initializer wrapper is empty"))?
                }
                ValueInitializerWrapperKind::CompilerArgument { initializer, pattern, body, slot } => {
                    Self::verify_compiler_argument_wrapper(store, instruction, initializer, pattern, body, slot)?;
                    if Self::original_compiler_argument_wrapper_body(store, generic, instruction, owner)? != Some(body) {
                        return Err(IrVerifyError::new("argument initializer loses its original compiler wrapper receipt"));
                    }
                    body
                }
                _ => return Err(IrVerifyError::new("argument initializer has another wrapper authority")),
            };
            if child >= instruction { return Err(IrVerifyError::new("argument initializer wrapper lineage is cyclic")); }
            instruction = child;
        }
        if instruction != source || !range.contains(&(source as usize))
            || store.tags.get(source as usize) == Some(&FullTag::ExprCheckedValue)
            || generic.original_compiler_argument_wrapper(source)?.is_some() {
            return Err(IrVerifyError::new("argument initializer loses its original material source"));
        }
        Ok(())
    }

    pub(super) fn original_compiler_argument_wrapper_body(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Option<u32>, IrVerifyError> {
        let Some(wrapper) = generic.original_compiler_argument_wrapper(instruction)? else { return Ok(None); };
        if wrapper.instruction != instruction || wrapper.owner != owner { return Err(IrVerifyError::new("compiler wrapper changes its original instruction or owner")); }
        Self::verify_compiler_argument_wrapper(store, instruction, wrapper.initializer, wrapper.pattern, wrapper.body, wrapper.slot)?;
        let payload = store.payload(store.data[instruction as usize].range())?;
        let block = IrBlockId::from_raw(payload[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("compiler wrapper arms are missing"))?;
        if payload != wrapper.payload.as_ref() || block.flags != wrapper.arms_flags
            || store.payload(block.instructions)? != wrapper.arms_payload.as_ref()
            || store.payload(store.pattern_data[wrapper.pattern as usize].range())? != wrapper.pattern_payload.as_ref() {
            return Err(IrVerifyError::new("compiler wrapper changes its original physical selection"));
        }
        Ok(Some(wrapper.body))
    }

    pub(super) fn verify_compiler_argument_wrappers(store: &FullStore) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        for wrapper in generic.original_compiler_argument_wrappers() {
            Self::original_compiler_argument_wrapper_body(store, generic, wrapper.instruction, wrapper.owner)?
                .ok_or_else(|| IrVerifyError::new("compiler wrapper original receipt is missing"))?;
        }
        Ok(())
    }

    pub(super) fn verify_compiler_argument_wrapper(store: &FullStore, instruction: u32, initializer: u32, pattern: u32, body: u32, slot: u32) -> Result<(), IrVerifyError> {
        if saved_argument_wrapper(store, instruction)? != (initializer, pattern, body)
            || store.patterns.get(pattern as usize) != Some(&FullPatternTag::Bind)
            || store.payload(store.pattern_data.get(pattern as usize).ok_or_else(|| IrVerifyError::new("compiler argument pattern is missing"))?.range())? != [slot] {
            return Err(IrVerifyError::new("compiler argument wrapper changes its original initialization or binding arm"));
        }
        Ok(())
    }

    pub(super) fn original_argument_wrapper_body(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Option<(u32, crate::sema::check::ExpressionIdentity)>, IrVerifyError> {
        let Some(saved) = generic.original_argument_wrapper(instruction) else { return Ok(None); };
        let (initializer, pattern, body) = saved_argument_wrapper(store, instruction)?;
        if saved.owner != owner || saved.initializer != initializer || saved.pattern != pattern { return Err(IrVerifyError::new("saved argument wrapper changes its original owner or initialization")); }
        Ok(Some((body, saved.call)))
    }

    pub(super) fn verify_original_argument_bindings(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        for saved in generic.original_argument_bindings() {
            let (initializer, pattern, body) = saved_argument_wrapper(store, saved.wrapper)?;
            if initializer != saved.initializer || pattern != saved.pattern || store.tags.get(saved.instruction as usize) != Some(&FullTag::ExprParam)
                || store.payload(store.data[saved.instruction as usize].range())? != [saved.slot]
                || store.patterns.get(pattern as usize) != Some(&FullPatternTag::Bind)
                || store.payload(store.pattern_data.get(pattern as usize).ok_or_else(|| IrVerifyError::new("saved argument binding pattern is missing"))?.range())? != [saved.slot]
                || !tree.is_descendant(body, saved.instruction)? {
                return Err(IrVerifyError::new("saved argument read changes its original initialization or binding scope"));
            }
            let crate::sema::arguments::ArgumentValueSource::Expression(expression) = saved.recipe.value else { return Err(IrVerifyError::new("saved argument recipe is not prepared")); };
            Self::verify_argument_initializer_lineage(store, generic, initializer, saved.initializer_source_instruction, &saved.initializer_wrappers, saved.owner)?;
            if generic.registered_instruction_origin(saved.initializer_source_instruction, false) != Some((OperationSourceOrigin::Expression(crate::sema::check::ExpressionIdentity { expression, ..saved.call }), saved.owner)) { return Err(IrVerifyError::new("saved argument initializer lost its original syntax source")); }
            match saved.scope {
                Some(scope) => Self::verify_generic_symbolic_source(store, generic, initializer, scope, saved.ty, &mut Vec::new())?,
                None => {
                    let TypeRef::Ground(ty) = saved.ty else { return Err(IrVerifyError::new("saved argument has an unowned symbolic type")); };
                    Self::verify_generic_source(store, generic, initializer, saved.owner, &store.semantic.to_type(ty)?, None, &mut Vec::new())?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "argument_prepare/initializer_tests.rs"]
mod initializer_tests;

#[path = "argument_prepare/optional_receiver.rs"]
mod optional_receiver;
pub(in crate::runtime::eval) use optional_receiver::BuildOptionalReceiverGuard;
