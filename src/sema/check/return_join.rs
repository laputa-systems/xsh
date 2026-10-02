use super::{Checker, Span, Type};

impl Checker {
    pub(super) fn unify_inferred_returns(&mut self, left: Type, right: Type, span: Span) -> Type {
        if let Some(joined) = self.join_graph_callable_values(&left, &right, span) { return joined; }
        if self.current_generic.is_some() {
            let outcome = (|| {
                let left = self.graph_type(&left, span)?;
                let right = self.graph_type(&right, span)?;
                let mut state = self.generic.borrow_mut();
                let reason = state.facts.graph.reason(span, None)?;
                use crate::sema::inference::{Atom, TypeNode};
                let graph = &mut state.facts.graph;
                let left_node = graph.node(graph.resolved(left)?)?.clone();
                let right_node = graph.node(graph.resolved(right)?)?.clone();
                if matches!((&left_node, &right_node), (TypeNode::Result { .. }, TypeNode::Result { .. })) {
                    drop(state);
                    return self.join_graph_return_types(left, right, span);
                }
                // Absence belongs to the joined value, not to the other operand's payload type.
                let joined = match (left_node, right_node) {
                    (TypeNode::Atom(Atom::UInt), TypeNode::Atom(Atom::Int))
                    | (TypeNode::Atom(Atom::Int), TypeNode::Atom(Atom::UInt)) => {
                        // Scalar branch results retain their established checked domain.
                        graph.assignable(left, right, reason)?;
                        left
                    }
                    (TypeNode::Atom(Atom::Null), TypeNode::Atom(Atom::Null)) => left,
                    (TypeNode::Atom(Atom::Null), TypeNode::Optional(_)) => right,
                    (TypeNode::Optional(_), TypeNode::Atom(Atom::Null)) => left,
                    (TypeNode::Atom(Atom::Null), _) => graph.optional(right)?,
                    (_, TypeNode::Atom(Atom::Null)) => graph.optional(left)?,
                    (TypeNode::Optional(left_item), TypeNode::Optional(right_item)) => { graph.unify(left_item, right_item, reason)?; left }
                    (TypeNode::Optional(item), _) => { graph.unify(item, right, reason)?; left }
                    (_, TypeNode::Optional(item)) => { graph.unify(left, item, reason)?; right }
                    _ => { graph.unify(left, right, reason)?; left }
                };
                Ok::<_, crate::sema::inference::InferenceError>(joined)
            })();
            return match outcome {
                Ok(ty) => self.graph_view(ty),
                Err(error) => { self.graph_error(span, error); Type::Invalid }
            };
        }
        let joined = if matches!((&left, &right), (Type::Int, Type::UInt) | (Type::UInt, Type::Int)) {
            Some(left.clone())
        } else { unify_return_shapes(&left, &right) };
        match joined {
            Some(ty) => ty,
            None => {
                self.error(span, &format!("incompatible inferred return paths `{left}` and `{right}`; declare a return type"), "check.infer-return");
                Type::Invalid
            }
        }
    }
}

fn unify_return_shapes(left: &Type, right: &Type) -> Option<Type> {
    if left == right { return Some(left.clone()); }
    match (left, right) {
        (Type::Unknown, other) | (other, Type::Unknown) => Some(other.clone()),
        (Type::Null, Type::Optional(inner)) | (Type::Optional(inner), Type::Null) => Some(Type::Optional(inner.clone())),
        (Type::Null, other) | (other, Type::Null) => Some(Type::Optional(Box::new(other.clone()))),
        (Type::Error, Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError)
        | (Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError, Type::Error) => Some(Type::Error),
        (Type::Optional(inner), other) | (other, Type::Optional(inner)) if !matches!(other, Type::Optional(_)) =>
            Some(Type::Optional(Box::new(unify_return_shapes(inner, other)?))),
        (Type::Record(left), Type::Record(right)) if left.keys().eq(right.keys()) => {
            Some(Type::Record(left.iter().map(|(name, ty)| Some((*name, unify_return_shapes(ty, &right[name])?))).collect::<Option<_>>()?))
        }
        (Type::List(left), Type::List(right)) => Some(Type::List(Box::new(unify_return_shapes(left, right)?))),
        (Type::Map(lk, left), Type::Map(rk, right)) => Some(Type::Map(Box::new(unify_return_shapes(lk, rk)?), Box::new(unify_return_shapes(left, right)?))),
        (Type::Optional(left), Type::Optional(right)) => Some(Type::Optional(Box::new(unify_return_shapes(left, right)?))),
        (Type::Result(left, le), Type::Result(right, re)) => Some(Type::Result(Box::new(unify_return_shapes(left, right)?), Box::new(unify_return_shapes(le, re)?))),
        _ => None,
    }
}
