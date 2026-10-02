use super::*;
use crate::sema::check::{BindingIdentity, DeclarationIdentity, ProducerFlowKind, ProducerFlowSource};
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildLexicalCaptureRead {
    pub binding: BindingIdentity,
    pub caller: DeclarationIdentity,
    pub origin: ExpressionIdentity,
    pub slot: usize,
    pub expression: BuildExprId,
    pub source_type: ScopedRoot,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_lexical_capture_read(&self, id: ExprId, lowered: BuildExprId, slots: &SlotScope) -> Option<()> {
        let ArenaExprKind::Ident(name) = self.program.arena.expr(id).kind else { return Some(()); };
        if !slots.captures.contains(&name) { return Some(()); }
        let slot = slots.resolve(name)?;
        if slots.host_bindings_by_slot.contains_key(&slot) { return Some(()); }
        let Some(binding) = self.top_level_known.get(&name)?.lexical_binding else { return Some(()); };
        let solved = self.solved();
        let origin = self.expression_identity(id);
        let caller = *solved.expression_owners.get(&origin)?;
        let flow = *solved.expression_producer_flows.get(&origin)?;
        let node = solved.producer_flows.node(flow).ok()?;
        let ProducerFlowKind::CapturedBinding { identity, version, input } = node.kind else { return None; };
        if node.source != ProducerFlowSource::Expression(origin)
            || identity != binding || solved.binding_producer_flows.get(&(identity, version)) != Some(&input) {
            return None;
        }
        let definition = solved.bindings.get(&binding)?;
        if definition.owner == Some(caller) { return None; }
        let source_type = ScopedRoot {
            ty: *solved.expressions.get(&origin)?,
            scope: solved.expression_scope(origin, Some(caller)).ok()?,
        };
        solved.graph.validate_scoped(source_type).ok()?;
        if super::super::indexed::generic::graph_ground_type(&solved.graph, source_type.ty).is_err() { return Some(()); }
        if !matches!(self.scratch.borrow().expressions.get(lowered.index()), Some(BuildExprRow::Param(actual)) if *actual == slot) { return None; }
        self.scratch.borrow_mut().lexical_capture_reads.insert(origin, BuildLexicalCaptureRead {
            binding, caller, origin, slot, expression: lowered, source_type,
        });
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::syntax::parser::Parser;

    #[test]
    fn original_lexical_capture_lowering_requires_independent_binding_read_and_owner_facts() {
        crate::runtime::eval::run_eval(|| {
            let source = "let base: Int = 3\npure plus(value: Int) -> Int { value + base }\nlet alias = plus\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("lexical-capture-source.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut declarations = Checker::check_compact_declarations(&parsed.arena);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            let read = (0..parsed.arena.stats().expressions).find_map(|index| {
                let expression = ExprId::from_index(index);
                let origin = ExpressionIdentity { source: source_id, namespace: None, expression };
                (matches!(parsed.arena.arena.expr(expression).kind, ArenaExprKind::Ident(name) if name == "base")
                    && bodies.solved.expression_owners.contains_key(&origin)).then_some(origin)
            }).unwrap();
            let caller = bodies.solved.expression_owners[&read];
            let flow = bodies.solved.expression_producer_flows[&read];
            let ProducerFlowKind::CapturedBinding { identity: binding, .. } = bodies.solved.producer_flows.node(flow).unwrap().kind else { panic!("the checker retains the selected original binding"); };
            declarations.solved = Default::default();
            let lower = |bodies: &CompactBodyProbeOutput| {
                let mut result = None;
                let lowered = lower_compact_function_units_into(&parsed.arena, &declarations, bodies, source, &sources,
                    StdlibLowerLinkage::Local, |unit| {
                        if unit.key == LoweredFunctionKey::Name(Name::intern("plus")) {
                            result = Some(unit.body.map(|body| {
                                let scratch = body.scratch.borrow();
                                let read = scratch.lexical_capture_reads.get(&read).expect("the source read has a separate capture receipt");
                                assert_eq!(read.binding, binding);
                                assert_eq!(read.caller, caller);
                                assert!(body.captures.iter().any(|capture| capture.lexical_binding == Some(binding) && capture.slot == read.slot));
                                assert!(matches!(scratch.expressions[read.expression.index()], BuildExprRow::Param(slot) if slot == read.slot));
                            }).is_some());
                        }
                        Ok(())
                    });
                lowered.is_ok() && result.unwrap_or(false)
            };
            assert!(lower(&bodies));
            let definition = Arc::get_mut(&mut bodies.solved).unwrap().bindings.remove(&binding).unwrap();
            assert!(!lower(&bodies), "a matching read type cannot replace the original binding fact");
            Arc::get_mut(&mut bodies.solved).unwrap().bindings.insert(binding, definition);
            assert!(lower(&bodies));
            Arc::get_mut(&mut bodies.solved).unwrap().expression_owners.remove(&read);
            assert!(!lower(&bodies), "the capture allocation cannot replace a missing read owner");
            Arc::get_mut(&mut bodies.solved).unwrap().expression_owners.insert(read, caller);
            Arc::get_mut(&mut bodies.solved).unwrap().expression_producer_flows.remove(&read);
            assert!(!lower(&bodies), "equal names and types cannot replace a missing checked binding relationship");
            Arc::get_mut(&mut bodies.solved).unwrap().expression_producer_flows.insert(read, flow);
            assert!(lower(&bodies));
        });
    }
}
