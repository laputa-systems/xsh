use super::{Checker, Span};
use crate::sema::inference::{ErrorJoin, InferenceError, TypeId, TypeNode};

impl Checker {
    /// Inferred Result paths keep one success domain and join independently
    /// checked error contributions. A caller's destination cannot choose either.
    pub(super) fn join_graph_return_types(
        &mut self, left: TypeId, right: TypeId, span: Span,
    ) -> Result<TypeId, InferenceError> {
        let mut state = self.generic.borrow_mut();
        let graph = &mut state.facts.graph;
        let reason = graph.reason(span, None)?;
        let left_node = graph.node(graph.resolved(left)?)?.clone();
        let right_node = graph.node(graph.resolved(right)?)?.clone();
        let (TypeNode::Result(left_success, left_error), TypeNode::Result(right_success, right_error)) = (left_node, right_node) else {
            graph.unify(left, right, reason)?;
            return Ok(left);
        };
        graph.unify(left_success, right_success, reason)?;
        let error = graph.fresh(1, span)?;
        let requirement = graph.require_error_join(ErrorJoin { inputs: vec![left_error, right_error], result: error, bound: None }, reason)?;
        let result = graph.result(left_success, error)?;
        graph.solve()?;
        if let Some(owner) = self.current_generic {
            state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::sema::check::Checker;
    use crate::sema::inference::{Atom, RequirementTemplate, TypeNode};
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn dynamic_result_inferred_returns_join_native_errors_without_caller_context() {
        for (first, second) in [("test.ok(true)", "test.fail(\"failed\")"), ("test.fail(\"failed\")", "test.ok(true)")] {
            let source = format!("proc compare(value: Any) [error] {{\n  match value {{\n    _ is Map[Any] => return {first}\n    _ => return {second}\n  }}\n}}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let declaration = checked.solved.declarations.values().next().unwrap();
            let graph = &checked.solved.graph;
            let TypeNode::Arrow(signature) = graph.node(graph.resolved(declaration.signature).unwrap()).unwrap() else { panic!("expected callable") };
            let TypeNode::Result(success, error) = graph.node(graph.resolved(signature.result).unwrap()).unwrap() else { panic!("expected Result") };
            assert!(matches!(graph.node(graph.resolved(*success).unwrap()).unwrap(), TypeNode::Atom(Atom::Unit)));
            assert!(matches!(graph.node(graph.resolved(*error).unwrap()).unwrap(), TypeNode::Atom(Atom::Error)));
            assert!(declaration.source_requirements.iter().any(|requirement| matches!(graph.requirement_template(*requirement).unwrap(), RequirementTemplate::ErrorJoin { .. })));
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn dynamic_result_return_annotation_requires_original_validation() {
        for expression in ["Ok(value.message)", "match value { Ok(inner) => Ok(inner.message), _ => Ok(\"none\") }"] {
            let source = format!("pure message(value: Any) -> Result[Str] {{ {expression} }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")), "{source}: {:?}", checked.diagnostics);
        }
        let source = "pure message(value: Any) -> Result[Str] { Ok(value.message.require()?) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.schema_validations.len(), 1);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn dynamic_result_error_join_preserves_success_and_written_error_bounds() {
        for source in [
            "pure changed(value: Bool) { if value { Ok(1) } else { Ok(\"bad\") } }\n",
            "proc narrowed(value: Bool) [error] -> Result[Unit, AssertionError] { if value { return test.ok(true) } else { return test.fail(\"failed\") } }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| matches!(code, "check.type-mismatch" | "check.effect-violation" | "check.pure-effect"))), "{source}: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn dynamic_result_error_join_preserves_independent_environment_permission() {
        for permissions in ["env", ""] {
            let source = format!("proc checked(value: Any) [{permissions}] {{ let _ = env.get(\"SETTING\"); match value {{ _ is Map[Any] => return test.ok(true), _ => return test.fail(\"failed\") }} }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if permissions.is_empty() {
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{source}: {:?}", checked.diagnostics);
            } else {
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                checked.solved.validate().unwrap();
            }
        }
    }
}
