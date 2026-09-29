//! Checked millisecond arithmetic shared by constant preparation and execution.

use crate::syntax::node::BinaryOp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DurationOperand { Millis(u64), Count(i64) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DurationResult { Millis(u64), Count(i64) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DurationArithmeticError { InvalidDimensions, Overflow, Underflow, NegativeFactor, NonpositiveDivisor, CountOverflow }

impl DurationArithmeticError {
    pub(crate) fn diagnostic(self) -> (&'static str, &'static str) {
        match self {
            Self::InvalidDimensions => ("type-error", "invalid Duration arithmetic dimensions"),
            Self::Overflow => ("duration-overflow", "Duration arithmetic exceeds representable milliseconds"),
            Self::Underflow => ("duration-underflow", "Duration subtraction would be negative"),
            Self::NegativeFactor => ("duration-negative-factor", "Duration multiplier must be nonnegative"),
            Self::NonpositiveDivisor => ("division-by-zero", "Duration divisor must be positive"),
            Self::CountOverflow => ("integer-overflow", "Duration interval count exceeds Int"),
        }
    }
}

/// Durations remain unsigned milliseconds; counts remain signed language Ints.
/// Division discards sub-millisecond remainders without converting through Float.
pub(crate) fn checked_duration_binary(op: BinaryOp, left: DurationOperand, right: DurationOperand) -> Result<DurationResult, DurationArithmeticError> {
    use DurationArithmeticError as E;
    use DurationOperand::{Millis, Count};
    Ok(match (op, left, right) {
        (BinaryOp::Add, Millis(a), Millis(b)) => DurationResult::Millis(a.checked_add(b).ok_or(E::Overflow)?),
        (BinaryOp::Sub, Millis(a), Millis(b)) => DurationResult::Millis(a.checked_sub(b).ok_or(E::Underflow)?),
        (BinaryOp::Mul, Millis(a), Count(b)) | (BinaryOp::Mul, Count(b), Millis(a)) => {
            let b = u64::try_from(b).map_err(|_| E::NegativeFactor)?;
            DurationResult::Millis(a.checked_mul(b).ok_or(E::Overflow)?)
        }
        (BinaryOp::Div, Millis(a), Count(b)) => {
            if b <= 0 { return Err(E::NonpositiveDivisor); }
            DurationResult::Millis(a / b as u64)
        }
        (BinaryOp::Div, Millis(a), Millis(b)) => {
            if b == 0 { return Err(E::NonpositiveDivisor); }
            DurationResult::Count(i64::try_from(a / b).map_err(|_| E::CountOverflow)?)
        }
        _ => return Err(E::InvalidDimensions),
    })
}
