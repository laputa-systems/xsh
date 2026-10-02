use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    fn checked_guard_error_binding_roots(&self, statement: StmtId) -> Option<(crate::sema::check::GuardErrorBindingIdentity, ExpressionIdentity, crate::sema::inference::ScopedRoot, crate::sema::inference::ScopedRoot)> {
        let ArenaStmtKind::Guard { initializer: ArenaExprOrRun::Expr(initializer), else_block, .. } = self.program.arena.stmt(statement).kind else { return None; };
        let param = self.program.arena.block_params(self.program.arena.block(else_block).params).first()?;
        let identity = crate::sema::check::GuardErrorBindingIdentity { statement: self.statement_identity(statement) };
        let initializer = self.expression_identity(initializer);
        let solved = self.solved();
        let original = solved.guard_error_bindings.get(&identity)?;
        if param.name.as_str() == "_" || original.block != else_block || original.name != param.name
            || original.initializer != initializer || solved.expressions.get(&initializer) != Some(&original.initializer_type)
            || solved.expression_owners.get(&initializer).copied() != original.owner { return None; }
        let scope = match original.owner { Some(owner) => Some(solved.declarations.get(&owner)?.scheme), None => None };
        let source_type = crate::sema::inference::ScopedRoot { ty: original.binding_type, scope };
        let initializer_type = crate::sema::inference::ScopedRoot { ty: original.initializer_type, scope: solved.expression_scope(initializer, original.owner).ok()? };
        solved.graph.validate_scoped(source_type).ok()?;
        solved.graph.validate_scoped(initializer_type).ok()?;
        let crate::sema::inference::TypeNode::Result(_, error) = solved.graph.node(solved.graph.resolved(initializer_type.ty).ok()?).ok()? else { return None; };
        if solved.graph.resolved(*error).ok()? != solved.graph.resolved(source_type.ty).ok()? { return None; }
        Some((identity, initializer, source_type, initializer_type))
    }

    pub(super) fn install_original_guard_error_binding(&self, statement: StmtId, error_slot: Option<usize>, slots: &mut SlotScope) -> Option<()> {
        let Some(slot) = error_slot else { return Some(()); };
        let ArenaStmtKind::Guard { target, initializer: ArenaExprOrRun::Expr(_), else_block, .. } = self.program.arena.stmt(statement).kind else { return Some(()); };
        if !matches!(self.program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if !is_discard_name(name)) { return Some(()); }
        let (identity, _, source_type, initializer_type) = self.checked_guard_error_binding_roots(statement)?;
        if super::super::indexed::generic::graph_ground_type(&self.solved().graph, source_type.ty).is_err()
            || super::super::indexed::generic::graph_ground_type(&self.solved().graph, initializer_type.ty).is_err() { return Some(()); }
        let param = self.program.arena.block_params(self.program.arena.block(else_block).params).first()?;
        if slots.resolve(param.name) != Some(slot) { return None; }
        slots.guard_error_binding_authorities.insert(param.name, identity);
        Some(())
    }

    pub(super) fn record_original_guard_error_binding(&self, statement: StmtId, value: BuildExprId, row: BuildStmtId, error_slot: Option<usize>) -> Option<()> {
        let Some(slot) = error_slot else { return Some(()); };
        let ArenaStmtKind::Guard { target, initializer: ArenaExprOrRun::Expr(_), .. } = self.program.arena.stmt(statement).kind else { return Some(()); };
        if !matches!(self.program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if !is_discard_name(name)) { return Some(()); }
        let (identity, initializer_source, source_type, initializer_type) = self.checked_guard_error_binding_roots(statement)?;
        if super::super::indexed::generic::graph_ground_type(&self.solved().graph, source_type.ty).is_err()
            || super::super::indexed::generic::graph_ground_type(&self.solved().graph, initializer_type.ty).is_err() { return Some(()); }
        let material = self.original_source_instruction(value)?;
        if self.expression_origins.get(&material) != Some(&initializer_source) { return None; }
        let mut scratch = self.scratch.borrow_mut();
        if !matches!(scratch.statements.get(row.index()), Some(BuildStmtRow::Guard { target: LoweredCompTarget::Slot(_), value: actual, else_param_slot: Some(actual_slot), .. }) if *actual == value && *actual_slot == slot) { return None; }
        scratch.guard_error_binding_origins.insert(identity, super::super::BuildValueBindingOrigin {
            statement: identity.statement, row, slot, initializer: value, initializer_source, source_type, initializer_type,
        });
        Some(())
    }

    fn checked_with_binding_roots(&self, statement: StmtId, ordinal: u32) -> Option<(crate::sema::check::WithBindingIdentity, ExpressionIdentity, crate::sema::inference::ScopedRoot, crate::sema::inference::ScopedRoot)> {
        let ArenaStmtKind::With { bindings, .. } = self.program.arena.stmt(statement).kind else { return None; };
        let binding = self.program.arena.with_bindings(bindings).get(ordinal as usize)?;
        let identity = crate::sema::check::WithBindingIdentity { statement: self.statement_identity(statement), ordinal };
        let initializer = self.expression_identity(binding.initializer);
        let solved = self.solved();
        let original = solved.with_bindings.get(&identity)?;
        let lexical = match original.owner { Some(owner) => Some(solved.declarations.get(&owner)?.scheme), None => None };
        if original.initializer != initializer || solved.expressions.get(&initializer) != Some(&original.initializer_type)
            || solved.expression_owners.get(&initializer).copied() != original.owner { return None; }
        let source_type = crate::sema::inference::ScopedRoot { ty: original.binding_type, scope: lexical };
        let initializer_type = crate::sema::inference::ScopedRoot { ty: original.initializer_type, scope: solved.expression_scope(initializer, original.owner).ok()? };
        solved.graph.validate_scoped(source_type).ok()?;
        solved.graph.validate_scoped(initializer_type).ok()?;
        Some((identity, initializer, source_type, initializer_type))
    }

    pub(super) fn install_original_with_binding(&mut self, statement: StmtId, ordinal: u32, slot: usize, value: BuildExprId, slots: &mut SlotScope) -> Option<()> {
        let (identity, initializer, source_type, initializer_type) = self.checked_with_binding_roots(statement, ordinal)?;
        let solved = self.solved();
        if super::super::indexed::generic::graph_ground_type(&solved.graph, source_type.ty).is_err()
            || super::super::indexed::generic::graph_ground_type(&solved.graph, initializer_type.ty).is_err() { return Some(()); }
        if matches!(storage::checked_storage_view(&solved.graph, Some(source_type)).ok()?.kind, LoweredType::Pure | LoweredType::Proc) { return Some(()); }
        let material = self.original_source_instruction(value)?;
        if self.expression_origins.get(&material) != Some(&initializer) { return None; }
        let ArenaStmtKind::With { bindings, .. } = self.program.arena.stmt(statement).kind else { return None; };
        let binding = self.program.arena.with_bindings(bindings).get(ordinal as usize)?;
        if binding.name.as_str() != "_" {
            if slots.resolve(binding.name) != Some(slot) { return None; }
            slots.with_value_binding_authorities.insert(binding.name, identity);
        }
        Some(())
    }

    pub(super) fn record_original_with_bindings(&mut self, statement: StmtId, row: BuildStmtId) -> Option<()> {
        let ArenaStmtKind::With { bindings, .. } = self.program.arena.stmt(statement).kind else { return None; };
        let lowered = {
            let scratch = self.scratch.borrow();
            let BuildStmtRow::With { bindings, .. } = scratch.statements.get(row.index())? else { return None; };
            bindings.clone()
        };
        if lowered.len() != self.program.arena.with_bindings(bindings).len() { return None; }
        for (ordinal, (slot, value)) in lowered.into_iter().enumerate() {
            let (identity, initializer_source, source_type, initializer_type) = self.checked_with_binding_roots(statement, ordinal as u32)?;
            let solved = self.solved();
            if super::super::indexed::generic::graph_ground_type(&solved.graph, source_type.ty).is_err()
                || super::super::indexed::generic::graph_ground_type(&solved.graph, initializer_type.ty).is_err() { continue; }
            if matches!(storage::checked_storage_view(&solved.graph, Some(source_type)).ok()?.kind, LoweredType::Pure | LoweredType::Proc) { continue; }
            let material = self.original_source_instruction(value)?;
            if self.expression_origins.get(&material) != Some(&initializer_source) { return None; }
            self.scratch.borrow_mut().with_value_binding_origins.insert(identity, super::super::BuildValueBindingOrigin {
                statement: identity.statement, row, slot, initializer: value, initializer_source, source_type, initializer_type,
            });
        }
        Some(())
    }

    pub(super) fn record_original_value_binding(
        &mut self, statement: StmtId, initializer: ExprId, slot: usize,
        value: BuildExprId, row: BuildStmtId, slots: &mut SlotScope,
    ) -> Option<()> {
        let (target, ty, original) = match self.program.arena.stmt(statement).kind {
            ArenaStmtKind::Let { target, ty, initializer: ArenaExprOrRun::Expr(original), .. }
            | ArenaStmtKind::Guard { target, ty, initializer: ArenaExprOrRun::Expr(original), .. } => (target, ty, original),
            _ => return Some(()),
        };
        if original != initializer { return None; }
        let ArenaBindingTargetKind::Name(name) = self.program.arena.binding_target(target).kind else { return Some(()); };
        let source_statement = self.statement_identity(statement);
        let binding_identity = crate::sema::check::BindingIdentity {
            source: source_statement.source, namespace: source_statement.namespace, target,
        };
        let initializer_source = self.expression_identity(initializer);
        let solved = self.solved();
        let binding = solved.bindings.get(&binding_identity)?;
        let lexical = match binding.owner {
            Some(owner) => Some(solved.declarations.get(&owner)?.scheme),
            None => None,
        };
        let source_type = crate::sema::inference::ScopedRoot { ty: binding.ty, scope: binding.scheme.or(lexical) };
        solved.graph.validate_scoped(source_type).ok()?;
        // Closed data is the supported transport boundary. The bounded adapter
        // only tests that boundary; its tree view never supplies an authority.
        if super::super::indexed::generic::graph_ground_type(&solved.graph, binding.ty).is_err() { return Some(()); }
        let kind = storage::checked_storage_view(&solved.graph, Some(source_type)).ok()?.kind;
        if matches!(kind, LoweredType::Pure | LoweredType::Proc) { return Some(()); }
        if binding.mutable || slots.resolve(name) != Some(slot)
            || solved.expression_owners.get(&initializer_source).copied() != binding.owner { return None; }
        let actual = *solved.expressions.get(&initializer_source)?;
        let initializer_type = crate::sema::inference::ScopedRoot {
            ty: actual, scope: solved.expression_scope(initializer_source, binding.owner).ok()?,
        };
        solved.graph.validate_scoped(initializer_type).ok()?;
        if super::super::indexed::generic::graph_ground_type(&solved.graph, actual).is_err() { return Some(()); }
        let material = {
            let scratch = self.scratch.borrow();
            match scratch.statements.get(row.index())? {
                BuildStmtRow::Let { slot: actual_slot, value: actual_value } if *actual_slot == slot && *actual_value == value => {}
                BuildStmtRow::LetInt { slot: actual_slot, value: actual_value } if *actual_slot == slot => {
                    if scratch.int_expression_origins.get(actual_value) != Some(&initializer_source) { return None; }
                }
                BuildStmtRow::LetBool { slot: actual_slot, value: actual_value } if *actual_slot == slot => {
                    if scratch.bool_expression_origins.get(actual_value) != Some(&initializer_source) { return None; }
                }
                BuildStmtRow::Guard { target: LoweredCompTarget::Slot(actual_slot), value: actual_value, .. } if *actual_slot == slot && *actual_value == value => {}
                _ => return None,
            }
            let mut material = value;
            loop {
                match scratch.expressions.get(material.index())? {
                    BuildExprRow::CheckedValue { value: checked, .. } => {
                        if checked.index() >= material.index() { return None; }
                        material = *checked;
                    }
                    BuildExprRow::Try(required) if ty.is_some() && !self.expression_origins.contains_key(&material) => {
                        let BuildExprRow::Require { value: checked, .. } = scratch.expressions.get(required.index())? else { break; };
                        if required.index() >= material.index() || checked.index() >= required.index() { return None; }
                        material = *checked;
                    }
                    _ => break,
                }
            }
            material
        };
        if let Some(original) = self.expression_origins.get(&material) {
            if *original != initializer_source { return None; }
        } else {
            let executed = self.original_source_instruction(material)?;
            if executed != material {
                if self.expression_origins.get(&executed) != Some(&initializer_source) { return None; }
            } else { self.expression_origins.insert(material, initializer_source); }
        }
        let mut scratch = self.scratch.borrow_mut();
        match scratch.statements.get(row.index())? {
            BuildStmtRow::Let { slot: actual_slot, value: actual_value } if *actual_slot == slot && *actual_value == value => {}
            BuildStmtRow::LetInt { slot: actual_slot, value: actual_value } if *actual_slot == slot
                && scratch.int_expression_origins.get(actual_value) == Some(&initializer_source) => {}
            BuildStmtRow::LetBool { slot: actual_slot, value: actual_value } if *actual_slot == slot
                && scratch.bool_expression_origins.get(actual_value) == Some(&initializer_source) => {}
            BuildStmtRow::Guard { target: LoweredCompTarget::Slot(actual_slot), value: actual_value, .. } if *actual_slot == slot && *actual_value == value => {}
            _ => return None,
        }
        scratch.expressions.get(value.index())?;
        scratch.value_binding_origins.insert(binding_identity, super::super::BuildValueBindingOrigin {
            statement: source_statement, row, slot, initializer: value, initializer_source, source_type, initializer_type,
        });
        slots.value_binding_authorities.insert(name, binding_identity);
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::{BindingIdentity, Checker};
    use crate::syntax::parser::Parser;

    #[test]
    fn original_guard_failure_binding_keeps_handler_and_result_error_roots() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc selected(result: Result[Int]) -> Int { guard let kept = result else { |failure| print $failure.message; return 0 }; kept }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-guard-failure-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.guard_error_bindings.len(), 1);
            let (&identity, definition) = checked.solved.guard_error_bindings.iter().next().unwrap();
            assert_eq!(definition.name, Name::intern("failure"));
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push(unit.body.unwrap()); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let scratch = functions[0].scratch.borrow();
            let original = &scratch.guard_error_binding_origins[&identity];
            assert_eq!(original.statement, identity.statement);
            assert_eq!(original.initializer_source, definition.initializer);
            assert_eq!(original.source_type.ty, definition.binding_type);
            assert_eq!(original.initializer_type.ty, definition.initializer_type);
            assert_eq!(super::super::super::indexed::generic::graph_ground_type(&checked.solved.graph, original.source_type.ty).unwrap(), Type::Error);
            assert_eq!(super::super::super::indexed::generic::graph_ground_type(&checked.solved.graph, original.initializer_type.ty).unwrap(), Type::Result(Box::new(Type::Int), Box::new(Type::Error)));
            assert!(matches!(scratch.statements[original.row.index()], BuildStmtRow::Guard { value, else_param_slot: Some(slot), .. } if value == original.initializer && slot == original.slot));
            assert!(scratch.guard_error_binding_uses.values().any(|actual| *actual == identity));
        });
    }

    #[test]
    fn original_guard_failure_binding_refuses_missing_checked_handler_root() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(result: Result[Int], outer: Error) -> Error { guard let kept = result else { |failure| return failure }; let _ = kept; outer }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("missing-original-guard-failure-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let mut declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let lower_selected = |bodies: &CompactBodyProbeOutput, declarations: &CompactDeclOutput| {
                let mut lowered = None;
                lower_compact_function_units_into(&parsed.arena, declarations, bodies, source, &sources,
                    StdlibLowerLinkage::Local, |unit| { lowered = Some(unit.is_lowered()); Ok(()) }).unwrap();
                lowered.unwrap()
            };
            assert!(lower_selected(&bodies, &declarations));
            drop(checked);
            declarations.solved = Default::default();
            let (&identity, _) = bodies.solved.guard_error_bindings.iter().next().unwrap();
            let original = Arc::get_mut(&mut bodies.solved).unwrap().guard_error_bindings.remove(&identity).unwrap();
            assert!(!lower_selected(&bodies, &declarations), "a physical error slot cannot replace its checked handler definition");
            Arc::get_mut(&mut bodies.solved).unwrap().guard_error_bindings.insert(identity, original);
            assert!(lower_selected(&bodies, &declarations));
        });
    }

    #[test]
    fn original_with_success_bindings_keep_checked_ordinals_and_separate_result_roots() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(result: Result[Int, Str]) -> Int { with first = result, second = first + first { return second } else { |_failure| return 0 } }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-with-success-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.with_bindings.len(), 2);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push(unit.body.unwrap()); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let scratch = functions[0].scratch.borrow();
            assert_eq!(scratch.with_value_binding_origins.len(), 2);
            for (&identity, original) in &scratch.with_value_binding_origins {
                let definition = &checked.solved.with_bindings[&identity];
                let BuildStmtRow::With { ref bindings, .. } = scratch.statements[original.row.index()] else { panic!("with success slot must retain its allocation"); };
                assert_eq!(bindings[identity.ordinal as usize], (original.slot, original.initializer));
                assert_eq!(original.statement, identity.statement);
                assert_eq!(original.initializer_source, definition.initializer);
                assert_eq!(original.source_type.ty, definition.binding_type);
                assert_eq!(original.initializer_type.ty, definition.initializer_type);
                assert_eq!(super::super::super::indexed::generic::graph_ground_type(&checked.solved.graph, original.source_type.ty).unwrap(), Type::Int);
                let expected = if identity.ordinal == 0 { Type::Result(Box::new(Type::Int), Box::new(Type::Str)) } else { Type::Int };
                assert_eq!(super::super::super::indexed::generic::graph_ground_type(&checked.solved.graph, original.initializer_type.ty).unwrap(), expected);
                assert!(scratch.with_value_binding_uses.values().any(|actual| *actual == identity));
            }
        });
    }

    #[test]
    fn original_with_success_binding_refuses_missing_checked_ordinal() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(result: Result[Int, Str]) -> Int { with first = result { return first } else { |_failure| return 0 } }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("missing-original-with-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let mut declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let lower_selected = |bodies: &CompactBodyProbeOutput, declarations: &CompactDeclOutput| {
                let mut lowered = None;
                lower_compact_function_units_into(&parsed.arena, declarations, bodies, source, &sources,
                    StdlibLowerLinkage::Local, |unit| { lowered = Some(unit.is_lowered()); Ok(()) }).unwrap();
                lowered.unwrap()
            };
            assert!(lower_selected(&bodies, &declarations));
            drop(checked);
            declarations.solved = Default::default();
            let (&identity, _) = bodies.solved.with_bindings.iter().next().unwrap();
            let original = Arc::get_mut(&mut bodies.solved).unwrap().with_bindings.remove(&identity).unwrap();
            assert!(!lower_selected(&bodies, &declarations), "a lowered success slot cannot invent its authored ordinal");
            Arc::get_mut(&mut bodies.solved).unwrap().with_bindings.insert(identity, original);
            assert!(lower_selected(&bodies, &declarations));
        });
    }

    #[test]
    fn original_guard_success_binding_keeps_its_checked_result_and_success_roots() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(result: Result[Int, Str], fallback: Int) -> Int { let kept = fallback - fallback; { guard let kept = result else { |_failure| return kept }; kept } }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-guard-success-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let (binding, initializer) = (0..parsed.arena.stats().statements).find_map(|index| {
                if let ArenaStmtKind::Guard { target, initializer: ArenaExprOrRun::Expr(initializer), .. } = parsed.arena.arena.stmt(StmtId::from_index(index)).kind {
                    Some((BindingIdentity { source: source_id, namespace: None, target },
                        ExpressionIdentity { source: source_id, namespace: None, expression: initializer }))
                } else { None }
            }).unwrap();
            let definition = checked.solved.bindings.get(&binding).unwrap();
            assert!(!definition.mutable);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push(unit.body.unwrap()); Ok(())
                }).unwrap();
            drop(parsed);
            let scratch = functions[0].scratch.borrow();
            let original = scratch.value_binding_origins.get(&binding).unwrap();
            let BuildStmtRow::Guard { target: LoweredCompTarget::Slot(slot), value, .. } = scratch.statements[original.row.index()] else { panic!("success binding must retain its Guard allocation"); };
            assert_eq!(slot, original.slot);
            assert_eq!(value, original.initializer);
            assert_eq!(original.initializer_source, initializer);
            assert_eq!(original.source_type.ty, definition.ty);
            assert_eq!(original.initializer_type.ty, checked.solved.expressions[&initializer]);
            assert_eq!(super::super::super::indexed::generic::graph_ground_type(&checked.solved.graph, original.source_type.ty).unwrap(), Type::Int);
            assert_eq!(super::super::super::indexed::generic::graph_ground_type(&checked.solved.graph, original.initializer_type.ty).unwrap(), Type::Result(Box::new(Type::Int), Box::new(Type::Str)));
            assert!(scratch.value_statement_reads.values().any(|(_, actual)| *actual == binding));
        });
    }

    #[test]
    fn original_scalar_integer_binding_keeps_its_physical_initializer_source() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value: Int) -> Int { let scalar = value + value; scalar }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-scalar-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push(unit.body.unwrap()); Ok(())
                }).unwrap();
            drop(parsed);
            let scratch = functions[0].scratch.borrow();
            assert_eq!(scratch.value_binding_origins.len(), 1);
            let (&binding, original) = scratch.value_binding_origins.iter().next().unwrap();
            let BuildStmtRow::LetInt { slot, value } = scratch.statements[original.row.index()] else { panic!("integer declaration must retain its specialized row"); };
            assert_eq!(slot, original.slot);
            assert_eq!(scratch.int_expression_origins.get(&value), Some(&original.initializer_source));
            assert_eq!(checked.solved.bindings[&binding].ty, original.source_type.ty);
            assert_eq!(checked.solved.expressions[&original.initializer_source], original.initializer_type.ty);
            assert_eq!(scratch.value_statement_reads.len(), 1);
        });
    }

    #[test]
    fn original_inert_list_value_binding_retains_its_material_initializer_source() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure kept() -> Int { let rows: List[Int] = [7]; rows.len() }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-inert-list-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push(unit.body.unwrap()); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let function = &functions[0];
            let scratch = function.scratch.borrow();
            let binding = scratch.value_binding_origins.values().next().unwrap();
            assert_eq!(scratch.value_binding_origins.len(), 1);
            assert_eq!(storage::checked_storage_view(&checked.solved.graph, Some(binding.initializer_type)).unwrap().kind, LoweredType::List);
            let mut material = binding.initializer;
            loop {
                material = match &scratch.expressions[material.index()] {
                    BuildExprRow::CheckedValue { value, .. } => *value,
                    BuildExprRow::Try(required) => {
                        let BuildExprRow::Require { value, .. } = scratch.expressions[required.index()] else { panic!("annotation propagation must validate its original initializer"); };
                        assert!(!function.expression_origins.contains_key(&material));
                        assert!(!function.expression_origins.contains_key(required));
                        value
                    }
                    _ => break,
                };
            }
            assert_eq!(function.expression_origins.get(&material), Some(&binding.initializer_source));
            assert_eq!(checked.solved.expressions[&binding.initializer_source], binding.initializer_type.ty);
        });
    }

    #[test]
    fn original_nested_native_result_value_binding_keeps_its_initialization_after_source_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc selected(ctx: TestContext) [error] -> Result[Str] {\n {\n  let invalid = test.run_script(ctx, \"print original\")?\n  invalid.stdout\n }\n}\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-native-result-value-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let (binding, statement, initializer) = (0..parsed.arena.stats().statements).find_map(|index| {
                let statement = StmtId::from_index(index);
                if let ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(initializer), .. } = parsed.arena.arena.stmt(statement).kind
                    && matches!(parsed.arena.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "invalid")
                {
                    Some((BindingIdentity { source: source_id, namespace: None, target },
                        StatementIdentity { source: source_id, namespace: None, statement },
                        ExpressionIdentity { source: source_id, namespace: None, expression: initializer }))
                } else { None }
            }).unwrap();
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push((unit.key, unit.body.unwrap())); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let function = &functions.iter().find(|(key, _)| *key == LoweredFunctionKey::Name(Name::intern("selected"))).unwrap().1;
            let scratch = function.scratch.borrow();
            assert_eq!(scratch.value_binding_origins.len(), 1, "a nested native result has its own immutable Let authority");
            let origin = &scratch.value_binding_origins[&binding];
            assert_eq!(origin.statement, statement);
            assert_eq!(origin.initializer_source, initializer);
            assert_eq!(origin.source_type.ty, checked.solved.bindings[&binding].ty);
            assert_eq!(origin.initializer_type.ty, checked.solved.expressions[&initializer]);
            let owner = checked.solved.bindings[&binding].owner;
            assert_eq!(origin.initializer_type.scope, checked.solved.expression_scope(initializer, owner).unwrap());
            checked.solved.graph.validate_scoped(origin.source_type).unwrap();
            checked.solved.graph.validate_scoped(origin.initializer_type).unwrap();
            assert_eq!(storage::checked_storage_view(&checked.solved.graph, Some(origin.source_type)).unwrap().kind, LoweredType::Record);
            assert!(matches!(scratch.statements[origin.row.index()], BuildStmtRow::Let { slot, value }
                if slot == origin.slot && value == origin.initializer));
            assert_eq!(scratch.value_binding_uses.len(), 1);
            let (&read, &actual_binding) = scratch.value_binding_uses.iter().next().unwrap();
            assert_eq!(actual_binding, binding);
            assert!(function.expression_origins.iter().any(|(instruction, original)| *original == read
                && matches!(scratch.expressions[instruction.index()], BuildExprRow::Param(slot) if slot == origin.slot)));
        });
    }

    #[test]
    fn original_value_bindings_restore_record_authority_after_mutable_and_callable_shadows() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure increment(value: Int) -> Int { value + 1 }\npure selected(value: Str) -> Str {\n let record = {label: value}\n let _ = record.label\n { let record = {label: \"inner\"}; let _ = record.label }\n { var record = {label: \"mutable\"}; let _ = record.label }\n { let record = increment; let _ = record(1) }\n record.label\n}\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-value-binding-shadows.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut bindings = Vec::new();
            let mut uses = Vec::new();
            for index in 0..parsed.arena.stats().statements {
                let statement = StmtId::from_index(index);
                if let ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(initializer), .. } = parsed.arena.arena.stmt(statement).kind
                    && matches!(parsed.arena.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "record")
                    && matches!(parsed.arena.arena.expr(initializer).kind, ArenaExprKind::Record(_))
                {
                    bindings.push((BindingIdentity { source: source_id, namespace: None, target }, statement));
                }
            }
            bindings.sort_by_key(|(_, statement)| parsed.arena.arena.stmt(*statement).span.start());
            for index in 0..parsed.arena.stats().expressions {
                let expression = ExprId::from_index(index);
                if matches!(parsed.arena.arena.expr(expression).kind, ArenaExprKind::Ident(name) if name == "record") {
                    uses.push(ExpressionIdentity { source: source_id, namespace: None, expression });
                }
            }
            uses.sort_by_key(|identity| parsed.arena.arena.expr(identity.expression).span.start());
            assert_eq!(bindings.len(), 2);
            assert_eq!(uses.len(), 5);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push((unit.key, unit.body.unwrap())); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let function = &functions.iter().find(|(key, _)| *key == LoweredFunctionKey::Name(Name::intern("selected"))).unwrap().1;
            let scratch = function.scratch.borrow();
            assert_eq!(scratch.value_binding_origins.len(), 2);
            assert_ne!(scratch.value_binding_origins[&bindings[0].0].slot, scratch.value_binding_origins[&bindings[1].0].slot);
            for (index, expected) in [(0, Some(bindings[0].0)), (1, Some(bindings[1].0)), (2, None), (3, None), (4, Some(bindings[0].0))] {
                assert_eq!(scratch.value_binding_uses.get(&uses[index]).copied(), expected);
                if let Some(binding) = expected {
                    let origin = &scratch.value_binding_origins[&binding];
                    assert!(function.expression_origins.iter().any(|(instruction, source)| *source == uses[index]
                        && matches!(scratch.expressions[instruction.index()], BuildExprRow::Param(slot) if slot == origin.slot)));
                }
            }
            assert_eq!(scratch.value_binding_uses.len(), 3);
            assert_eq!(scratch.callable_binding_origins.len(), 1, "typed callable creation stays in its separate authority domain");
        });
    }

    #[test]
    fn original_value_binding_refuses_missing_definition_or_initializer_facts() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value: Str) -> Str { let record = {label: value}; record.label }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("missing-original-value-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let (binding, initializer) = (0..parsed.arena.stats().statements).find_map(|index| {
                if let ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(expression), .. } = parsed.arena.arena.stmt(StmtId::from_index(index)).kind
                    && matches!(parsed.arena.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "record")
                {
                    Some((BindingIdentity { source: source_id, namespace: None, target },
                        ExpressionIdentity { source: source_id, namespace: None, expression }))
                } else { None }
            }).unwrap();
            let selected = Name::intern("selected");
            let lower_selected = |bodies: &CompactBodyProbeOutput, declarations: &CompactDeclOutput| {
                let mut result = None;
                lower_compact_function_units_into(&parsed.arena, declarations, bodies, source, &sources,
                    StdlibLowerLinkage::Local, |unit| {
                        if unit.key == LoweredFunctionKey::Name(selected) { result = Some(unit.is_lowered()); }
                        Ok(())
                    }).unwrap();
                result.unwrap()
            };
            assert!(lower_selected(&bodies, &declarations));
            drop(checked);
            declarations.solved = Default::default();
            let definition = Arc::get_mut(&mut bodies.solved).unwrap().bindings.remove(&binding).unwrap();
            assert!(!lower_selected(&bodies, &declarations), "a tree mirror cannot replace an original immutable definition");
            Arc::get_mut(&mut bodies.solved).unwrap().bindings.insert(binding, definition);
            let actual = Arc::get_mut(&mut bodies.solved).unwrap().expressions.remove(&initializer).unwrap();
            assert!(!lower_selected(&bodies, &declarations), "a checked binding cannot invent its missing initializer source root");
            Arc::get_mut(&mut bodies.solved).unwrap().expressions.insert(initializer, actual);
            let owner = Arc::get_mut(&mut bodies.solved).unwrap().expression_owners.remove(&initializer).unwrap();
            assert!(!lower_selected(&bodies, &declarations), "a closed initializer layout cannot replace its original lexical owner");
            Arc::get_mut(&mut bodies.solved).unwrap().expression_owners.insert(initializer, owner);
            assert!(lower_selected(&bodies, &declarations));
            drop(parsed);
            bodies.solved.validate().unwrap();
        });
    }
}
