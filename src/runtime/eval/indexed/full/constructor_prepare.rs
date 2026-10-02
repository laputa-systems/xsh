use super::*;
use super::super::generic::{PreparedRecordConstructor, RecordConstructorRow, RecordConstructorSpread, graph_ground_type};
use crate::runtime::eval::lower::constructor_prepare::OriginalRecordConstructorSource;

pub(super) type StagedRecordConstructor = (OriginalRecordConstructorSource, u32, InstructionOwner, u32, u32, Box<[u32]>, Box<[(u32, u32, u32, u32)]>);
fn constructor_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_original_record_constructor(&mut self, row: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(source) = scratch.record_constructor_sources.get(&row) else { return Ok(()); };
        let raw = self.current_owner.ok_or_else(|| constructor_problem("record_constructor_owner"))?;
        let owner = if let Some(driver) = driver_owner_index(raw) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| constructor_problem("record_constructor_owner"))?) };
        let encoded = |row| self.active_encoded_expressions.get(&row).copied().ok_or_else(|| constructor_problem("record_constructor_original_row_missing"));
        let record = encoded(source.record)?;
        let validation = encoded(source.validation)?;
        let fields = source.fields.iter().map(|&row| encoded(row)).collect::<Result<Box<[_]>, _>>()?;
        let spreads = source.spreads.iter().map(|spread| Ok((encoded(spread.record_initializer)?, encoded(spread.record_read)?,
            encoded(spread.field_initializer)?, encoded(spread.field_read)?))).collect::<Result<Box<[_]>, IrBuildError>>()?;
        self.record_constructor_rows.push((source.clone(), instruction, owner, record, validation, fields, spreads));
        Ok(())
    }

    pub(super) fn prepare_record_constructors(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        for (source, instruction, owner, record, validation, fields, encoded_spreads) in self.record_constructor_rows.clone() {
            let plan = &source.plan;
            let original = solved.constructor_applications.get(&plan.origin).ok_or_else(|| constructor_problem("record_constructor_original_source_missing"))?;
            if original.result != plan.checked.ty || original.caller != plan.application.caller
                || solved.expressions.get(&plan.origin) != Some(&plan.checked.ty)
                || solved.expression_scope(plan.origin, original.caller).map_err(|_| constructor_problem("record_constructor_original_scope"))? != plan.checked.scope {
                return Err(constructor_problem("record_constructor_original_result_changed"));
            }
            let same_authority = match (&original.authority, &plan.application.authority) {
                (crate::sema::check::ConstructorAuthority::Record { application: left, origin: left_origin },
                    crate::sema::check::ConstructorAuthority::Record { application: right, origin: right_origin }) => left == right && left_origin == right_origin,
                _ => false,
            };
            if !same_authority || original.expectation != plan.application.expectation || original.default_slots != plan.application.default_slots
                || original.supplied.len() != plan.application.supplied.len() || original.parameters.len() != plan.application.parameters.len()
                || solved.argument_sources.get(&plan.origin).map(Vec::as_slice) != Some(plan.recipes.as_ref())
                || original.supplied.iter().zip(&plan.application.supplied).any(|(left, right)| left.value != right.value || left.actual != right.actual
                    || left.slot != right.slot || left.assignability != right.assignability || left.projection != right.projection)
                || original.parameters.iter().zip(&plan.application.parameters).any(|(left, right)| left.label != right.label || left.ty != right.ty || left.default != right.default) {
                return Err(constructor_problem("record_constructor_original_application_changed"));
            }
            for (slot, identity, source, value) in plan.defaults.iter() {
                let default = solved.constructor_defaults.get(identity).ok_or_else(|| constructor_problem("record_constructor_original_default_missing"))?;
                if default.source != *source || default.owner != identity.owner || default.slot != *slot
                    || default.value.clone().in_type(&plan.parameters.get(*slot).ok_or_else(|| constructor_problem("record_constructor_original_default_slot"))?.1) != *value {
                    return Err(constructor_problem("record_constructor_original_default_changed"));
                }
            }
            solved.graph.validate_scoped(plan.checked).map_err(|_| constructor_problem("record_constructor_original_root_owner"))?;
            let result_type = graph_ground_type(&solved.graph, original.result).map_err(|_| constructor_problem("record_constructor_original_result_scope"))?;
            if result_type != Type::Record(plan.parameters.iter().cloned().collect()) || fields.len() != plan.parameters.len() {
                return Err(constructor_problem("record_constructor_original_field_contract"));
            }
            let result = self.intern_generic_ground_type(&result_type)?;
            let mut actuals = Vec::with_capacity(plan.application.supplied.len());
            let mut spreads = Vec::with_capacity(source.spreads.len());
            for (spread, &(record_initializer, record_read, field_initializer, field_read)) in source.spreads.iter().zip(encoded_spreads.iter()) {
                solved.graph.validate_scoped(spread.checked).map_err(|_| constructor_problem("record_constructor_spread_original_owner"))?;
                if solved.expressions.get(&spread.origin) != Some(&spread.checked.ty)
                    || solved.expression_scope(spread.origin, original.caller).map_err(|_| constructor_problem("record_constructor_spread_original_scope"))? != spread.checked.scope {
                    return Err(constructor_problem("record_constructor_spread_original_endpoint"));
                }
                let record_type = graph_ground_type(&solved.graph, spread.checked.ty).map_err(|_| constructor_problem("record_constructor_spread_closed_type"))?;
                let record_type = self.intern_generic_ground_type(&record_type)?;
                let wrapper = |initializer, slot: usize| self.compiler_argument_wrappers.iter().find_map(|(&instruction, saved)|
                    (saved.owner == owner && saved.initializer == initializer && saved.slot as usize == slot).then_some(instruction))
                    .ok_or_else(|| constructor_problem("record_constructor_spread_original_wrapper"));
                spreads.push(RecordConstructorSpread { original: spread.clone(), record_type, record_initializer, record_read,
                    record_wrapper: wrapper(record_initializer, spread.record_slot)?, field_initializer, field_read,
                    field_wrapper: wrapper(field_initializer, spread.field_slot)? });
            }
            for (ordinal, argument) in original.supplied.iter().enumerate() {
                let recipe = plan.recipes.get(ordinal).ok_or_else(|| constructor_problem("record_constructor_original_recipe_missing"))?;
                let field = *fields.get(argument.slot).ok_or_else(|| constructor_problem("record_constructor_supplied_slot"))?;
                let mut operand = field;
                while self.store.tags.get(operand as usize) == Some(&FullTag::ExprCheckedValue) {
                    operand = *self.store.payload(self.store.data[operand as usize].range()).map_err(|_| constructor_problem("record_constructor_checked_payload"))?.first().ok_or_else(|| constructor_problem("record_constructor_checked_value"))?;
                }
                if matches!(recipe.value, crate::sema::arguments::ArgumentValueSource::RecordField { .. }) {
                    if !spreads.iter().any(|spread| spread.original.ordinal == ordinal && spread.field_read == operand) {
                        return Err(constructor_problem("record_constructor_original_spread_operand_changed"));
                    }
                } else {
                    let saved = self.prepared_saved_argument_bindings.get(&operand).ok_or_else(|| constructor_problem("record_constructor_original_saved_operand"))?;
                    if saved.call != plan.origin || saved.ordinal as usize != ordinal || saved.recipe != *recipe || saved.owner != owner {
                        return Err(constructor_problem("record_constructor_original_saved_operand_changed"));
                    }
                }
                let actual = graph_ground_type(&solved.graph, argument.actual).map_err(|_| constructor_problem("record_constructor_original_actual_scope"))?;
                actuals.push(self.intern_generic_ground_type(&actual)?);
            }
            let mut rows = Vec::new();
            capture_constructor_row(&self.store, instruction, &mut rows, false, &mut Vec::new()).map_err(|_| constructor_problem("record_constructor_original_result_allocation"))?;
            capture_constructor_row(&self.store, validation, &mut rows, false, &mut Vec::new()).map_err(|_| constructor_problem("record_constructor_original_validation_allocation"))?;
            capture_constructor_row(&self.store, record, &mut rows, false, &mut Vec::new()).map_err(|_| constructor_problem("record_constructor_original_record_allocation"))?;
            for &field in fields.iter() {
                if self.store.tags.get(field as usize) == Some(&FullTag::ExprCheckedValue) {
                    capture_constructor_row(&self.store, field, &mut rows, false, &mut Vec::new()).map_err(|_| constructor_problem("record_constructor_original_checked_allocation"))?;
                }
            }
            for (slot, _, _, _) in plan.defaults.iter() {
                capture_constructor_row(&self.store, fields[*slot], &mut rows, true, &mut Vec::new()).map_err(|_| constructor_problem("record_constructor_original_default_allocation"))?;
            }
            for spread in &spreads {
                let mut material = spread.record_initializer;
                while self.store.tags.get(material as usize) == Some(&FullTag::ExprCheckedValue) {
                    capture_constructor_row(&self.store, material, &mut rows, false, &mut Vec::new()).map_err(|_| constructor_problem("record_constructor_original_spread_wrapper_allocation"))?;
                    material = self.store.payload(self.store.data[material as usize].range()).map_err(|_| constructor_problem("record_constructor_original_spread_wrapper_payload"))?[0];
                }
                for row in [spread.record_read, spread.field_initializer, spread.field_read] {
                    capture_constructor_row(&self.store, row, &mut rows, false, &mut Vec::new()).map_err(|_| constructor_problem("record_constructor_original_spread_allocation"))?;
                }
            }
            self.generic_evidence_mut().add_record_constructor(PreparedRecordConstructor { original: plan.clone(), instruction, record, validation, owner, result,
                fields, actuals: actuals.into_boxed_slice(), rows: rows.into_boxed_slice(), spreads: spreads.into_boxed_slice() }).map_err(|_| constructor_problem("record_constructor_proof_allocation"))?;
        }
        Ok(())
    }
}

fn capture_constructor_row(store: &FullStore, instruction: u32, rows: &mut Vec<RecordConstructorRow>, descend: bool, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
    if active.len() >= 256 || active.contains(&instruction) || rows.len() >= 65536 { return Err(IrVerifyError::new("record constructor constant is cyclic or exceeds its preparation bound")); }
    if rows.iter().any(|row| row.instruction == instruction) { return Ok(()); }
    let tag = *store.tags.get(instruction as usize).ok_or_else(|| IrVerifyError::new("record constructor original row is missing"))?;
    let payload = store.payload(store.data[instruction as usize].range())?.to_vec().into_boxed_slice();
    let mut row = RecordConstructorRow { instruction, tag: tag as u16, payload, block: None, text: None, bytes: None, constant: None, schema: None };
    active.push(instruction);
    let mut children = Vec::new();
    match tag {
        FullTag::ExprStr => { row.text = Some(Arc::from(store.string(*row.payload.first().ok_or_else(|| IrVerifyError::new("record constructor string is missing"))?)?)); },
        FullTag::ExprBytes | FullTag::ExprPath => { row.bytes = Some(Arc::from(store.bytes(*row.payload.first().ok_or_else(|| IrVerifyError::new("record constructor bytes are missing"))?)?)); },
        FullTag::ExprPreparedConstant => { row.constant = Some(store.prepared_constants.get(*row.payload.first().ok_or_else(|| IrVerifyError::new("record constructor constant is missing"))? as usize).ok_or_else(|| IrVerifyError::new("record constructor constant is invalid"))?.0.clone().into_value()); },
        FullTag::ExprRequire => {
            if row.payload.get(3) != Some(&1) { return Err(IrVerifyError::new("record constructor validation has no prepared schema")); }
            row.schema = Some(Arc::clone(store.prepared_schemas.get(*row.payload.get(4).ok_or_else(|| IrVerifyError::new("record constructor schema is missing"))? as usize).ok_or_else(|| IrVerifyError::new("record constructor schema is invalid"))?));
            children.push(row.payload[0]);
        }
        FullTag::ExprRecord | FullTag::ExprList => {
            let id = *row.payload.first().ok_or_else(|| IrVerifyError::new("record constructor constant block is missing"))?;
            let block = IrBlockId::from_raw(id).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("record constructor constant block is invalid"))?;
            if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("record constructor constant block has another kind")); }
            let words = store.payload(block.instructions)?;
            row.block = Some((id, words.to_vec().into_boxed_slice()));
            let mut cursor = FullCursor::new(words);
            let count = cursor.raw()?;
            for _ in 0..count {
                if tag == FullTag::ExprRecord {
                    if cursor.raw()? != 0 { return Err(IrVerifyError::new("record constructor constant has an unprepared spread")); }
                    cursor.raw()?;
                }
                children.push(cursor.raw()?);
            }
            cursor.finish()?;
        }
        FullTag::ExprTry | FullTag::ExprCheckedValue => { children.push(*row.payload.first().ok_or_else(|| IrVerifyError::new("record constructor wrapper child is missing"))?); },
        FullTag::ExprNull | FullTag::ExprBool | FullTag::ExprInt | FullTag::ExprFloat | FullTag::ExprDuration | FullTag::ExprEmptyMap => {},
        _ if !descend => {},
        _ => return Err(IrVerifyError::new("record constructor default uses an unprepared constant opcode")),
    }
    rows.push(row);
    if descend { for child in children { capture_constructor_row(store, child, rows, true, active)?; } }
    active.pop();
    Ok(())
}

impl FullVerifier {
    pub(super) fn verify_record_constructors(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (_, source) in generic.record_constructors() {
            Self::verify_record_constructor_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(source.result)?, None, &mut Vec::new())?;
        }
        Ok(())
    }

    pub(super) fn verify_record_constructor_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let source = generic.record_constructor_at(instruction)?.ok_or_else(|| IrVerifyError::new("record constructor lacks its original source proof"))?;
        if source.owner != owner || store.semantic.to_type(source.result)? != *expected {
            return Err(IrVerifyError::new("record constructor changes its original owner or result"));
        }
        for row in source.rows.iter() {
            if store.tags.get(row.instruction as usize).map(|tag| *tag as u16) != Some(row.tag)
                || store.payload(store.data[row.instruction as usize].range())? != row.payload.as_ref() {
                return Err(IrVerifyError::new("record constructor changes its original supplied/default allocation"));
            }
            if let Some((id, original)) = &row.block {
                let block = IrBlockId::from_raw(*id).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("record constructor original block is invalid"))?;
                if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || store.payload(block.instructions)? != original.as_ref() { return Err(IrVerifyError::new("record constructor changes its original supplied/default block")); }
            }
            if let Some(text) = &row.text {
                if store.string(row.payload[0])? != text.as_ref() { return Err(IrVerifyError::new("record constructor changes its original constant text")); }
            }
            if let Some(bytes) = &row.bytes {
                if store.bytes(row.payload[0])? != bytes.as_ref() { return Err(IrVerifyError::new("record constructor changes its original constant bytes")); }
            }
            if let Some(constant) = &row.constant {
                if store.prepared_constants.get(row.payload[0] as usize).is_none_or(|value| value.0.clone().into_value() != *constant) { return Err(IrVerifyError::new("record constructor changes its original prepared default value")); }
            }
            if let Some(schema) = &row.schema {
                if store.prepared_schemas.get(row.payload[4] as usize).is_none_or(|actual| !Arc::ptr_eq(schema, actual))
                    || !schema.valid() || !schema.matches_type(expected) || !schema.visit_wire_mappings(&mut |_| false) {
                    return Err(IrVerifyError::new("record constructor changes its original physical validation schema"));
                }
            }
        }
        let root = store.payload(store.data[instruction as usize].range())?;
        let validation = store.payload(store.data[source.validation as usize].range())?;
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprTry) || root.first() != Some(&source.validation)
            || store.tags.get(source.validation as usize) != Some(&FullTag::ExprRequire) || validation.first() != Some(&source.record)
            || TypeId::from_raw(*validation.get(1).ok_or_else(|| IrVerifyError::new("record constructor target is missing"))?).map(|id| store.semantic.to_type(id)).transpose()?.as_ref() != Some(expected) {
            return Err(IrVerifyError::new("record constructor loses its original validation lineage"));
        }
        for (ordinal, argument) in source.original.application.supplied.iter().enumerate() {
            let actual = store.semantic.to_type(source.actuals[ordinal])?;
            let mut field = source.fields[argument.slot];
            if store.tags.get(field as usize) == Some(&FullTag::ExprCheckedValue) { field = store.payload(store.data[field as usize].range())?[0]; }
            if let Some(spread) = source.spreads.iter().find(|spread| spread.original.ordinal == ordinal) {
                let mut material = spread.record_initializer;
                let mut depth = 0;
                while store.tags.get(material as usize) == Some(&FullTag::ExprCheckedValue) {
                    if depth >= 256 { return Err(IrVerifyError::new("record constructor spread wrapper is too deep")); }
                    material = *store.payload(store.data[material as usize].range())?.first().ok_or_else(|| IrVerifyError::new("record constructor spread wrapper has no child"))?;
                    depth += 1;
                }
                if field != spread.field_read || generic.registered_instruction_origin(material, false) != Some((super::super::generic::OperationSourceOrigin::Expression(spread.original.origin), owner)) {
                    return Err(IrVerifyError::new("record constructor spread changes its original source record"));
                }
                for (wrapper, initializer, slot) in [(spread.record_wrapper, spread.record_initializer, spread.original.record_slot), (spread.field_wrapper, spread.field_initializer, spread.original.field_slot)] {
                    let saved = generic.original_compiler_argument_wrapper(wrapper)?.ok_or_else(|| IrVerifyError::new("record constructor spread loses its original compiler binding"))?;
                    if saved.owner != owner || saved.initializer != initializer || saved.slot as usize != slot {
                        return Err(IrVerifyError::new("record constructor spread changes its original compiler binding"));
                    }
                    Self::original_compiler_argument_wrapper_body(store, generic, wrapper, owner)?;
                }
                let record_read = store.payload(store.data[spread.record_read as usize].range())?;
                let field_read = store.payload(store.data[spread.field_read as usize].range())?;
                let projection = store.payload(store.data[spread.field_initializer as usize].range())?;
                if store.tags.get(spread.record_read as usize) != Some(&FullTag::ExprParam) || record_read.first().copied() != Some(spread.original.record_slot as u32)
                    || store.tags.get(spread.field_read as usize) != Some(&FullTag::ExprParam) || field_read.first().copied() != Some(spread.original.field_slot as u32)
                    || store.tags.get(spread.field_initializer as usize) != Some(&FullTag::ExprField) || projection.first() != Some(&spread.record_read)
                    || store.string(*projection.get(1).ok_or_else(|| IrVerifyError::new("record constructor spread projection key is missing"))?)? != spread.original.field.as_str().as_str() {
                    return Err(IrVerifyError::new("record constructor spread changes its original field transport"));
                }
                Self::verify_generic_source(store, generic, spread.record_initializer, owner, &store.semantic.to_type(spread.record_type)?, instance, active)?;
                continue;
            }
            let saved = generic.original_argument_binding(field).ok_or_else(|| IrVerifyError::new("record constructor supplied field has no original saved argument"))?;
            if saved.call != source.original.origin || saved.ordinal as usize != ordinal || saved.recipe != source.original.recipes[ordinal] || saved.owner != owner {
                return Err(IrVerifyError::new("record constructor changes its original source argument recipe or destination"));
            }
            Self::verify_generic_source(store, generic, field, owner, &actual, instance, active).map_err(|error|
                IrVerifyError::new(format!("record constructor supplied field {} changes its original source: {}", source.original.parameters[argument.slot].0, error.message)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "constructor_prepare/tests.rs"]
mod tests;
