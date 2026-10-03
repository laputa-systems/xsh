use super::*;
use crate::syntax::arena::ExprId;
use crate::sema::inference::TypeNode;

impl Checker {
    /// A Result field selection constrains the original producer's success
    /// root; the carrier remains a Result and retains its error domain.
    pub(super) fn graph_checked_result_record_projection(&mut self, arena: &ArenaProgram, expression: ExprId, base: ExprId, field: Name) -> Type {
        let identity = self.expression_identity(arena, expression);
        if !self.graph_generation {
            return self.generic.borrow().facts.projections.get(&identity).map(|projection| self.graph_view(projection.result)).unwrap_or(Type::Invalid);
        }
        let origin = self.expression_identity(arena, base);
        let receiver = self.generic.borrow().facts.expressions.get(&origin).copied();
        let Some(receiver) = receiver else {
            self.error(arena.arena.expr(base).span, "Result field selection lacks its checked producer", "check.result-projection-source");
            return Type::Invalid;
        };
        let success = {
            let state = self.generic.borrow();
            state.facts.graph.resolved(receiver).and_then(|receiver| state.facts.graph.node(receiver)).map(|node| match node {
                TypeNode::Result(success, _) => Some(*success),
                _ => None,
            })
        };
        match success {
            Ok(Some(success)) => self.graph_projection(arena, expression, success, field),
            Ok(None) => {
                self.error(arena.arena.expr(base).span, "Result field selection has another checked carrier", "check.result-projection-source");
                Type::Invalid
            }
            Err(error) => { self.graph_error(arena.arena.expr(base).span, error); Type::Invalid }
        }
    }
}
