//! Checked Int arithmetic shared by constant preparation and execution.

use crate::syntax::node::BinaryOp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IntArithmeticError {
    DivisionByZero,
    Overflow,
}

impl IntArithmeticError {
    pub(crate) fn diagnostic(self) -> (&'static str, &'static str) {
        match self {
            Self::DivisionByZero => ("division-by-zero", "division by zero"),
            Self::Overflow => ("integer-overflow", "integer overflow"),
        }
    }
}

/// Division and remainder truncate toward zero; a remainder exists only when its quotient fits.
pub(crate) fn checked_int_binary(op: BinaryOp, left: i64, right: i64) -> Option<Result<i64, IntArithmeticError>> {
    let result = match op {
        BinaryOp::Add => left.checked_add(right),
        BinaryOp::Sub => left.checked_sub(right),
        BinaryOp::Mul => left.checked_mul(right),
        BinaryOp::Div | BinaryOp::Rem if right == 0 => return Some(Err(IntArithmeticError::DivisionByZero)),
        BinaryOp::Div => left.checked_div(right),
        BinaryOp::Rem => left.checked_rem(right),
        _ => return None,
    };
    Some(result.ok_or(IntArithmeticError::Overflow))
}
