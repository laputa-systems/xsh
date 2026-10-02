use super::*;
use super::super::generic::{ContainerKind, ContainerOperandOrigin, ContainerOperandRole, GroundContainerOperand, GroundContainerSource, GroundContainerCreationCheck, OriginalNamedMapKey, graph_ground_type};
use crate::sema::check::ExpressionIdentity;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildNamedMapKeyOrigin {
    pub container: ExpressionIdentity,
    pub entry_index: usize,
    pub name: Name,
    pub span: Span,
    pub checked: crate::sema::inference::TypeId,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildContainerCreationCheck {
    pub origin: ExpressionIdentity,
    pub checked: crate::sema::inference::ScopedRoot,
    pub container: BuildExprId,
}

fn problem(message: &'static str) -> IrBuildError { IrBuildError::format(message, None, 0, 0) }

fn checked_operand_wrappers(store: &FullStore, wrappers: &[super::super::generic::ValueInitializerWrapper], expected: &Type) -> Result<bool, IrVerifyError> {
    let mut checked = false;
    for wrapper in wrappers {
        if wrapper.kind != super::super::generic::ValueInitializerWrapperKind::CheckedValue { continue; }
        let ty = wrapper.payload.get(1).and_then(|&raw| TypeId::from_raw(raw)).ok_or_else(|| IrVerifyError::new("container checked operand lacks its validation type"))?;
        if store.semantic.to_type(ty)? != *expected { return Err(IrVerifyError::new("container checked operand changes its original validation type")); }
        checked = true;
    }
    Ok(checked)
}

fn container_rows(store: &FullStore, instruction: u32) -> Result<(ContainerKind, Vec<(ContainerOperandRole, u32)>, u8, Box<[u32]>), IrVerifyError> {
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 1 { return Err(IrVerifyError::new("container instruction payload is invalid")); }
    let block = IrBlockId::from_raw(words[0]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("container operand block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("container operand block has another kind")); }
    let payload = store.payload(block.instructions)?;
    let mut cursor = FullCursor::new(payload);
    let count = cursor.raw()?;
    if count > 2_000_000 { return Err(IrVerifyError::new("container operand count exceeds its work limit")); }
    let mut operands = Vec::new();
    let kind = match store.tags[instruction as usize] {
        FullTag::ExprList => {
            for index in 0..count { operands.push((ContainerOperandRole::ListItem(index), cursor.raw()?)); }
            ContainerKind::List
        }
        FullTag::ExprListBuild => {
            for index in 0..count {
                let role = match cursor.raw()? {
                    0 => ContainerOperandRole::ListItem(index),
                    1 => ContainerOperandRole::ListSplice(index),
                    _ => return Err(IrVerifyError::new("boolean payload is invalid")),
                };
                operands.push((role, cursor.raw()?));
                let location = cursor.raw()?;
                if store.locations.get(location as usize).is_none() { return Err(IrVerifyError::new("container list splice location is invalid")); }
            }
            ContainerKind::List
        }
        FullTag::ExprMapLiteral => {
            for index in 0..count {
                match cursor.raw()? {
                    0 => operands.push((ContainerOperandRole::MapSpread(index), cursor.raw()?)),
                    1 => {
                        operands.push((ContainerOperandRole::MapKey(index), cursor.raw()?));
                        operands.push((ContainerOperandRole::MapValue(index), cursor.raw()?));
                    }
                    _ => return Err(IrVerifyError::new("container map entry has an invalid key flag")),
                }
                let location = cursor.raw()?;
                if store.locations.get(location as usize).is_none() { return Err(IrVerifyError::new("container map entry location is invalid")); }
            }
            ContainerKind::Map
        }
        _ => return Err(IrVerifyError::new("container proof is attached to another opcode")),
    };
    cursor.finish()?;
    Ok((kind, operands, block.flags, payload.into()))
}

impl FullBuilder {
    pub(super) fn stage_original_container_creation_check(&mut self, expression: BuildExprId, instruction: u32, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.container_creation_checks.get(&expression) else { return Ok(()); };
        let raw = *self.active_encoded_expressions.get(&original.container).ok_or_else(|| problem("container_creation_material_missing"))?;
        let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("container_creation_payload"))?;
        if self.store.tags[instruction as usize] != FullTag::ExprCheckedValue || words.len() < 2 || words[0] != raw { return Err(problem("container_creation_material_changed")); }
        self.container_creation_check_rows.push((original.clone(), raw, instruction, owner));
        Ok(())
    }

    pub(super) fn stage_named_map_key(&mut self, original: BuildNamedMapKeyOrigin, instruction: u32, owner: InstructionOwner) -> Result<(), IrBuildError> {
        let solved = self.solved.clone().ok_or_else(|| problem("named_map_key_original_graph_missing"))?;
        let graph = &solved.graph;
        let caller = solved.expression_owners.get(&original.container).copied();
        let scope = solved.expression_scope(original.container, caller).map_err(|_| problem("named_map_key_original_scope"))?;
        let container = *solved.expressions.get(&original.container).ok_or_else(|| problem("named_map_key_original_container_missing"))?;
        let crate::sema::inference::TypeNode::Map(key, _) = graph.node(graph.resolved(container).map_err(|_| problem("named_map_key_original_container"))?).map_err(|_| problem("named_map_key_original_container"))? else { return Err(problem("named_map_key_original_container_kind")); };
        if graph.resolved(*key).map_err(|_| problem("named_map_key_original_type"))? != graph.resolved(original.checked).map_err(|_| problem("named_map_key_original_type"))? { return Err(problem("named_map_key_original_type_changed")); }
        graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: original.checked, scope }).map_err(|_| problem("named_map_key_original_certificate"))?;
        let ty = graph_ground_type(graph, original.checked).map_err(|_| problem("named_map_key_original_type_scope"))?;
        if ty != Type::Str || self.store.tags[instruction as usize] != FullTag::ExprStr { return Err(problem("named_map_key_original_material_kind")); }
        let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("named_map_key_material_payload"))?;
        if words.len() != 1 || self.store.string(words[0]).map_err(|_| problem("named_map_key_material_value"))? != original.name.as_str().as_str() { return Err(problem("named_map_key_original_material_changed")); }
        let checked = self.intern_generic_ground_type(&ty)?;
        self.generic_evidence_mut().add_named_map_key(OriginalNamedMapKey { container: original.container, entry_index: u32::try_from(original.entry_index).map_err(|_| problem("named_map_key_entry_overflow"))?, name: original.name, span: original.span, instruction, owner, checked }).map_err(|_| problem("named_map_key_original_allocation"))?;
        Ok(())
    }

    pub(super) fn prepare_ground_containers(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let origins = self.generic_expression_rows.iter().map(|&(instruction, expression, owner)| (instruction, (expression, owner))).collect::<FxHashMap<_, _>>();
        let mut creation_checks = FxHashMap::default();
        for (original, raw, wrapper, owner) in self.container_creation_check_rows.clone() {
            let (origin, original_owner) = origins.get(&raw).copied().ok_or_else(|| problem("container_creation_original_missing"))?;
            let caller = solved.expression_owners.get(&origin).copied();
            let checked = crate::sema::inference::ScopedRoot { ty: *solved.expressions.get(&origin).ok_or_else(|| problem("container_creation_original_type"))?, scope: solved.expression_scope(origin, caller).map_err(|_| problem("container_creation_original_scope"))? };
            if original.origin != origin || owner != original_owner || original.checked.ty != checked.ty || original.checked.scope != checked.scope { return Err(problem("container_creation_original_changed")); }
            solved.graph.validate_scoped(checked).map_err(|_| problem("container_creation_original_certificate"))?;
            let result = graph_ground_type(&solved.graph, checked.ty).map_err(|_| problem("container_creation_original_ground_type"))?;
            let payload = self.store.payload(self.store.data[wrapper as usize].range()).map_err(|_| problem("container_creation_original_payload"))?;
            let target = payload.get(1).and_then(|&raw| TypeId::from_raw(raw)).ok_or_else(|| problem("container_creation_original_target"))?;
            if self.store.tags[wrapper as usize] != FullTag::ExprCheckedValue || payload.len() < 2 || payload[0] != raw || self.store.semantic.to_type(target).map_err(|_| problem("container_creation_original_target"))? != result { return Err(problem("container_creation_original_validation_changed")); }
            let check = GroundContainerCreationCheck { instruction: wrapper, payload: payload.into() };
            if creation_checks.insert(raw, check).is_some() { return Err(problem("container_creation_original_ambiguous")); }
        }
        let mut named_keys = FxHashMap::default();
        if let Some(generic) = &self.generic {
            for (id, key) in generic.named_map_key_sources() {
                if named_keys.insert(key.instruction, (id, key.clone())).is_some() { return Err(problem("container_named_key_ambiguous")); }
            }
        }
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            if !matches!(self.store.tags[instruction as usize], FullTag::ExprList | FullTag::ExprListBuild | FullTag::ExprMapLiteral) { continue; }
            let Some(&checked) = solved.expressions.get(&origin) else { continue; };
            let Ok(result_type @ (Type::List(_) | Type::Map(_, _))) = graph_ground_type(&solved.graph, checked) else { continue; };
            let caller = solved.expression_owners.get(&origin).copied();
            let source_scope = solved.expression_scope(origin, caller).map_err(|_| problem("container_original_scope"))?;
            solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: checked, scope: source_scope }).map_err(|_| problem("container_original_certificate"))?;
            let (kind, rows, block_flags, block_payload) = container_rows(&self.store, instruction).map_err(|_| problem("container_original_encoded_rows"))?;
            let creation_check = creation_checks.remove(&instruction);
            let mut operands = Vec::with_capacity(rows.len());
            for (role, instruction) in rows {
                let (source_instruction, source_wrappers): (u32, Box<[super::super::generic::ValueInitializerWrapper]>) = if origins.contains_key(&instruction) || named_keys.contains_key(&instruction) {
                    (instruction, Box::new([]))
                } else { self.argument_initializer_lineage(instruction, owner)? };
                let (source, source_type) = if let Some(&(expression, child_owner)) = origins.get(&source_instruction) {
                    if child_owner != owner || solved.expression_owners.get(&expression).copied() != caller { return Err(problem("container_original_operand_owner")); }
                    let checked = *solved.expressions.get(&expression).ok_or_else(|| problem("container_original_operand_type_missing"))?;
                    let scope = solved.expression_scope(expression, caller).map_err(|_| problem("container_original_operand_scope"))?;
                    solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: checked, scope }).map_err(|_| problem("container_original_operand_certificate"))?;
                    let ty = graph_ground_type(&solved.graph, checked).map_err(|_| problem("container_original_operand_requires_scope"))?;
                    (ContainerOperandOrigin::Expression(expression), self.intern_generic_ground_type(&ty)?)
                } else {
                    let (id, key) = named_keys.get(&source_instruction).ok_or_else(|| IrBuildError::verification("container_original_operand_missing", IrVerifyError::new(format!(
                        "container {} operand {role:?} instruction {instruction} ({:?}) in {owner:?}", origin.expression.index(), self.store.tags.get(instruction as usize),
                    ))))?;
                    if key.container != origin || key.owner != owner || role != ContainerOperandRole::MapKey(key.entry_index) { return Err(problem("container_named_key_original_entry")); }
                    (ContainerOperandOrigin::NamedMapKey(*id), key.checked)
                };
                let original = self.store.semantic.to_type(source_type).map_err(|_| problem("container_original_source_type"))?;
                let expected = match (&result_type, role) {
                    (Type::List(item), ContainerOperandRole::ListItem(_)) => item.as_ref(),
                    (Type::List(_), ContainerOperandRole::ListSplice(_)) if matches!(original, Type::List(_)) => &result_type,
                    (Type::Map(key, _), ContainerOperandRole::MapKey(_)) => key.as_ref(),
                    (Type::Map(_, value), ContainerOperandRole::MapValue(_)) => value.as_ref(),
                    (Type::Map(_, _), ContainerOperandRole::MapSpread(_)) => &result_type,
                    _ => return Err(problem("container_original_operand_role")),
                };
                // The original Int source remains independent of the validation boundary.
                let ty = if creation_check.is_some() {
                    checked_operand_wrappers(&self.store, &source_wrappers, &original).map_err(|_| problem("container_original_checked_wrapper_type"))?;
                    source_type
                } else if original == Type::Int && *expected == Type::UInt {
                    if !checked_operand_wrappers(&self.store, &source_wrappers, expected).map_err(|_| problem("container_original_checked_conversion"))? { return Err(problem("container_original_checked_conversion_missing")); }
                    self.intern_generic_ground_type(expected)?
                } else {
                    checked_operand_wrappers(&self.store, &source_wrappers, &original).map_err(|_| problem("container_original_checked_wrapper_type"))?;
                    source_type
                };
                operands.push(GroundContainerOperand { origin: source, role, instruction, source_instruction, source_wrappers, source_type, ty });
            }
            let result = self.intern_generic_ground_type(&result_type)?;
            let instruction_payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("container_original_payload"))?.into();
            let scope = caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            self.generic_evidence_mut().add_ground_container(GroundContainerSource { origin, instruction, owner, scope, kind, result, creation_check, operands: operands.into_boxed_slice(), instruction_payload, block_flags, block_payload }).map_err(|_| problem("container_original_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_ground_container_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let id = generic.ground_container_at(instruction)?.ok_or_else(|| IrVerifyError::new("container lacks its original checked proof"))?;
        let source = generic.ground_container_source(id)?;
        if source.owner != owner || store.semantic.to_type(source.result)? != *expected { return Err(IrVerifyError::new("container changes its original owner or checked result")); }
        if let Some(check) = &source.creation_check {
            if store.tags.get(check.instruction as usize) != Some(&FullTag::ExprCheckedValue) || store.payload(store.data[check.instruction as usize].range())? != check.payload.as_ref()
                || check.payload.len() < 2 || check.payload[0] != instruction || check.payload[1] != source.result.raw() {
                return Err(IrVerifyError::new("container changes its original whole-value validation"));
            }
        }
        let (kind, rows, flags, payload) = container_rows(store, instruction)?;
        if kind != source.kind || flags != source.block_flags || payload != source.block_payload || store.payload(store.data[instruction as usize].range())? != source.instruction_payload.as_ref()
            || rows.len() != source.operands.len() || rows.iter().zip(&source.operands).any(|(&(role, instruction), operand)| role != operand.role || instruction != operand.instruction) {
            return Err(IrVerifyError::new("container changes its original operand order or physical encoding"));
        }
        let already_active = active.last() == Some(&instruction);
        if active.len() >= 256 || (!already_active && active.contains(&instruction)) { return Err(IrVerifyError::new("container operands are cyclic or too deep")); }
        if !already_active { active.push(instruction); }
        for operand in &source.operands {
            match operand.origin {
                ContainerOperandOrigin::Expression(_) => {
                    Self::verify_argument_initializer_lineage(store, generic, operand.instruction, operand.source_instruction, &operand.source_wrappers, owner)?;
                    let actual = store.semantic.to_type(operand.ty)?;
                    let original = store.semantic.to_type(operand.source_type)?;
                    let checked = checked_operand_wrappers(store, &operand.source_wrappers, &actual)?;
                    if original != actual && !(original == Type::Int && actual == Type::UInt && checked) { return Err(IrVerifyError::new("container operand loses its original checked UInt conversion")); }
                    Self::verify_generic_source(store, generic, operand.source_instruction, owner, &original, instance, active)?;
                }
                ContainerOperandOrigin::NamedMapKey(id) => {
                    let key = generic.named_map_key_source(id)?;
                    let words = store.payload(store.data[key.instruction as usize].range())?;
                    if store.tags[key.instruction as usize] != FullTag::ExprStr || words.len() != 1 || store.string(words[0])? != key.name.as_str().as_str() { return Err(IrVerifyError::new("named map key changes its original material value")); }
                }
            }
        }
        if !already_active { active.pop(); }
        Ok(())
    }
    pub(super) fn verify_ground_containers(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (id, _) in generic.ground_containers() {
            let source = generic.ground_container_source(id)?;
            Self::verify_ground_container_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(source.result)?, None, &mut Vec::new())?;
        }
        Ok(())
    }
}
