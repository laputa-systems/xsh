use super::types::Type;
use crate::source::Span;
use crate::syntax::node::BinaryOp;
pub(crate) use xsh_registry::signature::{ArithmeticDomain, ArithmeticOperator, ArithmeticResult, ArithmeticSignature};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SealedBinaryOperation<T> {
    pub(crate) operator: ArithmeticOperator,
    pub(crate) left: T,
    pub(crate) right: T,
    pub(crate) result: T,
    pub(crate) span: Span,
}

impl<T> SealedBinaryOperation<T> {
    pub(crate) fn types(&self) -> [&T; 3] { [&self.left, &self.right, &self.result] }
}

pub(crate) fn arithmetic_operator(operator: BinaryOp) -> Option<ArithmeticOperator> {
    match operator {
        BinaryOp::Add => Some(ArithmeticOperator::Add),
        BinaryOp::Sub => Some(ArithmeticOperator::Subtract),
        BinaryOp::Mul => Some(ArithmeticOperator::Multiply),
        BinaryOp::Div => Some(ArithmeticOperator::Divide),
        BinaryOp::Rem => Some(ArithmeticOperator::Remainder),
        _ => None,
    }
}

pub(crate) fn arithmetic_domain_accepts(domain: ArithmeticDomain, ty: &Type) -> bool {
    match (domain, ty.unvalidated()) {
        (ArithmeticDomain::Integer, Type::Int | Type::UInt)
        | (ArithmeticDomain::Int, Type::Int)
        | (ArithmeticDomain::Float, Type::Float)
        | (ArithmeticDomain::Str, Type::Str)
        | (ArithmeticDomain::Duration, Type::Duration)
        | (ArithmeticDomain::List, Type::List(_))
        | (ArithmeticDomain::Set, Type::Set(_)) => true,
        _ => false,
    }
}

pub(crate) fn arithmetic_selection(operator: ArithmeticOperator, left: &Type, right: &Type) -> Option<(ArithmeticSignature, Type)> {
    let signature = ArithmeticSignature::ALL.into_iter().find(|signature| signature.accepts_operator(operator) && {
        let (required_left, required_right) = signature.domains();
        arithmetic_domain_accepts(required_left, left) && arithmetic_domain_accepts(required_right, right)
    })?;
    match signature.result() {
        ArithmeticResult::ListConcatenation => {
            let (Type::List(left_item), Type::List(right_item)) = (left.unvalidated(), right.unvalidated()) else { return None };
            if !right_item.matches_stored(left_item) { return None; }
        }
        ArithmeticResult::SetDifference => {
            if !right.unvalidated().matches_expected(left.unvalidated()) { return None; }
        }
        _ => {}
    }
    Some((signature, arithmetic_result(signature, left, right)?))
}

pub(crate) fn arithmetic_result(signature: ArithmeticSignature, left: &Type, right: &Type) -> Option<Type> {
    Some(match signature.result() {
        ArithmeticResult::Int => Type::Int,
        ArithmeticResult::Float => Type::Float,
        ArithmeticResult::Str => Type::Str,
        ArithmeticResult::Duration => Type::Duration,
        ArithmeticResult::ListConcatenation => {
            let (Type::List(left_item), Type::List(right_item)) = (left.unvalidated(), right.unvalidated()) else { return None };
            let item = if **left_item == Type::Unknown { right_item.clone() } else { left_item.clone() };
            Type::concatenation_type([left.validated().map(|ty| ty.validation()), right.validated().map(|ty| ty.validation())], Type::List(item))
        }
        ArithmeticResult::SetDifference => {
            if !matches!(left.unvalidated(), Type::Set(_)) { return None; }
            left.unvalidated().clone()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_arithmetic_preserves_integer_representation_and_duration_dimensions() {
        for operator in [ArithmeticOperator::Add, ArithmeticOperator::Subtract, ArithmeticOperator::Multiply, ArithmeticOperator::Divide, ArithmeticOperator::Remainder] {
            for (left, right) in [(Type::Int, Type::Int), (Type::Int, Type::UInt), (Type::UInt, Type::Int), (Type::UInt, Type::UInt)] {
                assert_eq!(arithmetic_selection(operator, &left, &right), Some((ArithmeticSignature::Integer, Type::Int)));
            }
        }
        assert_eq!(arithmetic_selection(ArithmeticOperator::Divide, &Type::Duration, &Type::Duration), Some((ArithmeticSignature::DurationRatio, Type::Int)));
        assert_eq!(arithmetic_selection(ArithmeticOperator::Divide, &Type::Duration, &Type::Int), Some((ArithmeticSignature::DurationScale, Type::Duration)));
        assert!(arithmetic_selection(ArithmeticOperator::Multiply, &Type::Duration, &Type::UInt).is_none());
        assert!(arithmetic_selection(ArithmeticOperator::Multiply, &Type::UInt, &Type::Duration).is_none());
        assert!(arithmetic_selection(ArithmeticOperator::Divide, &Type::Duration, &Type::UInt).is_none());
        assert!(arithmetic_selection(ArithmeticOperator::Remainder, &Type::Float, &Type::Float).is_none());
    }

    #[test]
    fn canonical_concatenation_preserves_stored_items_and_nonempty_validation() {
        let signed = Type::List(Box::new(Type::Int));
        let unsigned = Type::List(Box::new(Type::UInt));
        assert!(arithmetic_selection(ArithmeticOperator::Add, &signed, &unsigned).is_none());
        assert!(arithmetic_selection(ArithmeticOperator::Add, &unsigned, &signed).is_none());
        let paths = Type::List(Box::new(Type::Path));
        let relative = Type::List(Box::new(Type::rel_path()));
        assert_eq!(arithmetic_selection(ArithmeticOperator::Add, &paths, &relative), Some((ArithmeticSignature::ListConcatenation, paths.clone())));
        assert!(arithmetic_selection(ArithmeticOperator::Add, &relative, &paths).is_none());
        let nonempty = Type::non_empty(Type::Int);
        for (left, right) in [(&nonempty, &signed), (&signed, &nonempty)] {
            assert_eq!(arithmetic_selection(ArithmeticOperator::Add, left, right), Some((ArithmeticSignature::ListConcatenation, nonempty.clone())));
        }
    }
}
