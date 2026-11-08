use super::{CallableApplicationScope, ConstraintResolutionError, MAX_TYPE_DEPTH, SealedOperationError, Type, TypeConstraints, Variable};
use crate::sema::check::EffectDeclarationId;
use crate::source::Span;
use std::collections::BTreeSet;

impl TypeConstraints {
    pub(crate) fn contextualize_equality_holes(
        &mut self,
        owner: CallableApplicationScope,
        component: &BTreeSet<EffectDeclarationId>,
        inputs: &[Type],
        holes: &[(Type, Span)],
        actual: &Type,
        context: &Type,
        span: Span,
    ) -> Result<(), SealedOperationError> {
        if context.contains_inference() || context.contains_any() || context.contains_recovery() || actual.contains_any() || actual.contains_recovery() { return Ok(()); }
        self.commit_constraint_transaction(|this| {
            let supported = match owner {
                CallableApplicationScope::Declaration(owner) => this.input_bound_variables(owner, component, inputs)?,
                CallableApplicationScope::Invocation => this.unresolved_input_variables(inputs)?,
                CallableApplicationScope::Guard(_) => return Err(ConstraintResolutionError::ForeignVariable.into()),
            };
            let mut fresh = BTreeSet::new();
            for (hole, producer) in holes {
                let Type::Inference(variable) = hole else { continue; };
                if !supported.contains_type(*variable)
                    && matches!(this.variables.get(variable), Some(Variable::Representative(value)) if value.binding.is_none() && value.rank == 0 && value.origin == *producer)
                { fresh.insert(*variable); }
            }
            let mut pending = vec![(this.resolve(actual)?, context.clone(), 0)];
            while let Some((actual, context, depth)) = pending.pop() {
                if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth.into()); }
                match (actual, context) {
                    (Type::Inference(variable), context) if fresh.contains(&variable) => {
                        this.constrain_context(&Type::Inference(variable), &context, span).map_err(SealedOperationError::Constraint)?;
                    }
                    (Type::Optional(actual), Type::Optional(context))
                    | (Type::List(actual), Type::List(context))
                    | (Type::Set(actual), Type::Set(context))
                    | (Type::Stream(actual), Type::Stream(context)) => pending.push((*actual, *context, depth + 1)),
                    (Type::Optional(actual), context) => pending.push((*actual, context, depth + 1)),
                    (actual, Type::Optional(context)) => pending.push((actual, *context, depth + 1)),
                    (Type::Result(actual_ok, actual_error), Type::Result(context_ok, context_error))
                    | (Type::Map(actual_ok, actual_error), Type::Map(context_ok, context_error)) => {
                        pending.push((*actual_ok, *context_ok, depth + 1));
                        pending.push((*actual_error, *context_error, depth + 1));
                    }
                    (Type::Record(actual), Type::Record(context)) => {
                        for (name, actual) in actual.iter() {
                            if let Some(context) = context.get(name) { pending.push((actual.clone(), context.clone(), depth + 1)); }
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn span(start: usize) -> Span { Span::new(SourceId::new(0), start, start + 1) }

    #[test]
    fn constructor_equality_context_preserves_header_roots_and_rejects_forged_origins() {
        let mut constraints = TypeConstraints::default();
        let header = constraints.fresh(span(1));
        let hole = constraints.fresh(span(2));
        let actual = Type::Result(Box::new(hole.clone()), Box::new(header.clone()));
        let expected = Type::Result(Box::new(Type::Int), Box::new(Type::Str));
        constraints.contextualize_equality_holes(CallableApplicationScope::Invocation, &BTreeSet::new(), std::slice::from_ref(&header), &[(hole.clone(), span(3))], &actual, &expected, span(4)).unwrap();
        assert_eq!(constraints.resolve(&hole).unwrap(), hole);
        for forbidden in [Type::Any, Type::Unknown, Type::Invalid] {
            let context = Type::Result(Box::new(forbidden), Box::new(Type::Str));
            constraints.contextualize_equality_holes(CallableApplicationScope::Invocation, &BTreeSet::new(), &[], &[(hole.clone(), span(2))], &actual, &context, span(4)).unwrap();
            assert_eq!(constraints.resolve(&hole).unwrap(), hole);
        }
        constraints.contextualize_equality_holes(CallableApplicationScope::Invocation, &BTreeSet::new(), std::slice::from_ref(&header), &[(hole.clone(), span(2))], &actual, &expected, span(4)).unwrap();
        assert_eq!(constraints.resolve(&hole).unwrap(), Type::Int);
        assert_eq!(constraints.resolve(&header).unwrap(), header);
    }

    #[test]
    fn constructor_equality_context_excludes_wrapper_results_supported_by_inputs() {
        let mut constraints = TypeConstraints::default();
        let owner = EffectDeclarationId { namespace: None, body: span(0) };
        let scope = CallableApplicationScope::Declaration(owner);
        let header = constraints.fresh(span(1));
        let hole = constraints.fresh(span(2));
        constraints.register_source_wrapper(scope, crate::sema::wrapper_operation::WrapperRequirement {
            kind: crate::sema::wrapper_operation::WrapperRequirementKind::Fallback { source: header.clone(), fallback: Type::Str, result: hole.clone() },
            span: span(3),
        }).unwrap();
        let actual = Type::Result(Box::new(hole.clone()), Box::new(Type::Str));
        let expected = Type::Result(Box::new(Type::Int), Box::new(Type::Str));
        constraints.contextualize_equality_holes(scope, &BTreeSet::from([owner]), std::slice::from_ref(&header), &[(hole.clone(), span(2))], &actual, &expected, span(4)).unwrap();
        assert_eq!(constraints.resolve(&hole).unwrap(), hole);
        assert_eq!(constraints.resolve(&header).unwrap(), header);
    }

    #[test]
    fn conflicting_constructor_equality_projections_roll_back() {
        let symbols = crate::symbol::SymbolOwner::new();
        symbols.with_current(|| {
            let mut constraints = TypeConstraints::default();
            let hole = constraints.fresh(span(1));
            let one = crate::symbol::Name::intern("one");
            let two = crate::symbol::Name::intern("two");
            let actual = Type::Record(std::collections::BTreeMap::from([(one, hole.clone()), (two, hole.clone())]).into());
            let expected = Type::Record(std::collections::BTreeMap::from([(one, Type::Int), (two, Type::Str)]).into());
            let before = constraints.clone();
            let rejected = constraints.contextualize_equality_holes(CallableApplicationScope::Invocation, &BTreeSet::new(), &[], &[(hole.clone(), span(1))], &actual, &expected, span(4));
            assert!(matches!(rejected, Err(SealedOperationError::Constraint(_))));
            assert_eq!(constraints.resolve(&hole).unwrap(), hole);
            assert_eq!(constraints.operations, before.operations);
            let valid = Type::Record(std::collections::BTreeMap::from([(one, Type::Int), (two, Type::Int)]).into());
            constraints.contextualize_equality_holes(CallableApplicationScope::Invocation, &BTreeSet::new(), &[], &[(hole.clone(), span(1))], &actual, &valid, span(4)).unwrap();
            assert_eq!(constraints.resolve(&hole).unwrap(), Type::Int);
        });
    }
}
