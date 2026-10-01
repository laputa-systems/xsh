use super::{DeclarationIdentity, ExpressionIdentity, SolvedExpressionCallable, Type};
use crate::sema::inference::{ConstraintRelation, InferenceContext, InferenceError, ScopedRoot, TypeId, TypeNode};
use crate::symbol::Name;
use crate::syntax::arena::{ArenaExprKind, ArenaProgram, ExprId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleProjectionKind {
    Field,
    Get { key: ExpressionIdentity },
    Index { key: ExpressionIdentity },
}

/// A module export keeps its complete callable promise through the original
/// access. The access carrier does not identify an implementation declaration.
#[derive(Clone, Debug)]
pub struct SolvedModuleProjection {
    pub receiver: ExpressionIdentity,
    pub source: TypeId,
    pub field: Name,
    pub field_type: TypeId,
    pub optional: bool,
    pub result: TypeId,
    pub kind: ModuleProjectionKind,
    pub caller: Option<DeclarationIdentity>,
    pub contribution: usize,
}

impl super::Checker {
    pub(super) fn record_graph_module_projection(&mut self, arena: &ArenaProgram, expression: ExprId, checked: &Type) -> Option<Type> {
        if !self.graph_generation { return None; }
        let span = arena.arena.expr(expression).span;
        let (receiver, field, kind) = if let ArenaExprKind::Field { base, name } = arena.arena.expr(expression).kind {
            (base, name, ModuleProjectionKind::Field)
        } else {
            let projection = self.projections.get(&span)?;
            let key = self.expression_identity(arena, projection.key);
            (projection.receiver, projection.field, match projection.operation {
                crate::sema::projection::ProjectionOperation::Get => ModuleProjectionKind::Get { key },
                crate::sema::projection::ProjectionOperation::Index => ModuleProjectionKind::Index { key },
            })
        };
        let identity = self.expression_identity(arena, expression);
        let receiver = self.expression_identity(arena, receiver);
        let original = self.generic.borrow().facts.expressions.get(&receiver).copied();
        let source = match original {
            Some(source) => source,
            None => {
                let physical = self.expr_types.get(&arena.arena.expr(receiver.expression).span)?.clone();
                if !matches!(physical, Type::Module(_)) { return None; }
                match self.graph_type(&physical, span) {
                    Ok(source) => source,
                    Err(error) => { self.graph_error(span, error); return Some(Type::Invalid); }
                }
            }
        };
        let is_module = {
            let state = self.generic.borrow();
            state.facts.graph.resolved(source).and_then(|source| state.facts.graph.node(source))
                .is_ok_and(|node| matches!(node, TypeNode::Module(_)))
        };
        if !is_module { return None; }
        let error = if matches!(kind, ModuleProjectionKind::Get { .. }) {
            let Type::Result(_, error) = checked else { return None; };
            match self.graph_type(error, span) { Ok(error) => Some(error), Err(error) => { self.graph_error(span, error); return Some(Type::Invalid); } }
        } else { None };
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            let contribution = state.facts.graph.constraint_origins().len();
            let level = u32::from(self.current_generic.is_some());
            let field_type = state.facts.graph.require_field(source, field, level, reason)?;
            let optional = state.facts.graph.module_field(source, field)?.optional;
            let result = if let Some(error) = error { state.facts.graph.result(field_type, error)? } else { field_type };
            state.facts.graph.charge_source_fact_nodes(1)?;
            state.facts.graph.charge_source_fact_edges(5 + u64::from(!matches!(kind, ModuleProjectionKind::Field)))?;
            state.facts.graph.charge_source_fact_work(1)?;
            state.facts.expressions.insert(receiver, source);
            state.facts.expressions.insert(identity, result);
            if let Some(owner) = self.current_generic {
                state.facts.expression_owners.insert(receiver, owner);
                state.facts.expression_owners.insert(identity, owner);
            }
            if error.is_none() && matches!(state.facts.graph.node(state.facts.graph.resolved(result)?)?, TypeNode::Arrow(_) | TypeNode::NativeCallable(_) | TypeNode::CallableChoice(_)) {
                state.facts.expression_callables.insert(identity, SolvedExpressionCallable { signature: result, scheme: None, declaration: None });
            }
            state.facts.module_projections.insert(identity, SolvedModuleProjection { receiver, source, field, field_type, optional, result, kind, caller: self.current_generic, contribution });
            Ok::<_, InferenceError>(Type::Graph(result))
        })();
        Some(match outcome { Ok(result) => result, Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }
}

impl<Graph> super::SolvedTypes<Graph> {
    pub(super) fn module_projection_roots(&self) -> Result<Vec<ScopedRoot>, InferenceError> {
        let mut roots = Vec::with_capacity(self.module_projections.len() * 3);
        for (identity, projection) in &self.module_projections {
            let scope = self.expression_scope(*identity, projection.caller)?;
            roots.extend([ScopedRoot { ty: projection.source, scope: self.expression_scope(projection.receiver, projection.caller)? }, ScopedRoot { ty: projection.field_type, scope }, ScopedRoot { ty: projection.result, scope }]);
        }
        Ok(roots)
    }

    pub(super) fn validate_module_projections(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        for (identity, projection) in &self.module_projections {
            if identity.source != projection.receiver.source || identity.namespace != projection.receiver.namespace || *identity == projection.receiver
                || self.expression_owners.get(identity).copied() != projection.caller { return Err(InferenceError::InvalidScheme); }
            let receiver = self.expressions.get(&projection.receiver).ok_or(InferenceError::InvalidScheme)?;
            let result = self.expressions.get(identity).ok_or(InferenceError::InvalidScheme)?;
            if graph.resolved(*receiver)? != graph.resolved(projection.source)? || graph.resolved(*result)? != graph.resolved(projection.result)? { return Err(InferenceError::InvalidScheme); }
            let field = graph.module_field(projection.source, projection.field)?;
            if field.ty != projection.field_type || field.optional != projection.optional { return Err(InferenceError::InvalidScheme); }
            let origin = graph.constraint_origins().get(projection.contribution).ok_or(InferenceError::InvalidScheme)?;
            if origin.relation != (ConstraintRelation::ModuleProjection { module: projection.source, label: projection.field, result: projection.field_type, optional: projection.optional }) { return Err(InferenceError::InvalidScheme); }
            match projection.kind {
                ModuleProjectionKind::Get { key } => {
                    if key.source != identity.source || key.namespace != identity.namespace || key == *identity { return Err(InferenceError::InvalidScheme); }
                    let TypeNode::Result(success, _) = graph.node(graph.resolved(projection.result)?)? else { return Err(InferenceError::InvalidScheme); };
                    if graph.resolved(*success)? != graph.resolved(projection.field_type)? { return Err(InferenceError::InvalidScheme); }
                }
                ModuleProjectionKind::Index { key } => {
                    if key.source != identity.source || key.namespace != identity.namespace || key == *identity || projection.result != projection.field_type { return Err(InferenceError::InvalidScheme); }
                }
                ModuleProjectionKind::Field => if projection.result != projection.field_type { return Err(InferenceError::InvalidScheme); },
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::inference::{CallableKind, EffectSet, EffectSummary};
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn module_projection_carriers_keep_original_export_proofs_after_syntax_drop() {
        let source = "type Plugin = module { export pure render(value: Str, suffix: Str = \"!\") -> Str; export proc clock() [time] -> Int }\nproc inspect(plugin: Plugin) [time, error] -> Str { let direct = plugin.render; let found = plugin.get(\"render\")?; let clock = plugin[\"clock\"]; let _ = clock(); let _ = direct(value: \"direct\"); found(value: \"found\") }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(73), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut checked = super::super::Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.module_projections.len(), 3);
        let projections = checked.solved.module_projections.values().cloned().collect::<Vec<_>>();
        assert!(projections.iter().any(|projection| projection.kind == ModuleProjectionKind::Field));
        assert!(projections.iter().any(|projection| matches!(projection.kind, ModuleProjectionKind::Get { .. })));
        assert!(projections.iter().any(|projection| matches!(projection.kind, ModuleProjectionKind::Index { .. })));
        for projection in &projections {
            let field = checked.solved.graph.module_field(projection.source, projection.field).unwrap();
            assert_eq!(field.ty, projection.field_type);
            let TypeNode::Arrow(arrow) = checked.solved.graph.node(field.ty).unwrap() else { panic!("complete written callable promise") };
            if projection.field == "render" {
                assert_eq!(arrow.kind, CallableKind::Pure);
                assert_eq!(arrow.params[0].label, "value");
                assert_eq!(arrow.params[1].label, "suffix");
                assert!(arrow.params[1].defaulted);
            } else {
                assert_eq!(arrow.kind, CallableKind::Proc);
                assert_eq!(checked.solved.graph.resolved_effect_summary(arrow.effects).unwrap(), EffectSummary::Closed(EffectSet::TIME));
            }
        }
        drop(parsed);
        let before = checked.solved.graph.counters().clone();
        checked.solved.validate().unwrap();
        let identity = *checked.solved.module_projections.keys().next().unwrap();
        let original = checked.solved.module_projections[&identity].clone();
        std::sync::Arc::get_mut(&mut checked.solved).unwrap().module_projections.get_mut(&identity).unwrap().optional = !original.optional;
        assert!(checked.solved.validate().is_err(), "presence cannot be rewritten independently of the core export proof");
        std::sync::Arc::get_mut(&mut checked.solved).unwrap().module_projections.insert(identity, original.clone());
        let other = projections.iter().find(|projection| projection.field != original.field).unwrap();
        std::sync::Arc::get_mut(&mut checked.solved).unwrap().module_projections.get_mut(&identity).unwrap().field_type = other.field_type;
        assert!(checked.solved.validate().is_err(), "another export cannot replace the original promise");
        std::sync::Arc::get_mut(&mut checked.solved).unwrap().module_projections.insert(identity, original);
        checked.solved.validate().unwrap();
        let after = checked.solved.graph.counters();
        assert_eq!(before.attempted_constraints, after.attempted_constraints);
        assert_eq!(before.unifications, after.unifications);
        assert_eq!(before.instantiations, after.instantiations);
    }
}
