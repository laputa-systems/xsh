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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MembershipDomain { List, Set, Map, UnsignedMap, Record, Str, Bytes, Path, EnvPath }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IndexSyntax { Ordinary, NegativeLiteral(std::num::NonZeroU32) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SliceBounds { Unbounded, Start, End, Both }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum TypeDomain { SortKey, CountKey, CollectItem, MinMaxItem, OrderingOperand, NegationOperand }

impl TypeDomain {
    pub(crate) fn may_accept(self, ty: &Type) -> bool {
        if self == Self::MinMaxItem { return !ty.contains_recovery() && ty != &Type::Any; }
        match ty.unvalidated() {
            Type::Inference(_) => true,
            Type::Record(fields) if self == Self::SortKey => fields.values().all(|ty| self.may_accept(ty)),
            ty => self.accepts(ty),
        }
    }

    pub(crate) fn accepts(self, ty: &Type) -> bool {
        if ty.contains_recovery() { return false; }
        if self == Self::MinMaxItem { return !ty.contains_inference() && ty != &Type::Any; }
        match (self, ty.unvalidated()) {
            (Self::CollectItem, ty) => !matches!(ty, Type::Stream(_) | Type::Inference(_)),
            (Self::OrderingOperand, ty) => ty.can_be_ordering_operand(),
            (Self::NegationOperand, ty) => matches!(ty, Type::Int | Type::Float),
            (Self::SortKey, Type::Int | Type::Str | Type::Bool | Type::Path)
            | (Self::CountKey, Type::Int | Type::UInt | Type::Str | Type::Bool) => true,
            (Self::SortKey, Type::Record(fields)) => fields.values().all(|ty| self.accepts(ty)),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordOverrideField<T> {
    pub(crate) name: crate::symbol::Name,
    pub(crate) ty: T,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SealedOperation<T> {
    TypePredicate { domain: TypeDomain, subject: T, span: Span },
    Binary(SealedBinaryOperation<T>),
    Index { receiver: T, key: T, result: T, syntax: IndexSyntax, constant_key: Option<crate::symbol::Name>, span: Span },
    Slice { receiver: T, result: T, bounds: SliceBounds, span: Span },
    RecordOverride { receiver: T, overrides: std::sync::Arc<[RecordOverrideField<T>]>, result: T, span: Span },
    Membership { needle: T, container: T, span: Span },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SealedOperationSignature {
    TypePredicate(TypeDomain),
    Binary(ArithmeticSignature),
    IndexList,
    IndexMap,
    IndexRecord,
    IndexModule,
    SliceList,
    SliceStr,
    SliceBytes,
    RecordOverride,
    Membership(MembershipDomain),
}

impl<T> From<SealedBinaryOperation<T>> for SealedOperation<T> {
    fn from(operation: SealedBinaryOperation<T>) -> Self { Self::Binary(operation) }
}

impl<T> SealedOperation<T> {
    pub(crate) fn span(&self) -> Span {
        match self { Self::Binary(operation) => operation.span, Self::Membership { span, .. } | Self::TypePredicate { span, .. } | Self::Index { span, .. } | Self::Slice { span, .. } | Self::RecordOverride { span, .. } => *span }
    }
    pub(crate) fn result(&self) -> Option<&T> {
        match self { Self::TypePredicate { .. } | Self::Membership { .. } => None, Self::Binary(operation) => Some(&operation.result), Self::Index { result, .. } | Self::Slice { result, .. } | Self::RecordOverride { result, .. } => Some(result) }
    }
    pub(crate) fn inputs(&self) -> impl Iterator<Item = &T> {
        let (fixed, overrides): (_, &[RecordOverrideField<T>]) = match self {
            Self::TypePredicate { subject: key, .. } => ([Some(key), None], &[]),
            Self::Binary(operation) => ([Some(&operation.left), Some(&operation.right)], &[]),
            Self::Membership { needle, container, .. } => ([Some(needle), Some(container)], &[]),
            Self::Index { receiver, key, .. } => ([Some(receiver), Some(key)], &[]),
            Self::Slice { receiver, .. } => ([Some(receiver), None], &[]),
            Self::RecordOverride { receiver, overrides, .. } => ([Some(receiver), None], overrides),
        };
        fixed.into_iter().flatten().chain(overrides.iter().map(|field| &field.ty))
    }
    pub(crate) fn key_operands(&self) -> Option<(&T, &T)> {
        match self {
            Self::Index { receiver, key, .. } => Some((receiver, key)),
            Self::Membership { container, needle, .. } => Some((container, needle)),
            _ => None,
        }
    }
    pub(crate) fn types(&self) -> impl Iterator<Item = &T> { self.inputs().chain(self.result()) }
    pub(crate) fn types_mut(&mut self) -> impl Iterator<Item = &mut T> where T: Clone {
        let (fixed, overrides): (_, &mut [RecordOverrideField<T>]) = match self {
            Self::TypePredicate { subject: key, .. } => ([Some(key), None, None], &mut []),
            Self::Binary(operation) => ([Some(&mut operation.left), Some(&mut operation.right), Some(&mut operation.result)], &mut []),
            Self::Membership { needle, container, .. } => ([Some(needle), Some(container), None], &mut []),
            Self::Index { receiver, key, result, .. } => ([Some(receiver), Some(key), Some(result)], &mut []),
            Self::Slice { receiver, result, .. } => ([Some(receiver), Some(result), None], &mut []),
            Self::RecordOverride { receiver, overrides, result, .. } => ([Some(receiver), Some(result), None], std::sync::Arc::make_mut(overrides)),
        };
        fixed.into_iter().flatten().chain(overrides.iter_mut().map(|field| &mut field.ty))
    }
    pub(crate) fn same_kind<U>(&self, other: &SealedOperation<U>) -> bool {
        match (self, other) {
            (Self::TypePredicate { domain, .. }, SealedOperation::TypePredicate { domain: right, .. }) => domain == right,
            (Self::Binary(left), SealedOperation::Binary(right)) => left.operator == right.operator,
            (Self::Membership { .. }, SealedOperation::Membership { .. }) => true,
            (Self::Index { syntax, constant_key, .. }, SealedOperation::Index { syntax: right_syntax, constant_key: right_key, .. }) => syntax == right_syntax && constant_key == right_key,
            (Self::Slice { bounds, .. }, SealedOperation::Slice { bounds: right, .. }) => bounds == right,
            (Self::RecordOverride { overrides, .. }, SealedOperation::RecordOverride { overrides: right, .. }) => overrides.len() == right.len() && overrides.iter().zip(right.iter()).all(|(left, right)| left.name == right.name),
            _ => false,
        }
    }
    pub(crate) fn owned_fields_bytes(&self) -> usize {
        match self {
            Self::RecordOverride { overrides, .. } => 2 * std::mem::size_of::<usize>() + overrides.len() * std::mem::size_of::<RecordOverrideField<T>>(),
            _ => 0,
        }
    }
    pub(crate) fn try_map<U, E>(&self, mut map: impl FnMut(&T) -> Result<U, E>) -> Result<SealedOperation<U>, E> {
        Ok(match self {
            Self::TypePredicate { domain, subject: key, span } => SealedOperation::TypePredicate { domain: *domain, subject: map(key)?, span: *span },
            Self::Binary(operation) => SealedOperation::Binary(SealedBinaryOperation { operator: operation.operator, left: map(&operation.left)?, right: map(&operation.right)?, result: map(&operation.result)?, span: operation.span }),
            Self::Membership { needle, container, span } => SealedOperation::Membership { needle: map(needle)?, container: map(container)?, span: *span },
            Self::Index { receiver, key, result, syntax, constant_key, span } => SealedOperation::Index { receiver: map(receiver)?, key: map(key)?, result: map(result)?, syntax: *syntax, constant_key: *constant_key, span: *span },
            Self::Slice { receiver, result, bounds, span } => SealedOperation::Slice { receiver: map(receiver)?, result: map(result)?, bounds: *bounds, span: *span },
            Self::RecordOverride { receiver, overrides, result, span } => SealedOperation::RecordOverride { receiver: map(receiver)?, overrides: overrides.iter().map(|field| Ok(RecordOverrideField { name: field.name, ty: map(&field.ty)? })).collect::<Result<Vec<_>, E>>()?.into(), result: map(result)?, span: *span },
        })
    }
}

pub(crate) fn record_override_selection(receiver: &Type, overrides: &[RecordOverrideField<Type>]) -> Option<(SealedOperationSignature, Type)> {
    let Type::Record(fields) = receiver.unvalidated() else { return None };
    let mut fields = fields.as_ref().clone();
    for field in overrides { fields.insert(field.name, field.ty.clone()); }
    Some((SealedOperationSignature::RecordOverride, Type::Record(fields.into())))
}

pub(crate) fn index_selection(receiver: &Type, key: &Type, constant_key: Option<crate::symbol::Name>) -> Option<(SealedOperationSignature, Type)> {
    Some(match receiver.unvalidated() {
        Type::List(item) if key.matches_expected(&Type::Int) => (SealedOperationSignature::IndexList, (**item).clone()),
        Type::Map(required, item) if key.matches_expected(required) || requires_uint_key_validation(receiver, key) => (SealedOperationSignature::IndexMap, (**item).clone()),
        Type::Record(fields) if key.matches_expected(&Type::Str) => (SealedOperationSignature::IndexRecord, fields.get(&constant_key?)?.clone()),
        Type::Module(module) if key.matches_expected(&Type::Str) => (SealedOperationSignature::IndexModule, module.exports.get(&constant_key?)?.field_value_type()?),
        _ => return None,
    })
}

pub(crate) fn requires_uint_key_validation(receiver: &Type, key: &Type) -> bool {
    matches!(receiver.unvalidated(), Type::Map(required, _) if required.unvalidated() == &Type::UInt)
        && key.unvalidated() == &Type::Int
}

pub(crate) fn may_require_uint_key_validation(receiver: &Type, key: &Type) -> bool {
    let receiver_can_be_unsigned_map = match receiver.unvalidated() {
        Type::Inference(_) => true,
        Type::Map(required, _) if required.as_ref() == key => false,
        Type::Map(required, _) => matches!(required.unvalidated(), Type::UInt | Type::Inference(_)),
        _ => false,
    };
    let key_can_be_unvalidated_int = matches!(key.unvalidated(), Type::Int | Type::Inference(_));
    receiver_can_be_unsigned_map && key_can_be_unvalidated_int
}

pub(crate) fn result_error_contract(ty: &Type) -> Option<&Type> {
    match ty.unvalidated() {
        Type::Optional(inner) => result_error_contract(inner),
        Type::Result(_, error) => Some(error),
        _ => None,
    }
}

pub(crate) fn slice_selection(receiver: &Type) -> Option<(SealedOperationSignature, Type)> {
    Some(match receiver.unvalidated() {
        Type::List(item) => (SealedOperationSignature::SliceList, Type::List(item.clone())),
        Type::Str => (SealedOperationSignature::SliceStr, Type::Str),
        Type::Bytes => (SealedOperationSignature::SliceBytes, Type::Bytes),
        _ => return None,
    })
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
    matches!((domain, ty.unvalidated()),
        (ArithmeticDomain::Integer, Type::Int | Type::UInt)
        | (ArithmeticDomain::Int, Type::Int)
        | (ArithmeticDomain::Float, Type::Float)
        | (ArithmeticDomain::Str, Type::Str)
        | (ArithmeticDomain::Duration, Type::Duration)
        | (ArithmeticDomain::List, Type::List(_))
        | (ArithmeticDomain::Set, Type::Set(_)))
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
        ArithmeticResult::SetDifference if !right.unvalidated().matches_expected(left.unvalidated()) => return None,
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

pub(crate) fn membership_selection(container: &Type, needle: &Type) -> Option<MembershipDomain> {
    use super::types::MembershipCheck;
    if container.contains_inference() || needle.contains_inference() || container.contains_recovery() || needle.contains_recovery() { return None; }
    match container.membership_needle_type(needle) {
        MembershipCheck::Expected(required) | MembershipCheck::Compared(required) if needle.matches_expected(required) => {},
        _ => return None,
    }
    membership_domain(container)
}

pub(crate) fn membership_domain(container: &Type) -> Option<MembershipDomain> {
    Some(match container.unvalidated() {
        Type::List(_) => MembershipDomain::List,
        Type::Set(item) if item.is_map_key() => MembershipDomain::Set,
        Type::Map(key, _) if key.is_map_key() => if **key == Type::UInt { MembershipDomain::UnsignedMap } else { MembershipDomain::Map },
        Type::Record(_) | Type::ErasedRecord => MembershipDomain::Record,
        Type::Str => MembershipDomain::Str,
        Type::Bytes => MembershipDomain::Bytes,
        Type::Path => MembershipDomain::Path,
        Type::EnvPathList => MembershipDomain::EnvPath,
        _ => return None,
    })
}
