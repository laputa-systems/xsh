use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn original_fs_root_allocation(&self, id: ExprId) -> bool {
        self.original_native_static_plan(id).is_some_and(|(metadata, _, _)|
            metadata.owner == crate::sema::registry_graph::RegistryOwner::Module("fs")
                && metadata.operation == RuntimeOp::FsTempDir)
    }

    pub(super) fn original_fs_root_method(&self, id: ExprId) -> bool {
        let solved = self.solved();
        let Some(operation) = solved.operations.get(&self.expression_identity(id)) else { return false; };
        let Ok(Some(selected)) = solved.graph.candidate_evidence(operation.requirement) else { return false; };
        matches!(solved.operation_catalog.candidate(&solved.graph, selected.candidate),
            Ok(crate::sema::check::SolvedOperationAuthority::Registry(metadata))
                if metadata.owner == crate::sema::registry_graph::RegistryOwner::Method(MethodReceiver::FsRoot))
    }

    /// The checked registry selection supplies the operation and every host
    /// destination. Compiler slots save receiver and authored operands in order;
    /// absent destinations remain absent for the native defaults.
    pub(super) fn lower_original_fs_root_method(
        &mut self, id: ExprId, callee: ExprId, slots: &mut SlotScope,
        current_function: Option<Name>, item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        use crate::sema::inference::{OperationBinding, TypeNode};
        use crate::modules::signature::{ImplBinding, SemanticRule};
        let identity = self.expression_identity(id);
        let solved = self.solved();
        let operation = solved.operations.get(&identity)?.clone();
        let selected = solved.graph.candidate_evidence(operation.requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        let metadata = metadata.clone();
        if metadata.owner != crate::sema::registry_graph::RegistryOwner::Method(MethodReceiver::FsRoot)
            || metadata.binding != ImplBinding::Native || metadata.semantic_rule != SemanticRule::Standard
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || !operation.argument_coercions.is_empty() { return None; }
        let crate::sema::inference::RequirementTemplate::Operation { call, .. } = solved.graph.requirement_template(operation.requirement).ok()? else { return None; };
        if solved.graph.operation_call(call).ok()?.binding != OperationBinding::Slots { return None; }
        let signature = solved.graph.resolved(selected.signature).ok()?;
        let TypeNode::Arrow(arrow) = solved.graph.node(signature).ok()? else { return None; };
        let arrow = arrow.clone();
        if arrow.params.len() != metadata.parameters.len() + 1
            || arrow.params[0].label != Name::intern("<receiver>")
            || arrow.params[0].defaulted || arrow.params.iter().any(|parameter| parameter.rest)
            || self.solved_type(arrow.params[0].ty)? != Type::FsRoot
            || self.solved_type(operation.receiver?)? != Type::FsRoot { return None; }
        let sources = solved.argument_sources.get(&identity)?.clone();
        if sources.len() != operation.binding.supplied_slots.len()
            || arrow.params[1..].iter().zip(&metadata.parameters).any(|(formal, original)| formal.label != original.label || formal.defaulted != original.defaulted) { return None; }
        let (base, propagation) = match self.program.arena.expr(callee).kind {
            ArenaExprKind::Field { base, .. } => (base, false),
            ArenaExprKind::NullSafeField { base, .. } => (base, true),
            _ => return None,
        };
        let span = self.program.arena.expr(id).span;
        let source = self.expression_identity(base);
        let source_type = crate::sema::inference::ScopedRoot {
            ty: *self.solved().expressions.get(&source)?,
            scope: self.solved().expression_scope(source, operation.caller).ok()?,
        };
        let source_value_type = self.solved_type(source_type.ty)?;
        let result_propagation = propagation && matches!(&source_value_type,
            Type::Result(inner, error) if **inner == Type::FsRoot && **error == Type::Error);
        let optional_receiver = propagation && matches!(&source_value_type,
            Type::Optional(inner) if **inner == Type::FsRoot);
        if source_value_type != Type::FsRoot && !result_propagation && !optional_receiver { return None; }
        let receiver = if result_propagation || optional_receiver {
            self.lower_postfix_receiver(base, slots, current_function, item_slot)?
        } else { self.lower_expr(base, slots, current_function, item_slot)? };
        let receiver_slot = slots.reserve("filesystem root receiver");
        let read = push_build_row!(self, expr, BuildExprRow::Param(receiver_slot));
        if result_propagation {
            if self.solved().expression_owners.get(&source).copied() != operation.caller { return None; }
            self.solved().graph.validate_scoped(source_type).ok()?;
            let mut scratch = self.scratch.borrow_mut();
            if !matches!(scratch.expressions.get(receiver.index())?, BuildExprRow::Try(_)) { return None; }
            scratch.native_receiver_origins.insert(read, super::super::indexed::full::BuildSavedNativeReceiverOrigin {
                call: identity, origin: source, source_type, initializer: receiver, slot: receiver_slot, wrapper: None,
            });
            if scratch.native_receiver_initializers.insert((receiver, receiver_slot), read).is_some() { return None; }
        } else if optional_receiver {
            self.record_original_guarded_native_receiver(id, base, receiver, read, receiver_slot)?;
        } else { self.record_original_native_receiver(id, base, receiver, read, receiver_slot)?; }
        let lowered = self.lower_source_argument_values(sources.iter().map(|source| (source.entry_index, source.value, source.span)),
            slots, current_function, item_slot)?;
        self.record_original_argument_bindings(id, &sources, &lowered)?;
        let mut bindings = vec![(receiver, receiver_slot)];
        bindings.extend(lowered.bindings);
        let mut arguments = vec![None; arrow.params.len()];
        arguments[0] = Some(read);
        for ((source, value), &slot) in sources.iter().zip(lowered.values).zip(&operation.binding.supplied_slots) {
            let parameter = arrow.params.get(slot + 1)?;
            if source.name.is_some_and(|name| name != parameter.label)
                || matches!(source.value, crate::sema::arguments::ArgumentValueSource::PositionalSplice(_))
                || arguments.get(slot + 1)?.is_some() { return None; }
            let ty = self.solved_type(parameter.ty)?;
            arguments[slot + 1] = Some(self.checked_unsigned_value(value, &ty, source.span));
        }
        if arguments[1..].iter().enumerate().any(|(slot, argument)| argument.is_none() != operation.binding.default_slots.contains(&slot)) { return None; }
        let call = push_build_row!(self, expr, BuildExprRow::ModuleCall { cli_plan: None, op: metadata.operation, args: arguments, span });
        Some(self.wrap_argument_bindings(call, bindings, span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::syntax::parser::Parser;

    #[test]
    fn fs_root_chained_require_retains_original_selected_method_before_lowering() {
        let source = "proc close_erased(root: FsRoot) [fs, error] -> Result[Unit] { let erased: Any = root; return erased.require(FsRoot)?.close() }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("fs-root-chained-require.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let symbols = parsed.arena.symbol_owner().clone();
        let _symbols = symbols.enter();
        let origin = bodies.solved.expressions.keys().find(|origin| {
            let ArenaExprKind::Call { callee, .. } = parsed.arena.arena.expr(origin.expression).kind else { return false; };
            matches!(parsed.arena.arena.expr(callee).kind, ArenaExprKind::Field { name, .. } | ArenaExprKind::NullSafeField { name, .. } if name == "close")
        }).copied().expect("the original close expression has checked type evidence");
        let operation = bodies.solved.operations.get(&origin).unwrap_or_else(|| panic!("checked opaque close requires its original operation: invocation {:?}, callable {:?}", bodies.solved.invocations.get(&origin), bodies.solved.expression_callables.get(&origin)));
        let selected = bodies.solved.graph.candidate_evidence(operation.requirement).unwrap().expect("the original method has a selected candidate");
        let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = bodies.solved.operation_catalog.candidate(&bodies.solved.graph, selected.candidate).unwrap() else { panic!("opaque receiver method keeps registry authority"); };
        assert_eq!(metadata.owner, crate::sema::registry_graph::RegistryOwner::Method(MethodReceiver::FsRoot));
        assert_eq!(metadata.operation, RuntimeOp::FsCloseRoot);
        let mut lowered = false;
        lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources, StdlibLowerLinkage::Local, |unit| {
            let scratch = unit.body.as_ref().unwrap().scratch.borrow();
            assert_eq!(scratch.native_receiver_origins.len(), 1);
            lowered = true;
            Ok(())
        }).expect("checked opaque require and close chain lowers with its original receiver receipt");
        assert!(lowered);
    }
}
