use crate::arith::{IntArithmeticError as IntError, checked_int_binary};
use crate::duration::{
    DurationArithmeticError as DurationError, DurationOperand::{Count, Millis},
    DurationResult, checked_duration_binary,
};
use crate::syntax::node::BinaryOp;

#[test]
fn int_branches_agree_with_the_lean_witnesses() {
    let cases = [
        (BinaryOp::Add, 2, 3, Some(Ok(5))),
        (BinaryOp::Div, -7, 2, Some(Ok(-3))),
        (BinaryOp::Rem, -7, 2, Some(Ok(-1))),
        (BinaryOp::Div, 7, -2, Some(Ok(-3))),
        (BinaryOp::Rem, 7, -2, Some(Ok(1))),
        (BinaryOp::Div, 7, 0, Some(Err(IntError::DivisionByZero))),
        (BinaryOp::Rem, 7, 0, Some(Err(IntError::DivisionByZero))),
        (BinaryOp::Add, i64::MAX, 1, Some(Err(IntError::Overflow))),
        (BinaryOp::Sub, i64::MIN, 1, Some(Err(IntError::Overflow))),
        (BinaryOp::Mul, i64::MAX, 2, Some(Err(IntError::Overflow))),
        (BinaryOp::Div, i64::MIN, -1, Some(Err(IntError::Overflow))),
        (BinaryOp::Rem, i64::MIN, -1, Some(Err(IntError::Overflow))),
        (BinaryOp::Lt, 1, 2, None),
    ];
    for (op, left, right, expected) in cases {
        assert_eq!(checked_int_binary(op, left, right), expected, "{op:?} {left} {right}");
    }
}

#[test]
fn core_checked_remainder_rejects_the_minimum_over_negative_one() {
    assert_eq!(i64::MIN.checked_rem(-1), None);
    assert_eq!(i64::MIN.checked_div(-1), None);
}

#[test]
fn duration_branches_agree_with_the_lean_witnesses() {
    let cases = [
        (BinaryOp::Add, Millis(250), Millis(1000), Ok(DurationResult::Millis(1250))),
        (BinaryOp::Mul, Count(3), Millis(250), Ok(DurationResult::Millis(750))),
        (BinaryOp::Div, Millis(5), Count(2), Ok(DurationResult::Millis(2))),
        (BinaryOp::Div, Millis(7000), Millis(2000), Ok(DurationResult::Count(3))),
        (BinaryOp::Sub, Millis(0), Millis(1), Err(DurationError::Underflow)),
        (BinaryOp::Add, Millis(u64::MAX), Millis(1), Err(DurationError::Overflow)),
        (BinaryOp::Mul, Millis(u64::MAX), Count(2), Err(DurationError::Overflow)),
        (BinaryOp::Mul, Millis(1), Count(-1), Err(DurationError::NegativeFactor)),
        (BinaryOp::Mul, Count(-1), Millis(0), Err(DurationError::NegativeFactor)),
        (BinaryOp::Div, Millis(1), Count(0), Err(DurationError::NonpositiveDivisor)),
        (BinaryOp::Div, Millis(1), Count(-1), Err(DurationError::NonpositiveDivisor)),
        (BinaryOp::Div, Millis(1), Millis(0), Err(DurationError::NonpositiveDivisor)),
        (BinaryOp::Div, Millis(u64::MAX), Millis(1), Err(DurationError::CountOverflow)),
        (BinaryOp::Rem, Millis(7), Millis(2), Err(DurationError::InvalidDimensions)),
        (BinaryOp::Add, Millis(1), Count(1), Err(DurationError::InvalidDimensions)),
        (BinaryOp::Div, Count(1), Millis(1), Err(DurationError::InvalidDimensions)),
    ];
    for (op, left, right, expected) in cases {
        assert_eq!(checked_duration_binary(op, left, right), expected, "{op:?} {left:?} {right:?}");
    }
}
