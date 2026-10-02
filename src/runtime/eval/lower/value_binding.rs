use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_value_binding(
        &mut self, statement: StmtId, initializer: ExprId, slot: usize,
        value: BuildExprId, row: BuildStmtId, slots: &mut SlotScope,
    ) -> Option<()> {
        let ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(original), .. } = self.program.arena.stmt(statement).kind else { return Some(()); };
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
            if !matches!(scratch.statements.get(row.index())?, BuildStmtRow::Let { slot: actual_slot, value: actual_value }
                if *actual_slot == slot && *actual_value == value) { return None; }
            let mut material = value;
            while let BuildExprRow::CheckedValue { value: checked, .. } = scratch.expressions.get(material.index())? {
                if checked.index() >= material.index() { return None; }
                material = *checked;
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
        if !matches!(scratch.statements.get(row.index())?, BuildStmtRow::Let { slot: actual_slot, value: actual_value }
            if *actual_slot == slot && *actual_value == value) { return None; }
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
            assert_eq!(function.expression_origins.get(&binding.initializer), Some(&binding.initializer_source));
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
