use super::*;
use super::super::super::generic::PreparedNativeRecordFieldArgument;

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn prepare_native_record_argument(&mut self, call: crate::sema::check::ExpressionIdentity, ordinal: usize, formal_slot: usize,
        recipe: &crate::sema::check::SolvedArgumentSource, instruction: u32, owner: InstructionOwner,
    ) -> Result<(PreparedNativeRecordFieldArgument, Type), IrBuildError> {
        let crate::sema::arguments::ArgumentValueSource::RecordField { record, field } = recipe.value else { return Err(native_problem("native_record_argument_recipe_kind")); };
        let solved = self.solved.clone().ok_or_else(|| native_problem("native_record_argument_original_graph_missing"))?;
        if solved.argument_sources.get(&call).and_then(|recipes| recipes.get(ordinal)) != Some(recipe) { return Err(native_problem("native_record_argument_original_recipe_changed")); }
        let (field_read, wrappers) = self.argument_initializer_lineage(instruction, owner)?;
        let mut bindings = self.argument_binding_rows.iter().filter(|(original, read, actual_owner, _)| *read == field_read && *actual_owner == owner
            && original.call == call && original.ordinal == ordinal && original.recipe == *recipe);
        let (_, _, _, resolved) = bindings.next().ok_or_else(|| native_problem("native_record_argument_original_binding_missing"))?;
        let resolved = *resolved;
        if bindings.next().is_some() { return Err(native_problem("native_record_argument_original_binding_ambiguous")); }
        let (field_wrapper, field_initializer, field_pattern) = resolved.ok_or_else(|| native_problem("native_record_argument_original_wrapper_missing"))?;
        let saved = self.compiler_argument_wrapper(field_wrapper, owner)?.ok_or_else(|| native_problem("native_record_argument_original_allocation_missing"))?;
        if saved.initializer != field_initializer || saved.pattern != field_pattern
            || self.store.tags.get(field_read as usize) != Some(&FullTag::ExprParam)
            || self.store.payload(self.store.data[field_read as usize].range()).map_err(|_| native_problem("native_record_argument_saved_read_payload"))? != [saved.slot]
            || self.store.tags.get(field_initializer as usize) != Some(&FullTag::ExprField) {
            return Err(native_problem("native_record_argument_original_field_allocation_changed"));
        }
        let projection = self.store.payload(self.store.data[field_initializer as usize].range()).map_err(|_| native_problem("native_record_argument_projection_payload"))?;
        if projection.len() != 3 || self.store.string(projection[1]).map_err(|_| native_problem("native_record_argument_projection_field"))? != field.as_str().as_str() {
            return Err(native_problem("native_record_argument_original_projection_changed"));
        }
        let record_read = projection[0];
        if self.store.tags.get(record_read as usize) != Some(&FullTag::ExprParam) { return Err(native_problem("native_record_argument_saved_record_kind")); }
        let read = self.store.payload(self.store.data[record_read as usize].range()).map_err(|_| native_problem("native_record_argument_saved_record_payload"))?;
        let [record_slot] = read else { return Err(native_problem("native_record_argument_saved_record_slot")); };
        let record_slot = *record_slot;
        let record_origin = crate::sema::check::ExpressionIdentity { expression: record, ..call };
        let mut allocation = None;
        for (&record_wrapper, candidate) in &self.compiler_argument_wrappers {
            if candidate.owner != owner || candidate.slot != record_slot { continue; }
            let (material, record_wrappers) = self.argument_initializer_lineage(candidate.initializer, owner)?;
            if self.prepared_argument_origins.get(&material) != Some(&(record_origin, owner)) { continue; }
            if allocation.replace((record_wrapper, *candidate, material, record_wrappers)).is_some() { return Err(native_problem("native_record_argument_original_record_allocation_ambiguous")); }
        }
        let (record_wrapper, record_saved, record_source_instruction, record_wrappers) = allocation.ok_or_else(|| native_problem("native_record_argument_original_record_allocation_missing"))?;
        let caller = solved.expression_owners.get(&call).copied();
        if solved.expression_owners.get(&record_origin).copied() != caller { return Err(native_problem("native_record_argument_original_record_owner_changed")); }
        let ty = *solved.expressions.get(&record_origin).ok_or_else(|| native_problem("native_record_argument_original_record_type_missing"))?;
        let scope = solved.expression_scope(record_origin, caller).map_err(|_| native_problem("native_record_argument_original_record_scope"))?;
        solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope }).map_err(|_| native_problem("native_record_argument_original_record_certificate"))?;
        let record_type = graph_ground_type(&solved.graph, ty).map_err(|_| native_problem("native_record_argument_original_record_not_closed"))?;
        let Type::Record(fields) = &record_type else { return Err(native_problem("native_record_argument_original_record_not_finite")); };
        let actual = fields.get(&field).cloned().ok_or_else(|| native_problem("native_record_argument_original_field_missing"))?;
        if fields.is_empty() { return Err(native_problem("native_record_argument_original_record_open")); }
        let record_type = self.intern_generic_ground_type(&record_type)?;
        let receipt = PreparedNativeRecordFieldArgument {
            ordinal: u32::try_from(ordinal).map_err(|_| native_problem("native_record_argument_ordinal_overflow"))?,
            formal_slot: u32::try_from(formal_slot).map_err(|_| native_problem("native_record_argument_formal_slot_overflow"))?,
            entry_index: u32::try_from(recipe.entry_index).map_err(|_| native_problem("native_record_argument_entry_overflow"))?,
            record_origin, field, record_type, instruction, wrappers, field_read, field_initializer, field_wrapper, field_pattern,
            field_body: saved.body, field_slot: saved.slot, record_read, record_initializer: record_saved.initializer, record_source_instruction, record_wrappers,
            record_wrapper, record_pattern: record_saved.pattern, record_body: record_saved.body, record_slot,
        };
        Ok((receipt, actual))
    }
}

impl FullVerifier {
    pub(super) fn verify_native_record_argument_packets(store: &FullStore, generic: &GenericEvidenceStore, source: &NativeCallSource) -> Result<(), IrVerifyError> {
        let range = match source.owner { InstructionOwner::Function(function) => store.function_instruction_range(function.index()), InstructionOwner::Driver(driver) => store.driver_instruction_range(driver as usize) }?;
        for field in source.record_arguments.iter() {
            let argument = source.expected.arguments.get(field.ordinal as usize).ok_or_else(|| IrVerifyError::new("native spread field loses its original ordinal"))?;
            if [field.instruction, field.field_read, field.field_initializer, field.field_wrapper, field.field_body, field.record_read,
                field.record_initializer, field.record_source_instruction, field.record_wrapper, field.record_body].into_iter().any(|instruction| !range.contains(&(instruction as usize)))
                || argument.instruction != field.instruction || source.expected.binding.supplied_slots.get(field.ordinal as usize) != Some(&field.formal_slot)
                || argument.original.entry_index != field.entry_index as usize || argument.original.name != Some(field.field)
                || !matches!(argument.original.value, crate::sema::arguments::ArgumentValueSource::RecordField { record, field: name } if record == field.record_origin.expression && name == field.field)
                || generic.registered_instruction_origin(field.record_source_instruction, false) != Some((OperationSourceOrigin::Expression(field.record_origin), source.owner))
                || [field.field_read, field.record_read, field.field_initializer].into_iter().any(|instruction| generic.registered_instruction_origin(instruction, false).is_some()) {
                return Err(IrVerifyError::new("native spread field changes its original recipe, source or generated allocation"));
            }
            let TypeRef::Ground(actual) = argument.ty else { return Err(IrVerifyError::new("native spread field has no closed original type")); };
            let Type::Record(fields) = store.semantic.to_type(field.record_type)? else { return Err(IrVerifyError::new("native spread field loses its finite original Record")); };
            if fields.is_empty() || fields.get(&field.field) != Some(&store.semantic.to_type(actual)?) { return Err(IrVerifyError::new("native spread field changes its original membership or type")); }
            Self::verify_argument_initializer_lineage(store, generic, field.instruction, field.field_read, &field.wrappers, source.owner)?;
            Self::verify_argument_initializer_lineage(store, generic, field.record_initializer, field.record_source_instruction, &field.record_wrappers, source.owner)?;
            for (wrapper, initializer, pattern, body, slot) in [(field.record_wrapper, field.record_initializer, field.record_pattern, field.record_body, field.record_slot),
                (field.field_wrapper, field.field_initializer, field.field_pattern, field.field_body, field.field_slot)] {
                if Self::original_compiler_argument_wrapper_body(store, generic, wrapper, source.owner)? != Some(body) { return Err(IrVerifyError::new("native spread field loses its original compiler allocation")); }
                Self::verify_compiler_argument_wrapper(store, wrapper, initializer, pattern, body, slot)?;
            }
            let projection = store.payload(store.data[field.field_initializer as usize].range())?;
            if store.tags.get(field.field_read as usize) != Some(&FullTag::ExprParam) || store.payload(store.data[field.field_read as usize].range())? != [field.field_slot]
                || store.tags.get(field.record_read as usize) != Some(&FullTag::ExprParam) || store.payload(store.data[field.record_read as usize].range())? != [field.record_slot]
                || store.tags.get(field.field_initializer as usize) != Some(&FullTag::ExprField) || projection.len() != 3 || projection[0] != field.record_read
                || store.string(projection[1])? != field.field.as_str().as_str() {
                return Err(IrVerifyError::new("native spread field changes its saved record read or exact projection"));
            }
        }
        Ok(())
    }

    pub(super) fn verify_native_record_argument_scopes(store: &FullStore, tree: &super::super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        for (_, source) in generic.native_call_sources() {
            for field in source.record_arguments.iter() {
                if !tree.is_descendant(field.record_body, field.record_read)? || !tree.is_descendant(field.record_body, field.field_wrapper)?
                    || !tree.is_descendant(field.field_body, field.field_read)? || !tree.is_descendant(field.field_body, source.instruction)?
                    || tree.is_descendant(field.record_body, field.record_initializer)? || tree.is_descendant(field.field_body, field.field_initializer)? {
                    return Err(IrVerifyError::new("native spread saved record or field read escapes its original allocation scope"));
                }
            }
        }
        Ok(())
    }
}
