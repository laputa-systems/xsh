use super::*;
use super::super::generic::{OperationSourceOrigin, OriginalArgumentBinding};

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

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
    pub(super) fn prepare_original_argument_bindings(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        self.prepared_argument_origins = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect();
        for (original, instruction, owner, resolved) in self.argument_binding_rows.clone() {
            let crate::sema::arguments::ArgumentValueSource::Expression(expression) = original.recipe.value else { continue; };
            let recipes = solved.argument_sources.get(&original.call).ok_or_else(|| problem("saved_argument_original_recipes_missing"))?;
            if recipes.get(original.ordinal) != Some(&original.recipe) { return Err(problem("saved_argument_original_recipe_changed")); }
            let (wrapper, initializer, pattern) = resolved.ok_or_else(|| problem("saved_argument_original_wrapper_missing"))?;
            let origin = crate::sema::check::ExpressionIdentity { expression, ..original.call };
            if self.prepared_argument_origins.get(&initializer) != Some(&(origin, owner)) { return Err(problem("saved_argument_initializer_origin_changed")); }
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
                instruction, initializer, slot: u32::try_from(original.slot).map_err(|_| problem("saved_argument_slot_overflow"))?, wrapper, pattern, owner, scope, ty };
            if self.prepared_saved_argument_bindings.insert(instruction, saved.clone()).is_some() { return Err(problem("saved_argument_read_ambiguous")); }
            self.generic_evidence_mut().add_original_argument_binding(saved).map_err(|_| problem("saved_argument_capacity"))?;
        }
        Ok(())
    }

    pub(super) fn original_argument_expression(&self, instruction: u32, call: crate::sema::check::ExpressionIdentity, ordinal: usize, recipe: &crate::sema::check::SolvedArgumentSource, owner: InstructionOwner) -> Result<crate::sema::check::ExpressionIdentity, IrBuildError> {
        let crate::sema::arguments::ArgumentValueSource::Expression(expression) = recipe.value else { return Err(problem("argument_recipe_protocol_not_prepared")); };
        let origin = crate::sema::check::ExpressionIdentity { expression, ..call };
        if self.prepared_argument_origins.get(&instruction) == Some(&(origin, owner)) { return Ok(origin); }
        if self.prepared_saved_argument_bindings.get(&instruction).is_some_and(|saved| saved.call == call && saved.ordinal as usize == ordinal && saved.recipe == *recipe && saved.owner == owner) { return Ok(origin); }
        Err(problem("argument_operand_original_source_missing"))
    }
}

impl FullVerifier {
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
            if generic.registered_instruction_origin(initializer, false) != Some((OperationSourceOrigin::Expression(crate::sema::check::ExpressionIdentity { expression, ..saved.call }), saved.owner)) { return Err(IrVerifyError::new("saved argument initializer lost its original syntax source")); }
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
