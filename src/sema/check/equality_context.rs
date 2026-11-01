use super::{BTreeSet, CheckedResultConstructor, Checker, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::constraints::CallableApplicationScope;
use crate::syntax::arena::{ArenaProgram, ExprId};

impl Checker {
    pub(super) fn contextualize_equality_constructor_holes(&mut self, arena: &ArenaProgram, operand: ExprId, actual: &Type, context: &Type) -> bool {
        if !actual.contains_inference() || context.contains_inference() || context.contains_any() || context.contains_recovery() { return true; }
        let span = arena.arena.expr(operand).span;
        let mut source = operand;
        for _ in 0..arena.arena.expr_tags.len() {
            let Some(origin) = self.resolved_bindings.immutable_value_source(arena, source) else { break; };
            source = origin;
        }
        let source = arena.arena.expr(source).span;
        let holes = self.result_constructors.iter().filter_map(|(&producer, kind)| {
            if producer.source_id != source.source_id || producer.start() < source.start() || producer.end() > source.end()
                || self.guarded_nodes.get(&producer) != self.guarded_nodes.get(&span)
            { return None; }
            let Type::Result(success, error) = self.expr_types.get(&producer)? else { return None; };
            let hole = match kind { CheckedResultConstructor::Ok => error, CheckedResultConstructor::Err => success };
            matches!(hole.as_ref(), Type::Inference(_)).then(|| (hole.as_ref().clone(), producer))
        }).collect::<Vec<_>>();
        if holes.is_empty() { return true; }
        let mut component = self.declaration_inference.pending_entries().map(|(id, _)| id.effect_declaration()).collect::<BTreeSet<_>>();
        let mut inputs = self.declaration_inference.pending_entries().flat_map(|(_, pending)| {
            pending.provisional().into_iter().flat_map(|shape| shape.params().iter().map(|parameter| parameter.signature.ty.clone()))
        }).collect::<Vec<_>>();
        if let Some(owner) = self.stream_origin_owner { component.insert(owner); }
        for binding in self.scopes.iter().flat_map(|scope| scope.values()) {
            let local = binding.id.and_then(|id| self.resolved_bindings.declarations.get(&id))
                .zip(self.stream_origin_owner).is_some_and(|(declaration, owner)| {
                    declaration.span.source_id == owner.body.source_id && declaration.span.start() >= owner.body.start() && declaration.span.end() <= owner.body.end()
                });
            if self.stream_origin_owner.is_some() && !local { inputs.push(binding.ty.clone()); }
        }
        let owner = self.stream_origin_owner.map_or(CallableApplicationScope::Invocation, CallableApplicationScope::Declaration);
        match self.type_constraints.contextualize_equality_holes(owner, &component, &inputs, &holes, actual, context, span) {
            Ok(()) => true,
            Err(error) => {
                self.error(span, &format!("fresh result equality context failed: {error}"), DiagnosticCode::CheckTypeMismatch);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parser::Parser;
    use crate::source::SourceId;
    use crate::syntax::arena::ArenaExprKind;

    #[test]
    fn equality_context_requires_the_actual_checked_constructor_producer() {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), "keep(Err(\"opaque\"))");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let expressions = parsed.arena.arena.source_rows().expr_ids(SourceId::new(0)).filter(|&id| matches!(parsed.arena.arena.expr(id).kind, ArenaExprKind::Call { .. })).collect::<Vec<_>>();
            let operand = *expressions.iter().max_by_key(|&&id| parsed.arena.arena.expr(id).span.end() - parsed.arena.arena.expr(id).span.start()).unwrap();
            let constructor = *expressions.iter().min_by_key(|&&id| parsed.arena.arena.expr(id).span.end() - parsed.arena.arena.expr(id).span.start()).unwrap();
            let producer = parsed.arena.arena.expr(constructor).span;
            let mut checker = Checker::new(super::super::CheckOptions::for_execution());
            let hole = checker.type_constraints.fresh(producer);
            let actual = Type::Result(Box::new(hole.clone()), Box::new(Type::Str));
            let expected = Type::Result(Box::new(Type::Int), Box::new(Type::Str));
            checker.expr_types.insert(producer, actual.clone());
            assert!(checker.contextualize_equality_constructor_holes(&parsed.arena, operand, &actual, &expected));
            assert_eq!(checker.type_constraints.resolve(&hole).unwrap(), hole);
            checker.result_constructors.insert(producer, CheckedResultConstructor::Err);
            let unrelated = checker.type_constraints.fresh(producer);
            checker.expr_types.insert(producer, Type::Result(Box::new(unrelated.clone()), Box::new(Type::Str)));
            assert!(checker.contextualize_equality_constructor_holes(&parsed.arena, operand, &actual, &expected));
            assert_eq!(checker.type_constraints.resolve(&hole).unwrap(), hole);
            assert_eq!(checker.type_constraints.resolve(&unrelated).unwrap(), unrelated);
            checker.expr_types.insert(producer, actual.clone());
            let guard = checker.type_constraints.register_non_null_guard(CallableApplicationScope::Invocation, Type::Optional(Box::new(Type::Str)), Type::Str, producer, parsed.arena.arena.expr(operand).span).unwrap();
            checker.guarded_nodes.insert(producer, guard);
            assert!(checker.contextualize_equality_constructor_holes(&parsed.arena, operand, &actual, &expected));
            assert_eq!(checker.type_constraints.resolve(&hole).unwrap(), hole);
            checker.guarded_nodes.remove(&producer);
            assert!(checker.contextualize_equality_constructor_holes(&parsed.arena, operand, &actual, &expected));
            assert_eq!(checker.type_constraints.resolve(&hole).unwrap(), Type::Int);
        });
    }
}
