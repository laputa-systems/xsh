use super::*;
use crate::sema::check::{BindingIdentity, ComprehensionIdentity, ProducerFlowSource};
use crate::sema::inference::{CandidateId, TypeId};
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};
use crate::source::SourceId;

#[derive(Clone, Copy)]
pub(super) enum CheckedIterationOrigin {
    Statement(StmtId),
    Comprehension { expression: ExprId, qualifier: u32 },
}

pub(super) struct CheckedIterationProjection {
    pub source: ProducerFlowSource,
    pub iterator: ExpressionIdentity,
    pub candidate: CandidateId,
    pub authority: Name,
    pub item_type: TypeId,
    pub item: Type,
    pub domain: IterableDomain,
    pub outer_result: bool,
}

struct CheckedIterationFact {
    candidate: CandidateId,
    authority: Name,
    item_type: TypeId,
    domain: IterableDomain,
    outer_result: bool,
}

// The original operand endpoint and selected canonical member jointly
// authorize item storage and failure transport. A pending family cannot
// choose a carrier here; it needs a checked instance before preparation.
fn checked_iteration_fact(solved: &SolvedTypes, source: ProducerFlowSource, iterator: ExpressionIdentity) -> Option<CheckedIterationFact> {
    let operation = match source {
        ProducerFlowSource::Statement(identity) if identity.source == iterator.source && identity.namespace == iterator.namespace => solved.statement_operations.get(&identity)?,
        ProducerFlowSource::Comprehension(identity) if identity.expression.source == iterator.source && identity.expression.namespace == iterator.namespace => &solved.comprehension_operations.get(&identity)?.operation,
        _ => return None,
    };
    if operation.receiver.is_some() || operation.actual_arguments.len() != 1
        || operation.binding.supplied_slots != [0] || !operation.binding.default_slots.is_empty()
        || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() { return None; }
    let graph = &solved.graph;
    let expression = *solved.expressions.get(&iterator)?;
    if graph.resolved(operation.actual_arguments[0]).ok()? != graph.resolved(expression).ok()? { return None; }
    let selected = graph.candidate_evidence(operation.requirement).ok()??;
    let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).ok()? else { return None; };
    let PreparedLanguageOperation::Iteration { domain, outer_result } = metadata.operation else { return None; };
    Some(CheckedIterationFact { candidate: selected.candidate, authority: metadata.identity, item_type: operation.result, domain, outer_result })
}

impl super::super::BuildIterationBindingOrigin {
    // A formal producer port identifies the original parameter independently
    // of the type shared by other parameters or their emitted slot words.
    pub(in crate::runtime::eval) fn original_parameter(solved: &SolvedTypes, iterator: ExpressionIdentity, caller: Option<crate::sema::check::DeclarationIdentity>) -> Option<Option<(crate::sema::check::DeclarationIdentity, u32)>> {
        use crate::sema::check::ProducerFlowKind;
        let flow = *solved.expression_producer_flows.get(&iterator)?;
        let node = solved.producer_flows.node(flow).ok()?;
        if node.source != ProducerFlowSource::Expression(iterator) { return None; }
        let ProducerFlowKind::Join { inputs } = &node.kind else { return Some(None); };
        let [input] = inputs.as_slice() else { return Some(None); };
        let node = solved.producer_flows.node(*input).ok()?;
        let ProducerFlowKind::Parameter { declaration, index } = node.kind else { return Some(None); };
        if caller != Some(declaration) || declaration.source != iterator.source || declaration.namespace != iterator.namespace
            || node.source != (ProducerFlowSource::Parameter { declaration, index }) { return None; }
        let function = solved.declarations.get(&declaration)?;
        if function.parameter_producer_flows.get(index as usize) != Some(input) { return None; }
        let signature = solved.graph.callable_signature(function.signature).ok()?;
        let crate::sema::inference::TypeNode::Arrow(arrow) = solved.graph.node(solved.graph.resolved(signature).ok()?).ok()? else { return None; };
        if index as usize >= arrow.params.len() { return None; }
        Some(Some((declaration, index)))
    }
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn checked_iteration_projection(&self, origin: CheckedIterationOrigin, iter: ExprId) -> Option<CheckedIterationProjection> {
        let source = match origin {
            CheckedIterationOrigin::Statement(statement) => {
                match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::For { iter: original, .. } | ArenaStmtKind::YieldDelegate(original) if original == iter => {},
                    _ => return None,
                }
                ProducerFlowSource::Statement(self.statement_identity(statement))
            }
            CheckedIterationOrigin::Comprehension { expression, qualifier } => {
                let range = match self.program.arena.expr(expression).kind {
                    ArenaExprKind::ListComp { qualifiers, .. } | ArenaExprKind::MapComp { qualifiers, .. } => qualifiers,
                    _ => return None,
                };
                match self.program.arena.comp_qualifiers(range).get(qualifier as usize)? {
                    crate::syntax::arena::ArenaCompQualifier::For { iter: original, .. } if *original == iter => {},
                    _ => return None,
                }
                ProducerFlowSource::Comprehension(ComprehensionIdentity { expression: self.expression_identity(expression), qualifier })
            }
        };
        let iterator = self.expression_identity(iter);
        let fact = checked_iteration_fact(self.solved(), source, iterator)?;
        let item = self.solved_type(fact.item_type)?;
        Some(CheckedIterationProjection { source, iterator, candidate: fact.candidate, authority: fact.authority, item_type: fact.item_type, item, domain: fact.domain, outer_result: fact.outer_result })
    }

    // The checked iterable selects failure transport. Materialized map and
    // scalar sources propagate their original Result error lexically; list
    // and stream adapters retain their own runtime error transport.
    pub(super) fn lower_checked_iterable(&mut self, origin: CheckedIterationOrigin, iter: ExprId, slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>) -> Option<(BuildExprId, CheckedIterationProjection)> {
        let Some(projection) = self.checked_iteration_projection(origin, iter) else {
            self.last_blocker_detail = Some((self.program.arena.expr(iter).span, "iteration has no selected original source operation".into()));
            return None;
        };
        let lowered = self.lower_expr(iter, slots, current_function, item_slot)?;
        let lowered = if projection.outer_result && matches!(projection.domain, IterableDomain::Map | IterableDomain::Str | IterableDomain::Bytes) {
            push_build_row!(self, expr, BuildExprRow::Try(lowered))
        } else { lowered };
        Some((lowered, projection))
    }

    pub(super) fn checked_iteration_binding_supported(&self, projection: &CheckedIterationProjection) -> bool {
        projection.domain == IterableDomain::List && !projection.outer_result
            && self.solved().graph.export_type(projection.item_type) == Ok(Type::Str)
    }

    // Only the selected source operation authorizes the item. A storage kind
    // or a same-typed slot never identifies which loop initialized a read.
    pub(super) fn record_checked_iteration_binding(&self, statement: StmtId, target: BindingTargetId, projection: &CheckedIterationProjection, row: BuildStmtId, iterator: BuildExprId, slot: usize) -> Option<()> {
        if !self.checked_iteration_binding_supported(projection) { return Some(()); }
        let graph = &self.solved().graph;
        let statement = self.statement_identity(statement);
        let ProducerFlowSource::Statement(source) = projection.source else { return None; };
        if source != statement { return None; }
        let binding = BindingIdentity { source: statement.source, namespace: statement.namespace, target };
        let target_source = self.program.arena.binding_target(target);
        if target_source.span.is_some_and(|span| self.program.arena.span(span).source_id != statement.source) { return None; }
        if !matches!(target_source.kind, ArenaBindingTargetKind::Name(name) if !is_discard_name(name)) { return Some(()); }
        let solved = self.solved();
        let operation = solved.statement_operations.get(&statement)?;
        let original = solved.bindings.get(&binding)?;
        if original.mutable || original.owner != operation.caller { return None; }
        let input = crate::sema::inference::ScopedRoot { ty: *solved.expressions.get(&projection.iterator)?, scope: solved.expression_scope(projection.iterator, operation.caller).ok()? };
        let item = crate::sema::inference::ScopedRoot { ty: operation.result, scope: solved.operation_scope(projection.source, operation).ok()? };
        let binding_type = crate::sema::inference::ScopedRoot { ty: original.ty, scope: original.scheme.or(item.scope) };
        for root in [input, item, binding_type] { graph.validate_scoped(root).ok()?; }
        if graph.export_type(input.ty).ok()? != Type::List(Box::new(Type::Str))
            || graph.export_type(item.ty).ok()? != Type::Str || graph.export_type(binding_type.ty).ok()? != Type::Str { return None; }
        let iterator_parameter = super::super::BuildIterationBindingOrigin::original_parameter(solved, projection.iterator, operation.caller)?;
        let mut scratch = self.scratch.borrow_mut();
        match iterator_parameter {
            Some((_, index)) if matches!(scratch.expressions.get(iterator.index()), Some(BuildExprRow::Param(slot)) if *slot == index as usize) => {},
            None if matches!(self.program.arena.expr(projection.iterator.expression).kind, ArenaExprKind::List(_))
                && matches!(scratch.expressions.get(iterator.index()), Some(BuildExprRow::List(_))) => {},
            _ => return None,
        }
        if scratch.iteration_binding_origins.contains_key(&binding) { return None; }
        scratch.iteration_binding_origins.insert(binding, super::super::BuildIterationBindingOrigin {
            statement, binding, iterator_source: projection.iterator, caller: operation.caller, selected: projection.candidate,
            input, item, binding_type, iterator_parameter, row, iterator, slot,
        });
        Some(())
    }

    // Each destructured leaf owns a checked binding fact, including nested
    // fields whose item is represented by a quantified structural endpoint.
    pub(super) fn lower_checked_iteration_target(&self, target: BindingTargetId, source: SourceId, slots: &mut SlotScope) -> Option<LoweredCompTarget> {
        match self.program.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => {
                if is_discard_name(name) { return Some(LoweredCompTarget::Discard); }
                if slots.is_declared_here(name) { return None; }
                let identity = BindingIdentity { source, namespace: self.current_namespace, target };
                let binding = self.solved().bindings.get(&identity)?;
                let ty = self.solved_type(binding.ty)?;
                Some(LoweredCompTarget::Slot(slots.declare_with_type(name, Some(ty))))
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                let mut lowered = LoweredCompFields::new();
                for field in self.program.arena.destructure_fields(fields) {
                    let span = self.program.arena.span(field.span);
                    if span.source_id != source { return None; }
                    let child = self.lower_checked_iteration_target(field.target, source, slots)?;
                    lowered.push((field.name, Box::new(child), span));
                }
                Some(LoweredCompTarget::Record { fields: lowered })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn original_list_item_binding_transport_keeps_loop_and_read_authority_after_arena_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc quoted(values: List[Str]) [io] {\n for value in values { print ${shlex.quote(value)} }\n}\nquoted([\"one\", \"two\"])\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("iteration-item-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(Arc::ptr_eq(&checked.solved, &bodies.solved));
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources, StdlibLowerLinkage::Local, |unit| {
                assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                functions.push(unit.body.unwrap()); Ok(())
            }).unwrap();
            let operation = *checked.solved.statement_operations.keys().find(|identity| matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::For { .. })).unwrap();
            let ArenaStmtKind::For { target, iter, .. } = parsed.arena.arena.stmt(operation.statement).kind else { unreachable!() };
            let binding = BindingIdentity { source: operation.source, namespace: operation.namespace, target };
            let iterator = ExpressionIdentity { source: operation.source, namespace: operation.namespace, expression: iter };
            drop(parsed);
            let scratch = functions[0].scratch.borrow();
            let original = scratch.iteration_binding_origins.get(&binding).expect("the original loop item retains its own source operation and physical binding");
            assert_eq!(original.statement, operation);
            assert_eq!(original.iterator_source, iterator);
            assert!(matches!(&scratch.statements[original.row.index()], BuildStmtRow::For { slot, iter, .. } if *slot == original.slot && *iter == original.iterator));
            assert!(scratch.iteration_binding_uses.values().any(|actual| *actual == binding), "actual native argument reads retain the original iteration binding");
            assert_eq!(checked.solved.graph.export_type(original.input.ty).unwrap(), Type::List(Box::new(Type::Str)));
            assert_eq!(checked.solved.graph.export_type(original.item.ty).unwrap(), Type::Str);
            assert_eq!(checked.solved.graph.export_type(original.binding_type.ty).unwrap(), Type::Str);
        });
    }

    #[test]
    fn original_list_item_native_operand_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc quoted(values: List[Str]) [io] {\n for value in values { print ${shlex.quote(value)} }\n}\nquoted([\"one\", \"two\"])\n", b"one\ntwo\n");
    }

    #[test]
    fn original_list_item_uses_follow_nested_loops_and_restore_after_local_shadows() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc quoted(values: List[Str]) [io] {\n for value in values {\n  print ${shlex.quote(value)}\n  for value in values { print ${shlex.quote(value)} }\n  if true { let value = \"shadow\"; print ${shlex.quote(value)} }\n  print ${shlex.quote(value)}\n }\n}\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("iteration-item-shadow.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut loops = checked.solved.statement_operations.keys().filter_map(|identity| {
                let statement = parsed.arena.arena.stmt(identity.statement);
                let ArenaStmtKind::For { target, .. } = statement.kind else { return None; };
                Some((statement.span.start(), BindingIdentity { source: identity.source, namespace: identity.namespace, target }))
            }).collect::<Vec<_>>();
            loops.sort_by_key(|(start, _)| *start);
            assert_eq!(loops.len(), 2);
            let reads = source.match_indices("quote(value)").map(|(start, _)| {
                *checked.solved.expressions.keys().find(|identity| {
                    let expression = parsed.arena.arena.expr(identity.expression);
                    matches!(expression.kind, ArenaExprKind::Ident(_)) && expression.span.start() as usize == start + "quote(".len()
                }).expect("each actual native argument owns an original expression")
            }).collect::<Vec<_>>();
            assert_eq!(reads.len(), 4);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources, StdlibLowerLinkage::Local, |unit| {
                assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                functions.push(unit.body.unwrap()); Ok(())
            }).unwrap();
            drop(parsed);
            let scratch = functions[0].scratch.borrow();
            assert_eq!(scratch.iteration_binding_origins.len(), 2);
            let expected = [Some(loops[0].1), Some(loops[1].1), None, Some(loops[0].1)];
            for (read, binding) in reads.into_iter().zip(expected) {
                assert_eq!(scratch.iteration_binding_uses.get(&read).copied(), binding);
            }
            assert_ne!(scratch.iteration_binding_origins[&loops[0].1].slot, scratch.iteration_binding_origins[&loops[1].1].slot);
        });
    }

    #[test]
    fn original_list_item_parameter_ports_refuse_missing_and_foreign_expression_mappings() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc quoted(first: List[Str], second: List[Str]) [io] {\n for left in first { print ${shlex.quote(left)} }\n for right in second { print ${shlex.quote(right)} }\n}\n";
            for missing in [true, false] {
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let mut declarations = Checker::check_compact_declarations(&parsed.arena);
                assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
                let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
                assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
                parsed.arena.symbol_owner().with_current(|| {
                    let valid = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                    assert_eq!(valid.blocker_events, 0);
                    let iterators = bodies.solved.statement_operations.keys().filter_map(|identity| {
                        let ArenaStmtKind::For { iter, .. } = parsed.arena.arena.stmt(identity.statement).kind else { return None; };
                        Some(ExpressionIdentity { source: identity.source, namespace: identity.namespace, expression: iter })
                    }).collect::<Vec<_>>();
                    assert_eq!(iterators.len(), 2);
                    declarations.solved = Default::default();
                    let solved = Arc::get_mut(&mut bodies.solved).expect("the mutation owns the original solved snapshot");
                    if missing {
                        solved.expression_producer_flows.remove(&iterators[0]);
                    } else {
                        let other = solved.expression_producer_flows[&iterators[1]];
                        solved.expression_producer_flows.insert(iterators[0], other);
                    }
                    let refused = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                    assert!(refused.blocker_events > 0, "a parameter type cannot replace the original expression-to-formal producer port, missing={missing}");
                });
            }
        });
    }

    #[test]
    fn checked_iteration_domains_and_failure_transport_survive_arena_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "stream numbers() -> Stream[Int] { yield 1 }\nfor item in [1] { let _ = item }\nfor item in numbers() { let _ = item }\nfor item in {[\"key\"]: 1} { let _ = item }\nfor item in \"word\" { let _ = item }\nfor item in b\"word\" { let _ = item }\nfor item in Ok([1]) { let _ = item }\nfor item in Ok(numbers()) { let _ = item }\nfor item in Ok({[\"key\"]: 1}) { let _ = item }\nfor item in Ok(\"word\") { let _ = item }\nfor item in Ok(b\"word\") { let _ = item }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(51), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let roots = checked.solved.statement_operations.keys().filter_map(|identity| {
                let ArenaStmtKind::For { iter, .. } = parsed.arena.arena.stmt(identity.statement).kind else { return None; };
                Some((ProducerFlowSource::Statement(*identity), ExpressionIdentity { source: parsed.arena.arena.expr(iter).span.source_id, namespace: identity.namespace, expression: iter }))
            }).collect::<Vec<_>>();
            assert_eq!(roots.len(), 10);
            drop(parsed);
            let symbols = checked.solved.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut selected = Vec::new();
            for (source, iterator) in roots {
                let fact = checked_iteration_fact(&checked.solved, source, iterator).expect("each original source keeps a selected iteration contract");
                assert!(checked.solved.graph.export_type(fact.item_type).is_ok());
                let metadata = checked.solved.operation_catalog.candidate(&checked.solved.graph, fact.candidate).unwrap();
                assert!(matches!(metadata, crate::sema::check::SolvedOperationAuthority::Language(candidate) if candidate.identity == fact.authority));
                assert!(checked_iteration_fact(&checked.solved, source, ExpressionIdentity { source: SourceId::new(52), ..iterator }).is_none());
                selected.push((fact.domain, fact.outer_result));
            }
            for domain in [IterableDomain::List, IterableDomain::Stream, IterableDomain::Map, IterableDomain::Str, IterableDomain::Bytes] {
                for outer_result in [false, true] { assert!(selected.contains(&(domain, outer_result))); }
            }
        });
    }

    #[test]
    fn comprehension_iteration_keeps_generator_ordinals_across_filters() {
        crate::runtime::eval::run_eval(|| {
            let source = "let pairs = [left + right for left in [1, 2] if left > 0 for right in [3, 4]]\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(53), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let roots = checked.solved.comprehension_operations.keys().map(|identity| {
                let ArenaExprKind::ListComp { qualifiers, .. } = parsed.arena.arena.expr(identity.expression.expression).kind else { panic!("fixture owns a list comprehension") };
                let crate::syntax::arena::ArenaCompQualifier::For { iter, .. } = parsed.arena.arena.comp_qualifiers(qualifiers)[identity.qualifier as usize] else { panic!("only actual generator ordinals own iteration proofs") };
                (*identity, ExpressionIdentity { expression: iter, ..identity.expression })
            }).collect::<Vec<_>>();
            assert_eq!(roots.iter().map(|(identity, _)| identity.qualifier).collect::<Vec<_>>(), vec![0, 2]);
            drop(parsed);
            let symbols = checked.solved.symbol_owner().clone(); let _symbols = symbols.enter();
            for (identity, iterator) in roots {
                let fact = checked_iteration_fact(&checked.solved, ProducerFlowSource::Comprehension(identity), iterator).unwrap();
                assert_eq!(fact.domain, IterableDomain::List);
                assert!(!fact.outer_result);
                assert!(checked_iteration_fact(&checked.solved, ProducerFlowSource::Comprehension(ComprehensionIdentity { qualifier: 1, ..identity }), iterator).is_none());
            }
        });
    }

    #[test]
    fn original_for_operation_cannot_be_reconstructed_from_expression_types() {
        crate::runtime::eval::run_eval(|| {
            let source = "for value in [1, 2] { print ${value} }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut declarations = Checker::check_compact_declarations(&parsed.arena);
            assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let valid = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert_eq!(valid.blocker_events, 0, "checked iteration must lower before its proof is removed");
                let original = *bodies.solved.statement_operations.keys().find(|identity| {
                    matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::For { .. })
                }).expect("the checked loop retains its own operation");
                declarations.solved = Default::default();
                Arc::get_mut(&mut bodies.solved).expect("the fixture owns its solved snapshot").statement_operations.remove(&original);
                let absent = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert!(absent.blocker_events > 0, "iteration without its original source operation must refuse lowering");
            });
        });
    }

    #[test]
    fn original_comprehension_operations_cannot_be_reconstructed_from_expression_types() {
        crate::runtime::eval::run_eval(|| {
            let source = "let pairs = [left + right for left in [1, 2] if left > 0 for right in [3, 4]]\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut declarations = Checker::check_compact_declarations(&parsed.arena);
            assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let valid = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert_eq!(valid.blocker_events, 0);
                let original = *bodies.solved.comprehension_operations.keys().find(|identity| identity.qualifier == 2).unwrap();
                declarations.solved = Default::default();
                Arc::get_mut(&mut bodies.solved).expect("the fixture owns its solved snapshot").comprehension_operations.remove(&original);
                let absent = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert!(absent.constructed_top_level_statements < valid.constructed_top_level_statements, "the second generator must keep its own original source operation");
                assert!(absent.top_level_blockers.iter().any(|count| *count > 0), "the missing generator proof must make the containing source statement unavailable");
            });
        });
    }

    #[test]
    fn original_yield_delegation_operation_cannot_be_reconstructed_from_expression_types() {
        crate::runtime::eval::run_eval(|| {
            let source = "stream copied() -> Stream[Int] { yield @[1, 2] }\nlet values = copied() |> collect\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("checked-delegation.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources.clone());
                assert!(evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).is_ok(), "the original delegation prepares before its proof is removed");
                drop(evaluator);
                let original = *checked.solved.statement_operations.keys().find(|identity| matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::YieldDelegate(_))).unwrap();
                Arc::get_mut(&mut checked.solved).expect("the fixture owns its solved snapshot").statement_operations.remove(&original);
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                assert!(evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).is_err(), "delegation without its own source operation must not produce a prepared program");
            });
        });
    }

    #[test]
    fn checked_iteration_executes_all_carriers_and_nested_targets_after_arena_disposal() {
        let source = r#"stream numbers() -> Stream[Int] { yield 1 }
for item in [1] { print ${item} }
for item in numbers() { print ${item} }
for {value} in {["key"]: 1} { print ${value} }
for item in "a" { print ${item} }
for item in b"A" { print ${item} }
for item in Ok([1]) { print ${item} }
for item in Ok(numbers()) { print ${item} }
for {value} in Ok({["key"]: 1}) { print ${value} }
for item in Ok("a") { print ${item} }
for item in Ok(b"A") { print ${item} }
for {nested: {tag}} in [{nested: {tag: 7}}] { print ${tag} }
let pairs = [left + right for left in [1, 2] if left > 0 for right in [3, 4]]
let rows = {key: value for {key, value} in {["row"]: 9}}
print ${pairs[0]} ${pairs[3]} ${rows["row"]}
"#;
        execute_after_arena_disposal(source, b"1\n1\n1\na\n65\n1\n1\n1\na\n65\n7\n4 6 9\n");
    }

    #[test]
    fn checked_stream_iteration_keeps_pull_and_cleanup_lazy_after_arena_disposal() {
        let source = r#"stream numbers() [io] -> Stream[Int] {
    defer { print "close" }
    print "pull"
    yield 1
    yield 2
}
let values = numbers()
print "created"
for value in values { print ${value}; break }
print "done"
"#;
        execute_after_arena_disposal(source, b"created\npull\n1\nclose\ndone\n");
    }

    fn execute_after_arena_disposal(source: &'static str, expected: &'static [u8]) {
        crate::runtime::eval::run_eval(move || {
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("checked-iteration.xsh", source);
                let parsed = Parser::parse_source_arena_only(source_id, source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let solved = Arc::downgrade(&checked.solved);
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                let plan = parsed.arena.symbol_owner().with_current(|| evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked))
                    .expect("original checked iteration must prepare an entirely indexed program");
                let symbols = evaluator.indexed_program.as_ref().expect("preparation installs indexed code").symbol_owner().clone();
                drop(checked);
                drop(parsed);
                assert!(solved.upgrade().is_none(), "prepared iteration releases its original inference bundle");
                let evaluated = crate::runtime::eval::run_eval(move || symbols.with_current(|| {
                    let execute = || {
                        assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                        evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                    };
                    if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) }
                    else { execute() }
                }));
                let output = match evaluated { Ok(output) => output, Err(_) => panic!("prepared iteration must execute after arena disposal") };
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, expected, "{}", String::from_utf8_lossy(&output.stderr));
                assert!(output.stderr.is_empty(), "{:?}", output.stderr);
                assert!(output.traceback.is_none(), "{:?}", output.traceback);
            }
        });
    }
}
