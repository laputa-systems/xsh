use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_callable_binding(
        &self, statement: StmtId, initializer: ExprId, slot: usize,
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
        let is_callable = |ty| -> Option<bool> {
            let ty = solved.graph.resolved(ty).ok()?;
            Some(matches!(solved.graph.node(ty).ok()?,
                crate::sema::inference::TypeNode::Arrow(_) | crate::sema::inference::TypeNode::NativeCallable(_)))
        };
        let Some(binding) = solved.bindings.get(&binding_identity) else {
            // A typed initializer cannot stand in for a missing checked local
            // definition. Scalar and explicit erased bindings use other paths.
            return match solved.expressions.get(&initializer_source) {
                Some(actual) if is_callable(*actual)? => None,
                _ => Some(()),
            };
        };
        if !is_callable(binding.ty)? { return Some(()); }
        if binding.mutable || slots.resolve(name) != Some(slot) { return None; }
        let actual = *solved.expressions.get(&initializer_source)?;
        if !is_callable(actual)? { return None; }
        solved.expression_callables.get(&initializer_source)?;
        let lexical = match binding.owner {
            Some(owner) => Some(solved.declarations.get(&owner)?.scheme),
            None => None,
        };
        let source_type = crate::sema::inference::ScopedRoot { ty: binding.ty, scope: binding.scheme.or(lexical) };
        solved.graph.validate_scoped(source_type).ok()?;
        let initializer_scope = solved.expression_scope(initializer_source, binding.owner).ok()?;
        solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: actual, scope: initializer_scope }).ok()?;
        let origin = super::super::BuildCallableBindingOrigin {
            statement: source_statement, row, slot, initializer: value, initializer_source, source_type,
        };
        self.scratch.borrow_mut().callable_binding_origins.insert(binding_identity, origin);
        slots.callable_binding_authorities.insert(name, binding_identity);
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::{BindingIdentity, Checker};
    use crate::syntax::parser::Parser;

    #[test]
    fn original_local_callable_bindings_keep_initialization_rows_and_lexical_uses_after_source_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure increment(value: Int) -> Int { value + 1 }\npure decrement(value: Int) -> Int { value - 1 }\npure selected(value: Int) -> Int {\n let callback = increment\n let before = callback(value)\n {\n  let callback = decrement\n  let _ = callback(value)\n }\n {\n  let callback = 9\n  let _ = callback + 1\n }\n before + callback(value)\n}\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("local-callable-binding.xsh", source);
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
                    && matches!(parsed.arena.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "callback")
                    && matches!(parsed.arena.arena.expr(initializer).kind, ArenaExprKind::Ident(_))
                {
                    bindings.push((BindingIdentity { source: source_id, namespace: None, target },
                        StatementIdentity { source: source_id, namespace: None, statement },
                        ExpressionIdentity { source: source_id, namespace: None, expression: initializer }));
                }
            }
            bindings.sort_by_key(|(_, statement, _)| parsed.arena.arena.stmt(statement.statement).span.start());
            for index in 0..parsed.arena.stats().expressions {
                let expression = ExprId::from_index(index);
                if matches!(parsed.arena.arena.expr(expression).kind, ArenaExprKind::Ident(name) if name == "callback") {
                    uses.push(ExpressionIdentity { source: source_id, namespace: None, expression });
                }
            }
            uses.sort_by_key(|identity| parsed.arena.arena.expr(identity.expression).span.start());
            assert_eq!(bindings.len(), 2);
            assert_eq!(uses.len(), 4);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| { assert!(unit.is_lowered(), "{:?}", unit.blocker_detail); functions.push((unit.key, unit.body.unwrap())); Ok(()) }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let function = &functions.iter().find(|(key, _)| *key == LoweredFunctionKey::Name(Name::intern("selected"))).unwrap().1;
            let scratch = function.scratch.borrow();
            assert_eq!(scratch.callable_binding_origins.len(), 2, "each actual immutable callable let retains its original definition and emitted row");
            for (binding, statement, initializer_source) in &bindings {
                let origin = &scratch.callable_binding_origins[binding];
                assert_eq!(origin.statement, *statement);
                assert_eq!(origin.initializer_source, *initializer_source);
                assert_eq!(origin.source_type.ty, checked.solved.bindings[binding].ty);
                let binding_fact = &checked.solved.bindings[binding];
                let lexical = binding_fact.owner.map(|owner| checked.solved.declarations[&owner].scheme);
                assert_eq!(origin.source_type.scope, binding_fact.scheme.or(lexical));
                checked.solved.graph.validate_scoped(origin.source_type).unwrap();
                assert!(matches!(scratch.statements[origin.row.index()], BuildStmtRow::Let { slot, value } if slot == origin.slot && value == origin.initializer));
            }
            assert_ne!(scratch.callable_binding_origins[&bindings[0].0].slot, scratch.callable_binding_origins[&bindings[1].0].slot);
            for (index, expected) in [(0, Some(bindings[0].0)), (1, Some(bindings[1].0)), (2, None), (3, Some(bindings[0].0))] {
                assert_eq!(scratch.callable_binding_uses.get(&uses[index]).copied(), expected, "shadowing restores the exact outer callable while an ordinary scalar read has no callable authority");
                if let Some(binding) = expected {
                    let slot = scratch.callable_binding_origins[&binding].slot;
                    assert!(function.expression_origins.iter().any(|(row, identity)| *identity == uses[index]
                        && matches!(scratch.expressions[row.index()], BuildExprRow::Param(actual) if actual == slot)));
                }
            }
            assert_eq!(scratch.callable_binding_uses.len(), 3);
        });
    }

    #[test]
    fn original_local_callable_binding_refuses_missing_definition_or_initializer_facts() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure increment(value: Int) -> Int { value + 1 }\npure selected(value: Int) -> Int { let callback = increment; callback(value) }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("missing-local-callable-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let selected = Name::intern("selected");
            let binding = bodies.solved.bindings.iter().find(|(identity, binding)| binding.owner.is_some()
                && matches!(parsed.arena.arena.binding_target(identity.target).kind, ArenaBindingTargetKind::Name(name) if name == "callback")).map(|(identity, _)| *identity).unwrap();
            let initializer = (0..parsed.arena.stats().statements).find_map(|index| {
                if let ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(expression), .. } = parsed.arena.arena.stmt(StmtId::from_index(index)).kind
                    && target == binding.target
                { Some(ExpressionIdentity { source: source_id, namespace: None, expression }) } else { None }
            }).unwrap();
            let lower_selected = |bodies: &CompactBodyProbeOutput, declarations: &CompactDeclOutput| {
                let mut result = None;
                lower_compact_function_units_into(&parsed.arena, declarations, bodies, source, &sources,
                    StdlibLowerLinkage::Local, |unit| {
                        if unit.key == LoweredFunctionKey::Name(selected) { result = Some(unit.is_lowered()); }
                        Ok(())
                    }).unwrap();
                result.unwrap()
            };
            assert!(lower_selected(&bodies, &declarations), "the unchanged checked source lowers before its original proof is removed");
            drop(checked);
            declarations.solved = Default::default();
            let definition = Arc::get_mut(&mut bodies.solved).unwrap().bindings.remove(&binding).unwrap();
            assert!(!lower_selected(&bodies, &declarations), "a typed initializer cannot replace its missing original binding proof");
            Arc::get_mut(&mut bodies.solved).unwrap().bindings.insert(binding, definition);
            let actual = Arc::get_mut(&mut bodies.solved).unwrap().expressions.remove(&initializer).unwrap();
            assert!(!lower_selected(&bodies, &declarations), "principal callable metadata cannot replace the missing original initializer type");
            Arc::get_mut(&mut bodies.solved).unwrap().expressions.insert(initializer, actual);
            assert!(lower_selected(&bodies, &declarations));
            drop(parsed);
            bodies.solved.validate().unwrap();
        });
    }
}
