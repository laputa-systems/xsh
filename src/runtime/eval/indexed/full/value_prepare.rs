use super::*;
use super::super::generic::{PreparedValueBinding, ValueBindingContract, ValueBindingAllocation, ValueBindingIdentity, OperationSourceOrigin, ValueBindingSource, ValueBindingUse, ValueInitializerWrapper, ValueInitializerWrapperKind, graph_ground_type};
use super::callable_prepare::CallableLexicalIndex;

#[path = "value_prepare/field_presence.rs"]
mod field_presence;

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
        let binding = if let Some(&binding) = scratch.value_binding_uses.get(&origin) {
            if !scratch.value_binding_origins.contains_key(&binding) { return Err(value_problem("value_read_original_binding_missing")); }
            ValueBindingIdentity::Named(binding)
        } else if let Some(&binding) = scratch.with_value_binding_uses.get(&origin) {
            if !scratch.with_value_binding_origins.contains_key(&binding) { return Err(value_problem("with_read_original_binding_missing")); }
            ValueBindingIdentity::With(binding)
        } else if let Some(&binding) = scratch.guard_error_binding_uses.get(&origin) {
            if !scratch.guard_error_binding_origins.contains_key(&binding) { return Err(value_problem("guard_error_read_original_binding_missing")); }
            ValueBindingIdentity::GuardError(binding)
        } else { return Ok(()); };
        self.value_use_rows.push((OperationSourceOrigin::Expression(origin), binding, instruction, value_owner(self.current_owner)?));
        Ok(())
    }

    pub(super) fn stage_value_typed_use(&mut self, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if !matches!(self.store.tags.get(instruction as usize), Some(FullTag::IntSlot | FullTag::BoolSlot)) { return Ok(()); }
        let binding = if let Some(&binding) = scratch.value_binding_uses.get(&origin) {
            if !scratch.value_binding_origins.contains_key(&binding) { return Err(value_problem("value_read_original_binding_missing")); }
            ValueBindingIdentity::Named(binding)
        } else if let Some(&binding) = scratch.with_value_binding_uses.get(&origin) {
            if !scratch.with_value_binding_origins.contains_key(&binding) { return Err(value_problem("with_read_original_binding_missing")); }
            ValueBindingIdentity::With(binding)
        } else if let Some(&binding) = scratch.guard_error_binding_uses.get(&origin) {
            if !scratch.guard_error_binding_origins.contains_key(&binding) { return Err(value_problem("guard_error_read_original_binding_missing")); }
            ValueBindingIdentity::GuardError(binding)
        } else { return Ok(()); };
        self.value_use_rows.push((OperationSourceOrigin::Expression(origin), binding, instruction, owner));
        Ok(())
    }

    pub(super) fn stage_value_statement_use(&mut self, instruction: u32, statement: crate::sema::check::StatementIdentity, binding: crate::sema::check::BindingIdentity, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if !matches!(self.store.tags.get(instruction as usize), Some(FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot)) { return Ok(()); }
        if !scratch.value_binding_origins.contains_key(&binding) { return Err(value_problem("value_statement_read_original_binding_missing")); }
        let owner = value_owner(self.current_owner)?;
        let origin = OperationSourceOrigin::Statement(statement);
        self.generic_evidence_mut().register_instruction_origin(instruction, origin, owner).map_err(|_| value_problem("value_statement_read_original_source"))?;
        self.value_use_rows.push((origin, ValueBindingIdentity::Named(binding), instruction, owner));
        Ok(())
    }

    pub(super) fn stage_with_value_statement_use(&mut self, instruction: u32, statement: crate::sema::check::StatementIdentity, binding: crate::sema::check::WithBindingIdentity, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if !matches!(self.store.tags.get(instruction as usize), Some(FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot)) { return Ok(()); }
        if !scratch.with_value_binding_origins.contains_key(&binding) { return Err(value_problem("with_statement_read_original_binding_missing")); }
        let owner = value_owner(self.current_owner)?;
        let origin = OperationSourceOrigin::Statement(statement);
        self.generic_evidence_mut().register_instruction_origin(instruction, origin, owner).map_err(|_| value_problem("with_statement_read_original_source"))?;
        self.value_use_rows.push((origin, ValueBindingIdentity::With(binding), instruction, owner));
        Ok(())
    }

    pub(super) fn stage_guard_error_statement_use(&mut self, instruction: u32, statement: crate::sema::check::StatementIdentity, binding: crate::sema::check::GuardErrorBindingIdentity, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if !matches!(self.store.tags.get(instruction as usize), Some(FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot)) { return Ok(()); }
        if !scratch.guard_error_binding_origins.contains_key(&binding) { return Err(value_problem("guard_error_statement_read_original_binding_missing")); }
        let owner = value_owner(self.current_owner)?;
        let origin = OperationSourceOrigin::Statement(statement);
        self.generic_evidence_mut().register_instruction_origin(instruction, origin, owner).map_err(|_| value_problem("guard_error_statement_read_original_source"))?;
        self.value_use_rows.push((origin, ValueBindingIdentity::GuardError(binding), instruction, owner));
        Ok(())
    }

    pub(super) fn stage_value_statement_binding(&mut self, row: BuildStmtId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        for (&control, &original_row) in &scratch.field_presence_controls {
            if original_row == row {
                if self.store.tags.get(instruction as usize) != Some(&FullTag::StmtIf) { return Err(value_problem("field_presence_original_control_changed")); }
                let owner = value_owner(self.current_owner)?;
                self.generic_evidence_mut().register_instruction_origin(instruction, OperationSourceOrigin::Statement(control), owner)
                    .map_err(|_| value_problem("field_presence_control_source_registration"))?;
                self.field_presence_control_rows.push((control, instruction, owner));
            }
        }
        if let Some((binding, original)) = self.active_guard_error_bindings.get(&row).cloned() {
            if !matches!(scratch.statements.get(row.index()), Some(BuildStmtRow::Guard { target: LoweredCompTarget::Slot(_), value, else_param_slot: Some(slot), .. }) if *value == original.initializer && *slot == original.slot)
                || binding.statement != original.statement { return Err(value_problem("guard_error_original_allocation_changed")); }
            let initializer = *self.active_encoded_expressions.get(&original.initializer).ok_or_else(|| value_problem("guard_error_initializer_missing"))?;
            self.value_binding_rows.push((ValueBindingIdentity::GuardError(binding), original, instruction, initializer, value_owner(self.current_owner)?));
        }
        if let Some(originals) = self.active_with_value_bindings.get(&row).cloned() {
            let BuildStmtRow::With { bindings, .. } = scratch.statements.get(row.index()).ok_or_else(|| value_problem("with_binding_statement_missing"))? else { return Err(value_problem("with_binding_statement_changed")); };
            for (binding, original) in originals {
                let &(slot, value) = bindings.get(binding.ordinal as usize).ok_or_else(|| value_problem("with_binding_ordinal_changed"))?;
                if slot != original.slot || value != original.initializer || binding.statement != original.statement { return Err(value_problem("with_binding_original_allocation_changed")); }
                let initializer = *self.active_encoded_expressions.get(&value).ok_or_else(|| value_problem("with_binding_initializer_missing"))?;
                self.value_binding_rows.push((ValueBindingIdentity::With(binding), original, instruction, initializer, value_owner(self.current_owner)?));
            }
            return Ok(());
        }
        let Some((binding, original)) = self.active_value_bindings.get(&row).cloned() else { return Ok(()); };
        let statement = scratch.statements.get(row.index()).ok_or_else(|| value_problem("value_binding_statement_changed"))?;
        let initializer = match statement {
            BuildStmtRow::Let { slot, value } if *slot == original.slot && *value == original.initializer => {
                *self.active_encoded_expressions.get(value).ok_or_else(|| value_problem("value_binding_initializer_missing"))?
            }
            BuildStmtRow::LetInt { slot, value } if *slot == original.slot
                && scratch.int_expression_origins.get(value) == Some(&original.initializer_source) => {
                if self.store.tags.get(instruction as usize) != Some(&FullTag::StmtLetInt) { return Err(value_problem("value_binding_allocation_changed")); }
                let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| value_problem("value_binding_initializer_payload"))?;
                if words.len() != 2 || words[0] as usize != original.slot { return Err(value_problem("value_binding_allocation_changed")); }
                words[1]
            }
            BuildStmtRow::LetBool { slot, value } if *slot == original.slot
                && scratch.bool_expression_origins.get(value) == Some(&original.initializer_source) => {
                if self.store.tags.get(instruction as usize) != Some(&FullTag::StmtLetBool) { return Err(value_problem("value_binding_allocation_changed")); }
                let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| value_problem("value_binding_initializer_payload"))?;
                if words.len() != 2 || words[0] as usize != original.slot { return Err(value_problem("value_binding_allocation_changed")); }
                words[1]
            }
            BuildStmtRow::Guard { target: LoweredCompTarget::Slot(slot), value, .. } if *slot == original.slot && *value == original.initializer => {
                *self.active_encoded_expressions.get(value).ok_or_else(|| value_problem("value_binding_initializer_missing"))?
            }
            _ => return Err(value_problem("value_binding_allocation_changed")),
        };
        self.value_binding_rows.push((ValueBindingIdentity::Named(binding), original, instruction, initializer, value_owner(self.current_owner)?));
        Ok(())
    }

    pub(super) fn prepare_value_bindings(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let mut applications = BTreeMap::new();
        let saved_wrappers: BTreeMap<_, _> = self.prepared_saved_argument_bindings.values().map(|saved| (saved.wrapper, saved.clone())).collect();
        for (binding, original, instruction, initializer, owner) in self.value_binding_rows.clone() {
            solved.graph.validate_scoped(original.source_type).map_err(|_| value_problem("value_binding_original_scope"))?;
            solved.graph.validate_scoped(original.initializer_type).map_err(|_| value_problem("value_initializer_original_scope"))?;
            let (definition_owner, definition_type, definition_scope, definition_mutable) = match binding {
                ValueBindingIdentity::Named(binding) => {
                    let definition = solved.bindings.get(&binding).ok_or_else(|| value_problem("value_binding_original_definition_missing"))?;
                    let lexical = definition.owner.and_then(|owner| solved.declarations.get(&owner).map(|declaration| declaration.scheme));
                    (definition.owner, definition.ty, definition.scheme.or(lexical), definition.mutable)
                }
                ValueBindingIdentity::With(binding) => {
                    let definition = solved.with_bindings.get(&binding).ok_or_else(|| value_problem("with_binding_original_definition_missing"))?;
                    if binding.statement != original.statement || definition.initializer != original.initializer_source
                        || definition.initializer_type != original.initializer_type.ty { return Err(value_problem("with_binding_original_initializer_changed")); }
                    let lexical = definition.owner.and_then(|owner| solved.declarations.get(&owner).map(|declaration| declaration.scheme));
                    (definition.owner, definition.binding_type, lexical, false)
                }
                ValueBindingIdentity::GuardError(binding) => {
                    let definition = solved.guard_error_bindings.get(&binding).ok_or_else(|| value_problem("guard_error_original_definition_missing"))?;
                    if binding.statement != original.statement || definition.initializer != original.initializer_source
                        || definition.initializer_type != original.initializer_type.ty { return Err(value_problem("guard_error_original_initializer_changed")); }
                    let lexical = definition.owner.and_then(|owner| solved.declarations.get(&owner).map(|declaration| declaration.scheme));
                    (definition.owner, definition.binding_type, lexical, false)
                }
            };
            let initializer_scope = solved.expression_scope(original.initializer_source, definition_owner).map_err(|_| value_problem("value_initializer_original_scope"))?;
            if definition_mutable || definition_type != original.source_type.ty
                || original.source_type.scope != definition_scope || original.initializer_type.scope != initializer_scope
                || solved.expressions.get(&original.initializer_source) != Some(&original.initializer_type.ty)
                || solved.expression_owners.get(&original.initializer_source).copied() != definition_owner {
                return Err(value_problem("value_binding_original_definition_changed"));
            }
            let binding_type = graph_ground_type(&solved.graph, original.source_type.ty).map_err(|_| value_problem("value_binding_requires_scope"))?;
            let initializer_type = graph_ground_type(&solved.graph, original.initializer_type.ty).map_err(|_| value_problem("value_initializer_requires_scope"))?;
            let binding_type = self.intern_generic_ground_type(&binding_type)?;
            let initializer_type = self.intern_generic_ground_type(&initializer_type)?;
            let scope = definition_owner.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let range = match owner { InstructionOwner::Function(function) => self.store.function_instruction_range(function.index()), InstructionOwner::Driver(driver) => self.store.driver_instruction_range(driver as usize) }.map_err(|_| value_problem("value_initializer_owner_range"))?;
            let mut initializer_source_instruction = initializer;
            let mut wrappers = Vec::new();
            loop {
                let tag = self.store.tags.get(initializer_source_instruction as usize);
                let saved = saved_wrappers.get(&initializer_source_instruction);
                let compiler = self.compiler_argument_wrapper(initializer_source_instruction, owner)?;
                let annotation_try = tag == Some(&FullTag::ExprTry)
                    && !self.generic_expression_rows.iter().any(|(instruction, _, _)| *instruction == initializer_source_instruction)
                    && self.store.payload(self.store.data[initializer_source_instruction as usize].range()).ok().and_then(|words| words.first()).is_some_and(|&child| self.store.tags.get(child as usize) == Some(&FullTag::ExprRequire));
                let annotation_require = tag == Some(&FullTag::ExprRequire)
                    && wrappers.last().is_some_and(|wrapper: &ValueInitializerWrapper| wrapper.kind == ValueInitializerWrapperKind::CheckedBindingTry && wrapper.payload.first() == Some(&initializer_source_instruction));
                if tag != Some(&FullTag::ExprCheckedValue) && saved.is_none() && compiler.is_none() && !annotation_try && !annotation_require { break; }
                if !range.contains(&(initializer_source_instruction as usize)) || wrappers.len() >= 256
                    || wrappers.iter().any(|wrapper: &ValueInitializerWrapper| wrapper.instruction == initializer_source_instruction) { return Err(value_problem("value_initializer_wrapper_depth_or_owner")); }
                let payload = self.store.payload(self.store.data[initializer_source_instruction as usize].range()).map_err(|_| value_problem("value_initializer_wrapper_payload"))?.to_vec().into_boxed_slice();
                let (child, kind) = if tag == Some(&FullTag::ExprCheckedValue) {
                    (*payload.first().ok_or_else(|| value_problem("value_initializer_wrapper_empty"))?, ValueInitializerWrapperKind::CheckedValue)
                } else if annotation_try {
                    (*payload.first().ok_or_else(|| value_problem("value_initializer_annotation_empty"))?, ValueInitializerWrapperKind::CheckedBindingTry)
                } else if annotation_require {
                    (*payload.first().ok_or_else(|| value_problem("value_initializer_annotation_empty"))?, ValueInitializerWrapperKind::CheckedBindingRequire)
                } else if let Some(compiler) = compiler {
                    (compiler.body, ValueInitializerWrapperKind::CompilerArgument { initializer: compiler.initializer, pattern: compiler.pattern, body: compiler.body, slot: compiler.slot })
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
            let mut with_bindings: Box<[u32]> = Box::new([]);
            let allocation = match self.store.tags.get(instruction as usize) {
                Some(FullTag::StmtLet) => ValueBindingAllocation::Value,
                Some(FullTag::StmtLetInt) => ValueBindingAllocation::Integer,
                Some(FullTag::StmtLetBool) => ValueBindingAllocation::Boolean,
                Some(FullTag::StmtGuard) => {
                    let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| value_problem("value_guard_payload"))?;
                    if let ValueBindingIdentity::GuardError(_) = binding {
                        let [0, success_slot, value, 1, error_slot, failure_body, location] = words else { return Err(value_problem("guard_error_allocation_changed")); };
                        if *error_slot as usize != original.slot || *value != initializer { return Err(value_problem("guard_error_original_slot_changed")); }
                        let Type::Result(_, error) = self.store.semantic.to_type(initializer_type).map_err(|_| value_problem("guard_error_initializer_type"))? else { return Err(value_problem("guard_error_initializer_requires_result")); };
                        if *error != self.store.semantic.to_type(binding_type).map_err(|_| value_problem("guard_error_binding_type"))? { return Err(value_problem("guard_error_original_type_changed")); }
                        ValueBindingAllocation::GuardError { success_slot: *success_slot, failure_body: *failure_body, location: *location }
                    } else {
                    let (error_slot, failure_body, location) = match words {
                        [0, slot, value, 0, failure_body, location] if *slot as usize == original.slot && *value == initializer => (None, *failure_body, *location),
                        [0, slot, value, 1, error_slot, failure_body, location] if *slot as usize == original.slot && *value == initializer => (Some(*error_slot), *failure_body, *location),
                        _ => return Err(value_problem("value_guard_allocation_changed")),
                    };
                    let Type::Result(success, _) = self.store.semantic.to_type(initializer_type).map_err(|_| value_problem("value_guard_initializer_type"))? else { return Err(value_problem("value_guard_initializer_requires_result")); };
                    if *success != self.store.semantic.to_type(binding_type).map_err(|_| value_problem("value_guard_binding_type"))? { return Err(value_problem("value_guard_success_type_changed")); }
                    ValueBindingAllocation::Guard { error_slot, failure_body, location }
                    }
                }
                Some(FullTag::StmtWith) => {
                    let ValueBindingIdentity::With(binding) = binding else { return Err(value_problem("with_binding_original_identity_missing")); };
                    let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| value_problem("with_binding_payload"))?;
                    let (bindings, body, error_slot, failure_body, captures, location) = match words {
                        [bindings, body, 0, failure_body, captures, location] => (*bindings, *body, None, *failure_body, *captures, *location),
                        [bindings, body, 1, error_slot, failure_body, captures, location] => (*bindings, *body, Some(*error_slot), *failure_body, *captures, *location),
                        _ => return Err(value_problem("with_binding_allocation_changed")),
                    };
                    let block = IrBlockId::from_raw(bindings).and_then(|block| self.store.blocks.get(block.index())).ok_or_else(|| value_problem("with_binding_list_missing"))?;
                    let payload = self.store.payload(block.instructions).map_err(|_| value_problem("with_binding_list_payload"))?;
                    let offset = 1 + 2 * binding.ordinal as usize;
                    if payload.first().is_none_or(|&len| len as usize * 2 + 1 != payload.len())
                        || payload.get(offset..offset + 2) != Some(&[original.slot as u32, initializer]) { return Err(value_problem("with_binding_original_ordinal_changed")); }
                    let actual = self.store.semantic.to_type(initializer_type).map_err(|_| value_problem("with_initializer_original_type"))?;
                    let success = match actual { Type::Result(success, _) => *success, plain => plain };
                    if success != self.store.semantic.to_type(binding_type).map_err(|_| value_problem("with_binding_original_type"))? { return Err(value_problem("with_binding_success_type_changed")); }
                    with_bindings = payload.to_vec().into_boxed_slice();
                    ValueBindingAllocation::With { ordinal: binding.ordinal, bindings, body, error_slot, failure_body, captures, location }
                }
                _ => return Err(value_problem("value_binding_allocation_kind")),
            };
            let contract = ValueBindingContract { instruction, allocation, owner, slot: u32::try_from(original.slot).map_err(|_| value_problem("value_binding_slot_overflow"))?, initializer,
                initializer_source_instruction, initializer_wrappers: wrappers.into_boxed_slice(), with_bindings, binding_type, initializer_type, scope };
            let source = self.generic_evidence_mut().add_value_binding_source(ValueBindingSource { binding, statement: original.statement, initializer_source: original.initializer_source,
                source_type: original.source_type, initializer_type: original.initializer_type, expected: contract.clone() }).map_err(|_| value_problem("value_binding_source_allocation"))?;
            let application = self.generic_evidence_mut().add_value_binding(PreparedValueBinding { source, contract }).map_err(|_| value_problem("value_binding_application_allocation"))?;
            if applications.insert(binding, application).is_some() { return Err(value_problem("value_binding_original_definition_duplicate")); }
        }
        for (origin, binding, instruction, owner) in self.value_use_rows.clone() {
            let application = *applications.get(&binding).ok_or_else(|| value_problem("value_read_original_application_missing"))?;
            let presence = self.prepare_value_field_presence(origin, binding, application, instruction, owner, &solved)?;
            self.generic_evidence_mut().add_value_binding_use(ValueBindingUse { origin, application, instruction, owner, presence }).map_err(|_| value_problem("value_binding_read_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    fn guard_error_binding_dominates(store: &FullStore, tree: &super::super::pattern::PatternTree, contract: &ValueBindingContract, instruction: u32) -> Result<bool, IrVerifyError> {
        let ValueBindingAllocation::GuardError { failure_body, .. } = contract.allocation else { return Ok(false); };
        let body = IrBlockId::from_raw(failure_body).and_then(|block| store.blocks.get(block.index())).ok_or_else(|| IrVerifyError::new("Guard failure body is missing"))?;
        for &statement in store.payload(body.instructions)?.iter().skip(1) {
            if tree.is_descendant(statement, instruction)? { return Ok(true); }
        }
        Ok(false)
    }

    fn with_value_binding_dominates(store: &FullStore, tree: &super::super::pattern::PatternTree, contract: &ValueBindingContract, instruction: u32) -> Result<bool, IrVerifyError> {
        let ValueBindingAllocation::With { ordinal, body, .. } = contract.allocation else { return Ok(false); };
        let body = IrBlockId::from_raw(body).and_then(|block| store.blocks.get(block.index())).ok_or_else(|| IrVerifyError::new("with success body is missing"))?;
        for &statement in store.payload(body.instructions)?.iter().skip(1) {
            if tree.is_descendant(statement, instruction)? { return Ok(true); }
        }
        let count = *contract.with_bindings.first().ok_or_else(|| IrVerifyError::new("with original binding list is empty"))?;
        for later in ordinal + 1..count {
            let initializer = *contract.with_bindings.get(2 + 2 * later as usize).ok_or_else(|| IrVerifyError::new("with original binding list is truncated"))?;
            if tree.is_descendant(initializer, instruction)? { return Ok(true); }
        }
        Ok(false)
    }

    pub(super) fn verify_value_initializer_lineage(store: &FullStore, generic: &GenericEvidenceStore, contract: &ValueBindingContract) -> Result<(), IrVerifyError> {
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
                ValueInitializerWrapperKind::CheckedBindingTry => {
                    if store.tags.get(instruction as usize) != Some(&FullTag::ExprTry) { return Err(IrVerifyError::new("value initializer changes its annotation propagation wrapper")); }
                    let required = *wrapper.payload.first().ok_or_else(|| IrVerifyError::new("value annotation propagation wrapper is empty"))?;
                    if store.tags.get(required as usize) != Some(&FullTag::ExprRequire) { return Err(IrVerifyError::new("value annotation propagation loses its validation")); }
                    required
                }
                ValueInitializerWrapperKind::CheckedBindingRequire => {
                    if store.tags.get(instruction as usize) != Some(&FullTag::ExprRequire) { return Err(IrVerifyError::new("value initializer changes its annotation validation wrapper")); }
                    *wrapper.payload.first().ok_or_else(|| IrVerifyError::new("value annotation validation wrapper is empty"))?
                }
                ValueInitializerWrapperKind::FsRootReceiverTry => return Err(IrVerifyError::new("ordinary value initializer cannot consume a filesystem receiver propagation wrapper")),
                ValueInitializerWrapperKind::CompilerArgument { initializer, pattern, body, slot } => {
                    Self::verify_compiler_argument_wrapper(store, instruction, initializer, pattern, body, slot)?;
                    body
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
            let (tag, expected) = match contract.allocation {
                ValueBindingAllocation::Value => (FullTag::StmtLet, vec![contract.slot, contract.initializer]),
                ValueBindingAllocation::Integer => (FullTag::StmtLetInt, vec![contract.slot, contract.initializer]),
                ValueBindingAllocation::Boolean => (FullTag::StmtLetBool, vec![contract.slot, contract.initializer]),
                ValueBindingAllocation::Guard { error_slot, failure_body, location } => {
                    let Type::Result(success, _) = store.semantic.to_type(contract.initializer_type)? else { return Err(IrVerifyError::new("guard initializer loses its checked Result carrier")); };
                    if *success != store.semantic.to_type(contract.binding_type)? { return Err(IrVerifyError::new("guard binding changes its checked success type")); }
                    let mut words = vec![0, contract.slot, contract.initializer, u32::from(error_slot.is_some())];
                    if let Some(slot) = error_slot { words.push(slot); }
                    words.extend([failure_body, location]);
                    (FullTag::StmtGuard, words)
                }
                ValueBindingAllocation::GuardError { success_slot, failure_body, location } => {
                    let Type::Result(_, error) = store.semantic.to_type(contract.initializer_type)? else { return Err(IrVerifyError::new("Guard error binding loses its checked Result carrier")); };
                    if *error != store.semantic.to_type(contract.binding_type)? { return Err(IrVerifyError::new("Guard error binding changes its checked error type")); }
                    (FullTag::StmtGuard, vec![0, success_slot, contract.initializer, 1, contract.slot, failure_body, location])
                }
                ValueBindingAllocation::With { ordinal, bindings, body, error_slot, failure_body, captures, location } => {
                    let block = IrBlockId::from_raw(bindings).and_then(|block| store.blocks.get(block.index())).ok_or_else(|| IrVerifyError::new("with binding list is missing"))?;
                    let payload = store.payload(block.instructions)?;
                    let offset = 1 + 2 * ordinal as usize;
                    if payload != contract.with_bindings.as_ref()
                        || payload.get(offset..offset + 2) != Some(&[contract.slot, contract.initializer]) {
                        return Err(IrVerifyError::new("with binding changes its original ordinal, slot, or initializer"));
                    }
                    let actual = store.semantic.to_type(contract.initializer_type)?;
                    let success = match actual { Type::Result(success, _) => *success, plain => plain };
                    if success != store.semantic.to_type(contract.binding_type)? { return Err(IrVerifyError::new("with binding changes its original success type")); }
                    let mut words = vec![bindings, body, u32::from(error_slot.is_some())];
                    if let Some(slot) = error_slot { words.push(slot); }
                    words.extend([failure_body, captures, location]);
                    (FullTag::StmtWith, words)
                }
            };
            if store.tags.get(contract.instruction as usize) != Some(&tag)
                || store.payload(store.data[contract.instruction as usize].range())? != expected {
                return Err(IrVerifyError::new("value binding changes its original allocation or initializer"));
            }
        }
        for use_ in generic.value_binding_uses() {
            let use_ = generic.value_binding_use(use_.instruction)?.ok_or_else(|| IrVerifyError::new("original value read is missing"))?;
            let binding = generic.value_binding(use_.application)?;
            let visible = if matches!(binding.contract.allocation, ValueBindingAllocation::With { .. }) {
                Self::with_value_binding_dominates(store, tree, &binding.contract, use_.instruction)?
            } else if matches!(binding.contract.allocation, ValueBindingAllocation::GuardError { .. }) {
                Self::guard_error_binding_dominates(store, tree, &binding.contract, use_.instruction)?
            } else { index.dominates(tree, binding.contract.instruction, use_.instruction)? };
            if !matches!(store.tags.get(use_.instruction as usize), Some(FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot))
                || store.payload(store.data[use_.instruction as usize].range())? != [binding.contract.slot]
                || !visible {
                return Err(IrVerifyError::new("immutable value read is outside its original binding scope"));
            }
            Self::verify_value_field_presence_dominance(store, tree, use_)?;
        }
        Ok(())
    }

    pub(super) fn verify_value_scalar_source(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let tag = store.tags.get(instruction as usize).copied();
        if matches!(tag, Some(FullTag::BoolBool | FullTag::BoolNot | FullTag::BoolAnd | FullTag::BoolOr | FullTag::BoolIntCompare)) {
            if *expected != Type::Bool { return Err(IrVerifyError::new("Boolean initializer changes its checked result type")); }
            let words = store.payload(store.data[instruction as usize].range())?;
            match tag {
                Some(FullTag::BoolNot) if words.len() == 1 => Self::verify_generic_source(store, generic, words[0], owner, &Type::Bool, None, active)?,
                Some(FullTag::BoolAnd | FullTag::BoolOr) if words.len() == 2 => {
                    for &child in words { Self::verify_generic_source(store, generic, child, owner, &Type::Bool, None, active)?; }
                }
                Some(FullTag::BoolIntCompare) if words.len() == 3 => {
                    if !matches!(words.first().and_then(|&index| store.binary_ops.get(index as usize)), Some(BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge)) { return Err(IrVerifyError::new("Boolean initializer changes its comparison operation")); }
                    for &child in &words[1..] { Self::verify_generic_source(store, generic, child, owner, &Type::Int, None, active)?; }
                }
                Some(FullTag::BoolBool) => {}
                _ => return Err(IrVerifyError::new("Boolean initializer changes its physical operands")),
            }
            return Ok(true);
        }
        if !matches!(tag, Some(FullTag::IntInt | FullTag::IntBinary)) { return Ok(false); }
        if *expected != Type::Int { return Err(IrVerifyError::new("integer initializer changes its checked result type")); }
        let words = store.payload(store.data[instruction as usize].range())?;
        if tag == Some(FullTag::IntBinary) {
            if words.len() != 3 || !matches!(words.first().and_then(|&index| store.binary_ops.get(index as usize)), Some(BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem)) {
                return Err(IrVerifyError::new("integer initializer changes its arithmetic operation"));
            }
            for &child in &words[1..] { Self::verify_generic_source(store, generic, child, owner, &Type::Int, None, active)?; }
        }
        Ok(true)
    }

    pub(super) fn verify_value_bindings(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (id, _) in generic.value_bindings() {
            let binding = generic.value_binding(id)?;
            Self::verify_value_initializer_lineage(store, generic, &binding.contract)?;
            Self::verify_generic_source(store, generic, binding.contract.initializer_source_instruction, binding.contract.owner,
                &store.semantic.to_type(binding.contract.initializer_type)?, None, &mut Vec::new())?;
        }
        Ok(())
    }

    pub(super) fn verify_value_binding_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(use_) = generic.value_binding_use(instruction)? else { return Ok(false); };
        let binding = generic.value_binding(use_.application)?;
        let contract = &binding.contract;
        Self::verify_value_initializer_lineage(store, generic, contract)?;
        let checked_type = if let Some(presence) = &use_.presence {
            Self::verify_value_field_presence(store, generic, use_, presence, owner, active)?;
            store.semantic.to_type(presence.narrowed_type)?
        } else { store.semantic.to_type(contract.binding_type)? };
        if use_.owner != owner || contract.owner != owner || checked_type != *expected {
            return Err(IrVerifyError::new(format!("immutable value read changes its original owner or checked type: instruction {instruction}, use owner {:?}, binding owner {:?}, requested owner {owner:?}, binding type {:?}, expected {expected:?}, source {:?}", use_.owner, contract.owner, store.semantic.to_type(contract.binding_type)?, generic.value_binding_source(binding.source)?.binding)));
        }
        Self::verify_generic_source(store, generic, contract.initializer_source_instruction, owner, &store.semantic.to_type(contract.initializer_type)?, None, active)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::operation_prepare::tests::source_fixture;
    use crate::sema::operation_graph::PreparedLanguageOperation;
    use crate::sema::check::Checker;

    fn checked_value_fixture(source: &str) -> FullProgram {
        use crate::syntax::parser::Parser;
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("original-value-proof.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::downgrade(&checked.solved);
        let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
        drop(parsed); drop(checked); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none());
        let program = prepared.unwrap();
        FullVerifier::verify(&program).unwrap();
        program
    }

    #[test]
    fn immutable_record_membership_read_keeps_original_field_presence_refinement_after_frontend_disposal() {
        execute_value_fixture(field_presence_source(), b"1\n", ValueBindingAllocation::Value, "selected");
    }

    fn field_presence_source() -> &'static str {
        r#"type PackageName = {name: Str}
proc selected() [error] -> Str {
  let kept = json.decode("{\"name\":\"pkg\",\"version\":\"1\"}")?.require(PackageName)?
  let _ = kept.name
  if "version" in kept { return kept.get("version")?.require(Str)? }
  "none"
}
proc main() [error] { print selected() }
"#
    }

    #[test]
    fn immutable_record_membership_read_refuses_missing_foreign_and_joint_type_receipts() {
        let program = checked_value_fixture(field_presence_source());
        let foreign = checked_value_fixture(field_presence_source());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let read = generic.value_binding_uses().find(|use_| use_.presence.is_some()).unwrap().clone();
            let presence = read.presence.as_ref().unwrap();
            let binding = generic.value_binding(read.application).unwrap().clone();
            assert_ne!(presence.material_type, presence.narrowed_type);
            assert!(presence.original.writes.is_empty());
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_value_binding_use_mut(read.instruction).unwrap().presence = None;
            assert!(FullVerifier::verify(&missing).is_err());
            let mut transplanted = program.clone();
            transplanted.store.generic.as_deref_mut().unwrap().test_value_binding_use_mut(read.instruction).unwrap().presence =
                foreign.generic_evidence().unwrap().value_binding_uses().find(|use_| use_.presence.is_some()).unwrap().presence.clone();
            assert!(FullVerifier::verify(&transplanted).is_err());
            let mut rewritten = program.clone();
            let generic = rewritten.store.generic.as_deref_mut().unwrap();
            generic.test_value_binding_mut(read.application).unwrap().contract.binding_type = presence.narrowed_type;
            generic.test_value_binding_source_mut(binding.source).unwrap().expected.binding_type = presence.narrowed_type;
            let forged = generic.test_value_binding_use_mut(read.instruction).unwrap().presence.as_mut().unwrap();
            std::mem::swap(&mut forged.material_type, &mut forged.narrowed_type);
            std::mem::swap(&mut forged.original.material, &mut forged.original.narrowed);
            assert!(FullVerifier::verify(&rewritten).is_err());
        });
    }

    #[test]
    fn immutable_record_membership_read_refuses_changed_keys_and_undominated_reads() {
        let program = checked_value_fixture(field_presence_source());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let read = generic.value_binding_uses().find(|use_| use_.presence.is_some()).unwrap();
            let presence = read.presence.as_ref().unwrap();
            let mut changed = program.clone();
            let other = (0..changed.store.strings.len()).map(|index| IrStringId::new(index).unwrap().raw())
                .find(|&string| changed.store.string(string).unwrap() == "none").unwrap();
            let key = changed.store.data[presence.key as usize].range();
            changed.store.extra[key.start as usize] = other;
            assert!(FullVerifier::verify(&changed).is_err());
            let outer = generic.value_binding_uses().find(|other| other.application == read.application && other.instruction != read.instruction
                && other.instruction != presence.subject && other.presence.is_none()).unwrap();
            let inside_parent = program.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::ExprMethod
                && program.store.payload(program.store.data[*instruction].range()).unwrap().first() == Some(&read.instruction)).unwrap().0;
            let outside_parent = program.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::ExprField
                && program.store.payload(program.store.data[*instruction].range()).unwrap().first() == Some(&outer.instruction)).unwrap().0;
            let mut moved = program.clone();
            let inside = moved.store.data[inside_parent].range();
            let outside = moved.store.data[outside_parent].range();
            moved.store.extra[inside.start as usize] = outer.instruction;
            moved.store.extra[outside.start as usize] = read.instruction;
            assert!(FullVerifier::verify(&moved).is_err());
        });
    }

    #[test]
    fn immutable_record_membership_read_refuses_writes_to_its_original_material_slot() {
        let source = field_presence_source().replace("  let kept =", "  var replacement = {name: \"other\"}\n  replacement = {name: \"changed\"}\n  let kept =");
        let program = checked_value_fixture(&source);
        let generic = program.generic_evidence().unwrap();
        let read = generic.value_binding_uses().find(|use_| use_.presence.is_some()).unwrap();
        let slot = generic.value_binding(read.application).unwrap().contract.slot;
        let assignment = program.store.tags.iter().position(|tag| *tag == FullTag::StmtAssign).unwrap();
        let mut changed = program.clone();
        let payload = changed.store.data[assignment].range();
        changed.store.extra[payload.start as usize] = slot;
        assert!(FullVerifier::verify(&changed).is_err());
    }

    fn execute_value_fixture(source: &str, expected: &[u8], allocation: ValueBindingAllocation, function_name: &str) {
        use crate::runtime::eval::Evaluator;
        use crate::syntax::parser::Parser;
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("original-value-routes.xsh", source);
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
                    assert!(evidence.value_bindings().any(|(_, binding)| std::mem::discriminant(&binding.contract.allocation) == std::mem::discriminant(&allocation)));
                    drop(checked); drop(parsed);
                    assert!(solved.upgrade().is_none());
                    let execute = || {
                        assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                        evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared value program remains installed"))
                    };
                    let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern(function_name)), recursive, execute);
                    assert_eq!(output.status, 0, "{:?}", output.diagnostics);
                    assert_eq!(output.stdout, expected);
                    assert!(output.stderr.is_empty());
                });
            }
        });
    }

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
            let OperationSourceOrigin::Expression(mut origin) = use_.origin else { panic!("record field read keeps its original expression identity"); };
            origin.source = SourceId::new(99);
            changed_source.store.generic.as_deref_mut().unwrap().test_value_binding_use_mut(use_.instruction).unwrap().origin = OperationSourceOrigin::Expression(origin);
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
            assert!(matches!(use_.origin, OperationSourceOrigin::Expression(origin) if source.binding.source() == origin.source));
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
                    let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, execute);
                    assert_eq!(output.status, 0, "{:?}", output.diagnostics);
                    assert_eq!(output.stdout, b"chosen\n");
                    assert!(output.stderr.is_empty());
                    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                });
            }
        });
    }
    #[test]
    fn scalar_integer_bindings_execute_original_shadows_after_frontend_disposal() {
        use crate::runtime::eval::Evaluator;
        use crate::syntax::parser::Parser;
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value: Int) -> Int { let kept = value + value; { let kept = value - value; let _ = kept }; kept }\nprint ${selected(7)}\n";
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("scalar-binding-routes.xsh", source);
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
                    assert_eq!(evidence.value_bindings().count(), 2);
                    assert!(evidence.value_bindings().all(|(_, binding)| binding.contract.allocation == ValueBindingAllocation::Integer));
                    assert_eq!(evidence.value_binding_uses().count(), 2);
                    drop(checked); drop(parsed);
                    assert!(solved.upgrade().is_none());
                    let execute = || {
                        assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                        evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared scalar program remains installed"))
                    };
                    let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, execute);
                    assert_eq!(output.status, 0, "{:?}", output.diagnostics);
                    assert_eq!(output.stdout, b"14\n");
                    assert!(output.stderr.is_empty());
                });
            }
        });
    }

    #[test]
    fn scalar_integer_bindings_refuse_writes_and_reads_in_their_own_initializer() {
        let program = source_fixture("pure selected(value: Int) -> Int { let kept = value - value; var changing: Int = 0; changing = 2; kept + changing }\nprint ${selected(7)}\n", PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: crate::sema::operation_graph::ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, binding) = generic.value_bindings().next().unwrap();
            let mut written = program.clone();
            let write = written.store.tags.iter().position(|tag| matches!(tag, FullTag::StmtAssignInt | FullTag::StmtAssign)).unwrap();
            let payload = written.store.data[write].range();
            written.store.extra[payload.start as usize] = binding.contract.slot;
            assert!(FullVerifier::verify(&written).is_err());
            let mut self_read = program.clone();
            let allocation = self_read.store.data[binding.contract.instruction as usize].range();
            let use_ = generic.value_binding_uses().find(|use_| use_.application == generic.value_bindings().next().unwrap().0).unwrap();
            self_read.store.extra[allocation.start as usize + 1] = use_.instruction;
            assert!(FullVerifier::verify(&self_read).is_err());
        });
    }

    #[test]
    fn scalar_integer_bindings_refuse_initializer_replacement_and_lexical_reordering() {
        let program = source_fixture("pure selected(value: Int) -> Int { let first = value + value; let second = value - value; first + second }\nprint ${selected(7)}\n", PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: crate::sema::operation_graph::ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, first) = generic.value_bindings().next().unwrap();
            let (_, second) = generic.value_bindings().nth(1).unwrap();
            assert_eq!(first.contract.allocation, ValueBindingAllocation::Integer);
            let mut replaced = program.clone();
            let payload = replaced.store.data[first.contract.instruction as usize].range();
            replaced.store.extra[payload.start as usize + 1] = second.contract.initializer;
            assert!(FullVerifier::verify(&replaced).is_err());
            replaced.store.generic.as_deref_mut().unwrap().test_value_binding_mut(id).unwrap().contract.initializer = second.contract.initializer;
            replaced.store.generic.as_deref_mut().unwrap().test_value_binding_source_mut(first.source).unwrap().expected.initializer = second.contract.initializer;
            assert!(FullVerifier::verify_generic_evidence(&replaced.store).unwrap_err().message.contains("original receipt"));
            let mut reordered = program.clone();
            let block = reordered.store.blocks.iter().find(|block| block.flags & BLOCK_SEQUENCE_KIND_MASK == BLOCK_STATEMENTS
                && reordered.store.payload(block.instructions).unwrap().contains(&first.contract.instruction)
                && reordered.store.payload(block.instructions).unwrap().contains(&second.contract.instruction)).unwrap().instructions;
            let words = reordered.store.payload(block).unwrap();
            let allocation = words.iter().position(|&word| word == first.contract.instruction).unwrap();
            let tail = words.len() - 1;
            reordered.store.extra.swap(block.start as usize + allocation, block.start as usize + tail);
            assert!(FullVerifier::verify(&reordered).is_err());
            let mut kind = program.clone();
            kind.store.tags[first.contract.instruction as usize] = FullTag::StmtLet;
            assert!(FullVerifier::verify(&kind).is_err());
        });
    }

    #[test]
    fn scalar_boolean_bindings_keep_original_typed_reads_after_frontend_disposal() {
        execute_value_fixture("pure selected(value: Bool) -> Bool { let kept = !value; { let kept = value and value; let _ = kept }; kept and kept }\nprint ${selected(false)}\n", b"true\n", ValueBindingAllocation::Boolean, "selected");
    }

    #[test]
    fn scalar_boolean_bindings_refuse_changed_initializers_and_lexical_order() {
        let program = checked_value_fixture("pure selected(value: Bool) -> Bool { let first = !value; let second = value and value; first and second }\nprint ${selected(false)}\n");
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, first) = generic.value_bindings().next().unwrap();
            let (_, second) = generic.value_bindings().nth(1).unwrap();
            assert_eq!(first.contract.allocation, ValueBindingAllocation::Boolean);
            let mut swapped = program.clone();
            let payload = swapped.store.data[first.contract.instruction as usize].range();
            swapped.store.extra[payload.start as usize + 1] = second.contract.initializer;
            assert!(FullVerifier::verify(&swapped).is_err());
            let mut reordered = program.clone();
            let block = reordered.store.blocks.iter().find(|block| block.flags & BLOCK_SEQUENCE_KIND_MASK == BLOCK_STATEMENTS
                && reordered.store.payload(block.instructions).unwrap().contains(&first.contract.instruction)).unwrap().instructions;
            let words = reordered.store.payload(block).unwrap();
            let first_position = words.iter().position(|&word| word == first.contract.instruction).unwrap();
            let tail = words.len() - 1;
            reordered.store.extra.swap(block.start as usize + first_position, block.start as usize + tail);
            assert!(FullVerifier::verify(&reordered).is_err());
        });
    }

    fn guard_value_source() -> &'static str {
        "pure selected(result: Result[Int, Str], fallback: Int) -> Int { let kept = fallback - fallback; { guard let kept = result else { |_failure| return kept }; kept } }\nprint ${selected(Ok(7), 8)}\nprint ${selected(Err(\"bad\"), 8)}\n"
    }

    fn guard_failure_source() -> &'static str {
        "error LocalFailure = Missing(message: Str)\nproc selected(result: Result[Int]) -> Int { guard let kept = result else { |failure| print $failure.message; return 0 }; kept }\nprint ${selected(Ok(7))}\nprint ${selected(Err(LocalFailure.Missing(\"bad\")))}\n"
    }

    #[test]
    fn guard_failure_binding_keeps_original_error_receiver_after_frontend_disposal() {
        execute_value_fixture(guard_failure_source(), b"7\nbad\n0\n", ValueBindingAllocation::GuardError { success_slot: 0, failure_body: 0, location: 0 }, "selected");
    }

    #[test]
    fn guard_failure_binding_refuses_missing_foreign_and_coforged_receipts() {
        let program = checked_value_fixture(guard_failure_source());
        let foreign = checked_value_fixture(guard_failure_source());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, binding) = generic.value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::GuardError { .. })).unwrap();
            let (_, foreign_binding) = foreign.generic_evidence().unwrap().value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::GuardError { .. })).unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_value_bindings();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut transplanted = program.clone();
            transplanted.store.generic.as_deref_mut().unwrap().test_value_binding_mut(id).unwrap().source = foreign_binding.source;
            assert!(FullVerifier::verify(&transplanted).is_err());
            let ValueBindingAllocation::GuardError { success_slot, failure_body, location } = binding.contract.allocation else { unreachable!() };
            let mut changed = program.clone();
            let range = program.store.data[binding.contract.instruction as usize].range();
            changed.store.extra[range.start as usize + 1] = binding.contract.slot;
            assert!(FullVerifier::verify(&changed).is_err());
            let allocation = ValueBindingAllocation::GuardError { success_slot: binding.contract.slot, failure_body, location };
            changed.store.generic.as_deref_mut().unwrap().test_value_binding_mut(id).unwrap().contract.allocation = allocation;
            changed.store.generic.as_deref_mut().unwrap().test_value_binding_source_mut(binding.source).unwrap().expected.allocation = allocation;
            assert!(FullVerifier::verify_generic_evidence(&changed.store).unwrap_err().message.contains("original receipt"));
            assert_ne!(success_slot, binding.contract.slot);
        });
    }

    #[test]
    fn guard_failure_binding_refuses_error_reads_moved_into_the_success_continuation() {
        let source = "pure selected(result: Result[Int], outer: Error) -> Error { guard let kept = result else { |failure| return failure }; let _ = kept; return outer }\n";
        let program = checked_value_fixture(source);
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (error_id, error) = generic.value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::GuardError { .. })).unwrap();
            let read = generic.value_binding_uses().find(|read| read.application == error_id).unwrap();
            let outer = program.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::ExprParam
                && program.store.payload(program.store.data[*instruction].range()).unwrap() == [1]).unwrap().0 as u32;
            let error_return = program.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::StmtReturn
                && program.store.payload(program.store.data[*instruction].range()).unwrap() == [read.instruction]).unwrap().0;
            let success_statement = program.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::StmtReturn
                && program.store.payload(program.store.data[*instruction].range()).unwrap() == [outer]).unwrap().0;
            let mut moved = program.clone();
            moved.store.extra[program.store.data[error_return].range().start as usize] = outer;
            moved.store.extra[program.store.data[success_statement].range().start as usize] = read.instruction;
            let failure = FullVerifier::verify(&moved).unwrap_err();
            assert!(failure.message.contains("binding scope"), "{}", failure.message);
            assert_eq!(error.contract.binding_type, generic.value_binding_source(error.source).unwrap().expected.binding_type);
        });
    }

    #[test]
    fn guard_failure_binding_refuses_writes_to_its_original_error_slot() {
        let program = checked_value_fixture("pure selected(result: Result[Int, Str], fallback: Str) -> Str { guard let kept = result else { |failure| var changing: Str = fallback; changing = fallback; return failure }; let _ = kept; fallback }\n");
        program.symbol_owner().with_current(|| {
            let (_, error) = program.generic_evidence().unwrap().value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::GuardError { .. })).unwrap();
            let assignment = program.store.tags.iter().position(|tag| *tag == FullTag::StmtAssign).unwrap();
            let mut changed = program.clone();
            changed.store.extra[program.store.data[assignment].range().start as usize] = error.contract.slot;
            assert!(FullVerifier::verify(&changed).is_err());
        });
    }

    fn with_value_source() -> &'static str {
        "pure selected(result: Result[Int, Str], fallback: Int) -> Int { let kept = fallback - fallback; with first = result, second = first + first { return second } else { |_failure| return kept } }\nprint ${selected(Ok(7), 8)}\nprint ${selected(Err(\"bad\"), 8)}\n"
    }

    #[test]
    fn with_success_bindings_keep_sequential_initializers_and_frontend_disposal() {
        execute_value_fixture(with_value_source(), b"14\n0\n", ValueBindingAllocation::With { ordinal: 0, bindings: 0, body: 0, error_slot: None, failure_body: 0, captures: 0, location: 0 }, "selected");
    }

    #[test]
    fn with_success_bindings_refuse_missing_foreign_and_coforged_original_ordinals() {
        let program = checked_value_fixture(with_value_source());
        let foreign = checked_value_fixture(with_value_source());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, first) = generic.value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::With { ordinal: 0, .. })).unwrap();
            let (_, foreign_first) = foreign.generic_evidence().unwrap().value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::With { ordinal: 0, .. })).unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_value_bindings();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut transplanted = program.clone();
            transplanted.store.generic.as_deref_mut().unwrap().test_value_binding_mut(id).unwrap().source = foreign_first.source;
            assert!(FullVerifier::verify(&transplanted).is_err());
            let ValueBindingIdentity::With(mut identity) = generic.value_binding_source(first.source).unwrap().binding else { panic!("with binding requires an authored ordinal"); };
            identity.ordinal = 1;
            let ValueBindingAllocation::With { bindings, body, error_slot, failure_body, captures, location, .. } = first.contract.allocation else { unreachable!() };
            let mut changed = program.clone();
            let allocation = ValueBindingAllocation::With { ordinal: 1, bindings, body, error_slot, failure_body, captures, location };
            changed.store.generic.as_deref_mut().unwrap().test_value_binding_mut(id).unwrap().contract.allocation = allocation;
            let source = changed.store.generic.as_deref_mut().unwrap().test_value_binding_source_mut(first.source).unwrap();
            source.binding = ValueBindingIdentity::With(identity);
            source.expected.allocation = allocation;
            assert!(FullVerifier::verify_generic_evidence(&changed.store).unwrap_err().message.contains("original receipt"));
        });
    }

    #[test]
    fn with_success_bindings_refuse_reads_moved_into_the_failure_handler() {
        let program = checked_value_fixture(with_value_source());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (second_id, _) = generic.value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::With { ordinal: 1, .. })).unwrap();
            let success = generic.value_binding_uses().find(|use_| use_.application == second_id).unwrap();
            let outer = generic.value_binding_uses().find(|use_| matches!(generic.value_binding(use_.application).unwrap().contract.allocation, ValueBindingAllocation::Integer)).unwrap();
            let success_return = program.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::StmtReturn
                && program.store.payload(program.store.data[*instruction].range()).unwrap() == [success.instruction]).unwrap().0;
            let failure_return = program.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::StmtReturn
                && program.store.payload(program.store.data[*instruction].range()).unwrap() == [outer.instruction]).unwrap().0;
            let mut moved = program.clone();
            moved.store.extra[program.store.data[success_return].range().start as usize] = outer.instruction;
            moved.store.extra[program.store.data[failure_return].range().start as usize] = success.instruction;
            let failure = FullVerifier::verify(&moved).unwrap_err();
            assert!(failure.message.contains("binding scope"), "{}", failure.message);
        });
    }

    #[test]
    fn guard_success_bindings_keep_failure_scope_and_frontend_disposal() {
        execute_value_fixture(guard_value_source(), b"7\n0\n", ValueBindingAllocation::Guard { error_slot: None, failure_body: 0, location: 0 }, "selected");
    }

    #[test]
    fn guard_success_bindings_restore_original_authority_after_nested_shadows() {
        execute_value_fixture("pure selected(result: Result[Int, Str], fallback: Int) -> Int { let kept = fallback - fallback; { guard let kept = result else { |_failure| return kept }; { let kept = fallback + fallback; let _ = kept }; kept } }\nprint ${selected(Ok(7), 8)}\nprint ${selected(Err(\"bad\"), 8)}\n", b"7\n0\n", ValueBindingAllocation::Guard { error_slot: None, failure_body: 0, location: 0 }, "selected");
    }

    #[test]
    fn guard_success_bindings_refuse_reads_moved_into_sibling_blocks() {
        let program = checked_value_fixture("pure selected(result: Result[Int, Str], fallback: Int) -> Int { let kept = fallback - fallback; { guard let kept = result else { |_failure| return kept }; let _ = kept }; { let _ = kept }; kept }\nprint ${selected(Ok(7), 8)}\n");
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (guard_id, _) = generic.value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::Guard { .. })).unwrap();
            let success = generic.value_binding_uses().find(|use_| use_.application == guard_id).unwrap();
            let success_statement = program.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::StmtExpr
                && program.store.payload(program.store.data[*instruction].range()).unwrap().first() == Some(&success.instruction)).unwrap().0;
            let (sibling_statement, sibling_read) = program.store.tags.iter().enumerate().find_map(|(instruction, tag)| {
                if *tag != FullTag::StmtExpr { return None; }
                let value = *program.store.payload(program.store.data[instruction].range()).unwrap().first()?;
                generic.value_binding_uses().find(|use_| use_.application != guard_id && use_.instruction == value).map(|_| (instruction, value))
            }).unwrap();
            let mut moved = program.clone();
            moved.store.extra[program.store.data[success_statement].range().start as usize] = sibling_read;
            moved.store.extra[program.store.data[sibling_statement].range().start as usize] = success.instruction;
            let failure = FullVerifier::verify(&moved).unwrap_err();
            assert!(failure.message.contains("binding scope"), "{}", failure.message);
        });
    }

    #[test]
    fn guard_success_bindings_refuse_writes_to_the_encoded_success_slot() {
        let program = checked_value_fixture("pure selected(result: Result[Int, Str], fallback: Int) -> Int { guard let kept = result else { |_failure| return fallback }; var changing: Int = fallback; changing = fallback - fallback; kept + changing }\nprint ${selected(Ok(7), 8)}\n");
        program.symbol_owner().with_current(|| {
            let (_, guard) = program.generic_evidence().unwrap().value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::Guard { .. })).unwrap();
            let assignment = program.store.tags.iter().position(|tag| matches!(tag, FullTag::StmtAssign | FullTag::StmtAssignInt)).unwrap();
            let mut changed = program.clone();
            changed.store.extra[program.store.data[assignment].range().start as usize] = guard.contract.slot;
            assert!(FullVerifier::verify(&changed).is_err());
        });
    }

    #[test]
    fn guard_success_bindings_refuse_reads_moved_into_the_failure_body() {
        let program = checked_value_fixture(guard_value_source());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (guard_id, guard) = generic.value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::Guard { .. })).unwrap();
            let success = generic.value_binding_uses().find(|use_| use_.application == guard_id).unwrap();
            let outer = generic.value_binding_uses().find(|use_| use_.application != guard_id).unwrap();
            let mut moved = program.clone();
            let outer_value = outer.instruction;
            let success_return = moved.store.tags.iter().enumerate().find(|(instruction, tag)| matches!(**tag, FullTag::StmtReturn | FullTag::StmtExpr | FullTag::StmtValue)
                && moved.store.payload(moved.store.data[*instruction].range()).unwrap().first() == Some(&success.instruction)).unwrap().0;
            let failure_return = moved.store.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::StmtReturn
                && moved.store.payload(moved.store.data[*instruction].range()).unwrap() == [outer_value]).unwrap().0;
            let success_payload = moved.store.data[success_return].range();
            let failure_payload = moved.store.data[failure_return].range();
            moved.store.extra[success_payload.start as usize] = outer_value;
            moved.store.extra[failure_payload.start as usize] = success.instruction;
            let failure = FullVerifier::verify(&moved).unwrap_err();
            assert!(failure.message.contains("binding scope"), "{}", failure.message);
            assert_ne!(generic.value_binding_source(guard.source).unwrap().binding, generic.value_binding_source(generic.value_binding(outer.application).unwrap().source).unwrap().binding);
        });
    }

    #[test]
    fn guard_success_bindings_refuse_changed_failure_metadata_and_coforged_receipts() {
        let program = checked_value_fixture(guard_value_source());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, guard) = generic.value_bindings().find(|(_, binding)| matches!(binding.contract.allocation, ValueBindingAllocation::Guard { .. })).unwrap();
            let ValueBindingAllocation::Guard { error_slot, failure_body, location } = guard.contract.allocation else { unreachable!() };
            let changed_location = location.wrapping_add(1);
            let changed_allocation = ValueBindingAllocation::Guard { error_slot, failure_body, location: changed_location };
            let mut changed = program.clone();
            let payload = changed.store.data[guard.contract.instruction as usize].range();
            changed.store.extra[(payload.start + payload.len) as usize - 1] = changed_location;
            assert!(FullVerifier::verify(&changed).is_err());
            changed.store.generic.as_deref_mut().unwrap().test_value_binding_mut(id).unwrap().contract.allocation = changed_allocation;
            changed.store.generic.as_deref_mut().unwrap().test_value_binding_source_mut(guard.source).unwrap().expected.allocation = changed_allocation;
            assert!(FullVerifier::verify_generic_evidence(&changed.store).unwrap_err().message.contains("original receipt"));
        });
    }

    #[test]
    fn scalar_native_length_binding_keeps_original_receiver_authority() {
        execute_value_fixture("pure measured(text: Str) -> Int { let width = text.byte_len(); width }\nprint ${measured(\"three\")}\n", b"5\n", ValueBindingAllocation::Integer, "measured");
    }

    #[test]
    fn driver_stream_body_immutable_binding_keeps_original_allocation_after_frontend_disposal() {
        execute_value_fixture("pure selected(value: Int) -> Int { value + 1 }\nstream rows() -> Stream[Int] { yield 1; yield 2 }\nfor row in rows() { let kept = row + row; print ${selected(kept)} }\n", b"3\n5\n", ValueBindingAllocation::Integer, "selected");
    }

    #[test]
    fn guarded_match_initializer_keeps_original_arm_binding_after_frontend_disposal() {
        execute_value_fixture("pure selected(result: Result[Int, Str]) -> Int { guard let positive = result else { |_failure| return 0 }; let chosen = match positive { value if value > 0 => value, _ => 0 }; chosen }\nprint ${selected(Ok(7))}\nprint ${selected(Ok(-7))}\nprint ${selected(Err(\"bad\"))}\n", b"7\n0\n0\n", ValueBindingAllocation::Value, "selected");
    }

    #[test]
    fn immutable_list_bindings_keep_original_lexical_reads_after_frontend_disposal() {
        execute_value_fixture("pure selected(values: List[Int], other: List[Int]) -> Int { let kept = values; { let kept = other; let _ = kept[0] }; kept[0] }\nprint ${selected([7], [9])}\n", b"7\n", ValueBindingAllocation::Value, "selected");
    }

    #[test]
    fn immutable_map_bindings_keep_original_receiver_reads_after_frontend_disposal() {
        execute_value_fixture("pure selected(values: Map[Str, Int]) -> Int { let kept = values; kept.get(\"one\") ?? 0 }\nprint ${selected({[\"one\"]: 7})}\n", b"7\n", ValueBindingAllocation::Value, "selected");
    }

    #[test]
    fn immutable_collection_bindings_refuse_initializer_replacement_and_coforged_receipts() {
        let program = checked_value_fixture("pure selected(values: List[Int], other: List[Int]) -> Int { let first = values; let second = other; first[0] + second[0] }\nprint ${selected([7], [9])}\n");
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, first) = generic.value_bindings().next().unwrap();
            let (_, second) = generic.value_bindings().nth(1).unwrap();
            assert_eq!(program.store.semantic.to_type(first.contract.binding_type).unwrap(), Type::List(Box::new(Type::Int)));
            let mut changed = program.clone();
            let payload = changed.store.data[first.contract.instruction as usize].range();
            changed.store.extra[payload.start as usize + 1] = second.contract.initializer;
            assert!(FullVerifier::verify(&changed).is_err());
            changed.store.generic.as_deref_mut().unwrap().test_value_binding_mut(id).unwrap().contract.initializer = second.contract.initializer;
            changed.store.generic.as_deref_mut().unwrap().test_value_binding_source_mut(first.source).unwrap().expected.initializer = second.contract.initializer;
            assert!(FullVerifier::verify_generic_evidence(&changed.store).unwrap_err().message.contains("original receipt"));
        });
    }

}
