use super::*;
use super::super::generic::{ComprehensionRoot, ComprehensionGenerator, ComprehensionBinding, ComprehensionTarget, PreparedComprehension, PreparedOperationAuthority, OperationSourceOrigin};
use crate::sema::check::{ProducerFlowSource, ProducerFlowKind};
use crate::sema::inference::ScopedRoot;
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};

fn comprehension_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

fn checked_comprehension_binding_flow(solved: &crate::sema::check::SolvedTypes, identity: crate::sema::check::BindingIdentity, path: &[Name], item: crate::sema::check::ProducerFlowId) -> Result<crate::sema::check::ProducerFlowId, IrBuildError> {
    let binding = *solved.binding_producer_flows.get(&(identity, 0)).ok_or_else(|| comprehension_problem("comprehension_original_binding_flow"))?;
    let node = solved.producer_flows.node(binding).map_err(|_| comprehension_problem("comprehension_original_binding_flow"))?;
    if node.source != (ProducerFlowSource::Binding { identity, version: 0 }) { return Err(comprehension_problem("comprehension_original_binding_source")); }
    let ProducerFlowKind::Join { inputs } = &node.kind else { return Err(comprehension_problem("comprehension_original_binding_producer")); };
    let [input] = inputs.as_slice() else { return Err(comprehension_problem("comprehension_original_binding_producer")); };
    let mut flow = *input;
    for field in path.iter().rev() {
        let node = solved.producer_flows.node(flow).map_err(|_| comprehension_problem("comprehension_original_projected_binding_flow"))?;
        let ProducerFlowKind::Project { input, path } = &node.kind else { return Err(comprehension_problem("comprehension_original_binding_projection")); };
        if path.0.as_slice() != [crate::sema::check::ProducerPathComponent::RecordField(*field)] { return Err(comprehension_problem("comprehension_original_binding_projection_path")); }
        flow = *input;
    }
    if flow != item { return Err(comprehension_problem("comprehension_original_binding_producer")); }
    Ok(binding)
}

impl FullBuilder {
    fn comprehension_ground_root(&mut self, solved: &crate::sema::check::SolvedTypes, root: ScopedRoot) -> Result<TypeId, IrBuildError> {
        solved.graph.validate_scoped(root).map_err(|_| comprehension_problem("comprehension_original_root_scope"))?;
        let reference = self.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_closed_reference(&solved.graph, root.ty, &mut self.store.semantic, &mut self.semantic)
            .map_err(|_| comprehension_problem("comprehension_original_root_not_ground"))?;
        let TypeRef::Ground(ty) = reference else { return Err(comprehension_problem("comprehension_original_root_not_ground")); };
        Ok(ty)
    }

    pub(super) fn stage_original_comprehension(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.comprehension_origins.get(&expression) else { return Ok(()); };
        let solved = self.solved.clone().ok_or_else(|| comprehension_problem("comprehension_original_graph_missing"))?;
        let raw = self.current_owner.ok_or_else(|| comprehension_problem("comprehension_original_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| comprehension_problem("comprehension_original_owner_invalid"))?) };
        let caller = solved.expression_owners.get(&original.origin).copied();
        if solved.expressions.get(&original.origin) != Some(&original.result.ty) || solved.expression_scope(original.origin, caller).ok() != Some(original.result.scope) { return Err(comprehension_problem("comprehension_original_result_changed")); }
        let prepare_root = |builder: &mut Self, root: &super::super::super::lower::BuildComprehensionRoot| -> Result<ComprehensionRoot, IrBuildError> {
            if root.source.source != original.origin.source || root.source.namespace != original.origin.namespace
                || solved.expressions.get(&root.source) != Some(&root.ty.ty) || solved.expression_scope(root.source, caller).ok() != Some(root.ty.scope)
                || solved.expression_owners.get(&root.source).copied() != caller { return Err(comprehension_problem("comprehension_original_child_changed")); }
            let instruction = *builder.active_encoded_expressions.get(&root.row).ok_or_else(|| comprehension_problem("comprehension_original_child_not_encoded"))?;
            let ty = builder.comprehension_ground_root(&solved, root.ty)?;
            let tag = *builder.store.tags.get(instruction as usize).ok_or_else(|| comprehension_problem("comprehension_original_child_missing"))?;
            let payload = builder.store.payload(builder.store.data[instruction as usize].range()).map_err(|_| comprehension_problem("comprehension_original_child_payload"))?.to_vec().into_boxed_slice();
            Ok(ComprehensionRoot { origin: root.source, instruction, ty, tag, payload })
        };
        let value = prepare_root(self, &original.value)?;
        let key = original.key.as_ref().map(|key| prepare_root(self, key)).transpose()?;
        let result = self.comprehension_ground_root(&solved, original.result)?;
        let mut generators = Vec::new();
        for generator in &original.generators {
            let clause = solved.comprehension_operations.get(&generator.identity).ok_or_else(|| comprehension_problem("comprehension_original_generator_missing"))?;
            let operation = &clause.operation;
            let input = ScopedRoot { ty: *solved.expressions.get(&generator.iterator_source).ok_or_else(|| comprehension_problem("comprehension_original_iterator_missing"))?, scope: solved.expression_scope(generator.iterator_source, caller).map_err(|_| comprehension_problem("comprehension_original_iterator_scope"))? };
            let item = ScopedRoot { ty: operation.result, scope: solved.operation_scope(ProducerFlowSource::Comprehension(generator.identity), operation).map_err(|_| comprehension_problem("comprehension_original_item_scope"))? };
            if generator.identity.expression != original.origin
                || operation.caller != caller || generator.input.ty != input.ty || generator.input.scope != input.scope || generator.item.ty != item.ty || generator.item.scope != item.scope
                || clause.input_producer_flow != generator.input_flow || clause.item_producer_flow != generator.item_flow
                || solved.expression_producer_flows.get(&generator.iterator_source) != Some(&generator.input_flow)
                || solved.producer_flows.node(generator.item_flow).map_err(|_| comprehension_problem("comprehension_original_item_flow"))?.source != ProducerFlowSource::Comprehension(generator.identity)
                || operation.receiver.is_some() || operation.actual_arguments.len() != 1 || operation.binding.supplied_slots != [0]
                || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
                || solved.graph.resolved(operation.actual_arguments[0]).ok() != solved.graph.resolved(input.ty).ok() { return Err(comprehension_problem("comprehension_original_generator_changed")); }
            let selected = solved.graph.candidate_evidence(operation.requirement).map_err(|_| comprehension_problem("comprehension_original_selection"))?.ok_or_else(|| comprehension_problem("comprehension_original_selection_missing"))?;
            if selected.candidate != generator.selected { return Err(comprehension_problem("comprehension_original_selection_changed")); }
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| comprehension_problem("comprehension_original_authority"))? else { return Err(comprehension_problem("comprehension_original_authority_kind")); };
            if !matches!(metadata.operation, PreparedLanguageOperation::Iteration { domain: IterableDomain::List | IterableDomain::Str | IterableDomain::Bytes | IterableDomain::Map, outer_result: false }) { return Err(comprehension_problem("comprehension_original_iterable_protocol")); }
            let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
            let iterator_instruction = *self.active_encoded_expressions.get(&generator.iterator).ok_or_else(|| comprehension_problem("comprehension_original_iterator_not_encoded"))?;
            let iterator = ComprehensionRoot { origin: generator.iterator_source, instruction: iterator_instruction, ty: self.comprehension_ground_root(&solved, input)?, tag: self.store.tags[iterator_instruction as usize], payload: self.store.payload(self.store.data[iterator_instruction as usize].range()).map_err(|_| comprehension_problem("comprehension_original_iterator_payload"))?.to_vec().into_boxed_slice() };
            let mut bindings = Vec::new();
            for original_binding in &generator.bindings {
                let definition = solved.bindings.get(&original_binding.identity).ok_or_else(|| comprehension_problem("comprehension_original_binding_missing"))?;
                let ty = ScopedRoot { ty: definition.ty, scope: definition.scheme.or(item.scope) };
                if original_binding.identity.source != original.origin.source || original_binding.identity.namespace != original.origin.namespace
                    || definition.mutable || definition.owner != caller || ty.ty != original_binding.ty.ty || ty.scope != original_binding.ty.scope { return Err(comprehension_problem("comprehension_original_binding_changed")); }
                checked_comprehension_binding_flow(&solved, original_binding.identity, &original_binding.path, generator.item_flow)?;
                bindings.push(ComprehensionBinding { identity: original_binding.identity, ty: self.comprehension_ground_root(&solved, ty)?, slot: u32::try_from(original_binding.slot).map_err(|_| comprehension_problem("comprehension_original_slot"))?, path: original_binding.path.clone() });
            }
            generators.push(ComprehensionGenerator { origin: generator.identity, iterator, item: self.comprehension_ground_root(&solved, item)?, bindings, target: generator.target.clone(), authority });
        }
        let filters = original.filters.iter().map(|(ordinal, root)| prepare_root(self, root).map(|root| (*ordinal, root))).collect::<Result<Vec<_>, _>>()?;
        let mut filter_authorities = Vec::new();
        for (_, root) in &original.filters {
            let authority = if let Some(operation) = solved.operations.get(&root.source) {
                let selected = solved.graph.candidate_evidence(operation.requirement).map_err(|_| comprehension_problem("comprehension_original_filter_selection"))?.ok_or_else(|| comprehension_problem("comprehension_original_filter_selection_missing"))?;
                match solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| comprehension_problem("comprehension_original_filter_authority"))? {
                    crate::sema::check::SolvedOperationAuthority::Language(metadata) => Some(PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit }),
                    _ => None,
                }
            } else { None };
            filter_authorities.push(authority);
        }
        let mut reads = Vec::new();
        for (root, binding) in &original.reads {
            let (generator, definition) = original.generators.iter().find_map(|generator| generator.bindings.iter().find(|candidate| candidate.identity == *binding).map(|binding| (generator, binding))).ok_or_else(|| comprehension_problem("comprehension_original_read_binding"))?;
            let binding_flow = checked_comprehension_binding_flow(&solved, *binding, &definition.path, generator.item_flow)?;
            let mut flow = *solved.expression_producer_flows.get(&root.source).ok_or_else(|| comprehension_problem("comprehension_original_read_flow"))?;
            if solved.producer_flows.node(flow).map_err(|_| comprehension_problem("comprehension_original_read_flow"))?.source != ProducerFlowSource::Expression(root.source) { return Err(comprehension_problem("comprehension_original_read_source")); }
            let mut matched = false;
            for _ in 0..=256 {
                let node = solved.producer_flows.node(flow).map_err(|_| comprehension_problem("comprehension_original_read_flow"))?;
                if node.source == (ProducerFlowSource::Binding { identity: *binding, version: 0 }) {
                    if flow != binding_flow { return Err(comprehension_problem("comprehension_original_binding_producer")); }
                    matched = true; break;
                }
                match &node.kind {
                    ProducerFlowKind::CapturedBinding { identity, version: 0, input } if *identity == *binding => { flow = *input; }
                    ProducerFlowKind::Join { inputs } => { let [input] = inputs.as_slice() else { break; }; flow = *input; }
                    _ => break,
                }
            }
            if !matched { return Err(comprehension_problem("comprehension_original_read_lineage")); }
            reads.push((prepare_root(self, root)?, *binding));
        }
        let tag = if key.is_some() { FullTag::ExprMapComp } else { FullTag::ExprListComp };
        if self.store.tags.get(instruction as usize) != Some(&tag) { return Err(comprehension_problem("comprehension_original_instruction_changed")); }
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| comprehension_problem("comprehension_original_payload"))?.to_vec().into_boxed_slice();
        let qualifier_block = payload.get(if key.is_some() { 2 } else { 1 }).copied().and_then(IrBlockId::from_raw).ok_or_else(|| comprehension_problem("comprehension_original_qualifiers"))?;
        let block = self.store.blocks.get(qualifier_block.index()).ok_or_else(|| comprehension_problem("comprehension_original_qualifiers"))?;
        let qualifier_payload = self.store.payload(block.instructions).map_err(|_| comprehension_problem("comprehension_original_qualifiers"))?.to_vec().into_boxed_slice();
        let receipt = PreparedComprehension { origin: original.origin, instruction, owner, result, value, key, generators, filters, filter_authorities, reads, payload, qualifier_block, qualifier_payload };
        Self::validate_comprehension_encoding(&self.store, &receipt).map_err(|_| comprehension_problem("comprehension_original_encoding_changed"))?;
        self.generic_evidence_mut().add_comprehension(receipt).map_err(|_| comprehension_problem("comprehension_original_receipt_capacity"))
    }

    fn validate_comprehension_encoding(store: &FullStore, value: &PreparedComprehension) -> Result<(), IrVerifyError> { FullVerifier::validate_comprehension_encoding(store, value) }
}

impl FullVerifier {
    fn validate_comprehension_target(store: &FullStore, input: &mut FullCursor<'_>, target: &ComprehensionTarget, bindings: &[ComprehensionBinding], depth: usize) -> Result<(), IrVerifyError> {
        if depth >= 256 { return Err(IrVerifyError::new("comprehension target exceeds its nesting limit")); }
        match target {
            ComprehensionTarget::Discard => { if input.raw()? != 2 { return Err(IrVerifyError::new("comprehension target changes its original discard")); } }
            ComprehensionTarget::Slot(index) => {
                let binding = bindings.get(*index as usize).ok_or_else(|| IrVerifyError::new("comprehension target loses its original binding"))?;
                if input.raw()? != 0 || input.raw()? != binding.slot { return Err(IrVerifyError::new("comprehension target changes its original item slot")); }
            }
            ComprehensionTarget::Record(fields) => {
                if input.raw()? != 1 || input.raw()? as usize != fields.len() { return Err(IrVerifyError::new("comprehension target changes its original record fields")); }
                for (name, child) in fields {
                    if Name::from_symbol(Symbol::from_raw(input.raw()?)) != *name { return Err(IrVerifyError::new("comprehension target changes its original field name")); }
                    Self::validate_comprehension_target(store, input, child, bindings, depth + 1)?;
                    if store.locations.get(input.raw()? as usize).is_none() { return Err(IrVerifyError::new("comprehension target field location is invalid")); }
                }
            }
        }
        Ok(())
    }

    fn validate_comprehension_encoding(store: &FullStore, value: &PreparedComprehension) -> Result<(), IrVerifyError> {
        for root in std::iter::once(&value.value).chain(value.key.iter()).chain(value.generators.iter().map(|generator| &generator.iterator)).chain(value.filters.iter().map(|(_, root)| root)).chain(value.reads.iter().map(|(root, _)| root)) {
            if store.tags.get(root.instruction as usize) != Some(&root.tag) || store.payload(store.data[root.instruction as usize].range())? != root.payload.as_ref() { return Err(IrVerifyError::new("comprehension child changes its original producer")); }
        }
        let (tag, value_index, qualifier_index) = if let Some(key) = &value.key {
            if value.payload.first() != Some(&key.instruction) { return Err(IrVerifyError::new("comprehension changes its original map key")); }
            (FullTag::ExprMapComp, 1, 2)
        } else { (FullTag::ExprListComp, 0, 1) };
        if store.tags.get(value.instruction as usize) != Some(&tag)
            || store.payload(store.data[value.instruction as usize].range())? != value.payload.as_ref()
            || value.payload.get(value_index) != Some(&value.value.instruction) || value.payload.get(qualifier_index) != Some(&value.qualifier_block.raw()) { return Err(IrVerifyError::new("comprehension changes its original value or qualifier block")); }
        let block = store.blocks.get(value.qualifier_block.index()).ok_or_else(|| IrVerifyError::new("comprehension qualifier block is missing"))?;
        if block.flags != BLOCK_LIST || store.payload(block.instructions)? != value.qualifier_payload.as_ref() { return Err(IrVerifyError::new("comprehension changes its original qualifier sequence")); }
        let mut input = FullCursor::new(&value.qualifier_payload);
        let count = input.raw()?;
        if count as usize != value.generators.len() + value.filters.len() { return Err(IrVerifyError::new("comprehension qualifier count differs from its original source")); }
        for ordinal in 0..count {
            if let Some(generator) = value.generators.iter().find(|generator| generator.origin.qualifier == ordinal) {
                if input.raw()? != 0 { return Err(IrVerifyError::new("comprehension generator changes its original target kind")); }
                Self::validate_comprehension_target(store, &mut input, &generator.target, &generator.bindings, 0)?;
                if input.raw()? != generator.iterator.instruction { return Err(IrVerifyError::new("comprehension generator changes its original iterator")); }
                input.raw()?;
            } else {
                let (_, filter) = value.filters.iter().find(|(index, _)| *index == ordinal).ok_or_else(|| IrVerifyError::new("comprehension qualifier loses its original source"))?;
                if input.raw()? != 1 || input.raw()? != filter.instruction { return Err(IrVerifyError::new("comprehension filter changes its original condition")); }
                input.raw()?;
            }
        }
        input.finish()?;
        Ok(())
    }

    pub(super) fn verify_original_comprehensions(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        let owners = store.generic_instruction_owners()?;
        let mut assigned = std::collections::BTreeSet::new();
        for (instruction, owner) in owners.iter().enumerate() {
            if matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath)
                && let (Some(owner), Some(&slot)) = (owner, store.payload(store.data[instruction].range())?.first()) {
                let key = match owner { InstructionOwner::Function(function) => (false, function.raw(), slot), InstructionOwner::Driver(driver) => (true, *driver, slot) };
                assigned.insert(key);
            }
        }
        for value in generic.comprehensions() {
            let value = generic.comprehension_at(value.instruction)?.ok_or_else(|| IrVerifyError::new("comprehension original receipt is missing"))?;
            Self::validate_comprehension_encoding(store, value)?;
            for generator in &value.generators {
                if !tree.is_descendant(value.instruction, generator.iterator.instruction)? { return Err(IrVerifyError::new("comprehension iterator is outside its original producer")); }
                Self::verify_generic_source(store, generic, generator.iterator.instruction, value.owner, &store.semantic.to_type(generator.iterator.ty)?, None, &mut Vec::new())?;
            }
            if !tree.is_descendant(value.instruction, value.value.instruction)? { return Err(IrVerifyError::new("comprehension value is outside its original producer")); }
            if let Some(key) = &value.key && !tree.is_descendant(value.instruction, key.instruction)? { return Err(IrVerifyError::new("comprehension key is outside its original producer")); }
            for (_, filter) in &value.filters { if !tree.is_descendant(value.instruction, filter.instruction)? { return Err(IrVerifyError::new("comprehension filter is outside its original producer")); } }
            for binding in value.generators.iter().flat_map(|generator| &generator.bindings) {
                let key = match value.owner { InstructionOwner::Function(function) => (false, function.raw(), binding.slot), InstructionOwner::Driver(driver) => (true, driver, binding.slot) };
                if assigned.contains(&key) { return Err(IrVerifyError::new("comprehension item is mutable without an assignment proof")); }
            }
            for (read, binding) in &value.reads {
                let (generator, binding) = value.generators.iter().find_map(|generator| generator.bindings.iter().find(|candidate| candidate.identity == *binding).map(|binding| (generator, binding))).ok_or_else(|| IrVerifyError::new("comprehension item loses its original generator"))?;
                if store.tags.get(read.instruction as usize) != Some(&FullTag::ExprParam) || store.payload(store.data[read.instruction as usize].range())? != [binding.slot] { return Err(IrVerifyError::new("comprehension read changes its original item slot")); }
                let mut visible = tree.is_descendant(value.value.instruction, read.instruction)?;
                if let Some(key) = &value.key && tree.is_descendant(key.instruction, read.instruction)? { visible = true; }
                for (ordinal, root) in &value.filters { if *ordinal > generator.origin.qualifier && tree.is_descendant(root.instruction, read.instruction)? { visible = true; } }
                for later in &value.generators { if later.origin.qualifier > generator.origin.qualifier && tree.is_descendant(later.iterator.instruction, read.instruction)? { visible = true; } }
                if !visible || tree.is_descendant(generator.iterator.instruction, read.instruction)? { return Err(IrVerifyError::new("comprehension item read is outside its original generator continuation")); }
            }
            Self::verify_generic_source(store, generic, value.value.instruction, value.owner, &store.semantic.to_type(value.value.ty)?, None, &mut Vec::new())?;
            if let Some(key) = &value.key { Self::verify_generic_source(store, generic, key.instruction, value.owner, &store.semantic.to_type(key.ty)?, None, &mut Vec::new())?; }
            for ((_, filter), authority) in value.filters.iter().zip(&value.filter_authorities) {
                if let Some(authority) = authority {
                    let operation = generic.operation_at(filter.instruction)?.ok_or_else(|| IrVerifyError::new("comprehension filter loses its original selected operation"))?;
                    let source = generic.operation_source(operation.source)?;
                    if source.owner != value.owner || source.origin != OperationSourceOrigin::Expression(filter.origin) || operation.result != TypeRef::Ground(filter.ty) || operation.authority != *authority { return Err(IrVerifyError::new("comprehension filter changes its checked operation result")); }
                } else { Self::verify_generic_source(store, generic, filter.instruction, value.owner, &Type::Bool, None, &mut Vec::new())?; }
            }
        }
        Ok(())
    }

    pub(super) fn verify_comprehension_source(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, _instance: Option<InstantiationId>, _active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        if let Some(value) = generic.comprehension_at(instruction)? {
            if value.owner != owner || store.semantic.to_type(value.result)? != *expected { return Err(IrVerifyError::new("comprehension producer changes its checked result or owner")); }
            Self::validate_comprehension_encoding(store, value)?;
            return Ok(true);
        }
        if let Some((value, read, binding)) = generic.comprehension_read_at(instruction)? {
            let binding = value.generators.iter().flat_map(|generator| &generator.bindings).find(|candidate| candidate.identity == binding).ok_or_else(|| IrVerifyError::new("comprehension read loses its original binding"))?;
            if value.owner != owner || store.semantic.to_type(read.ty)? != *expected || store.tags.get(instruction as usize) != Some(&FullTag::ExprParam)
                || store.payload(store.data[instruction as usize].range())? != [binding.slot]
                || generic.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(read.origin), owner)) { return Err(IrVerifyError::new("comprehension operand changes its original item contract")); }
            return Ok(true);
        }
        Ok(false)
    }
}
