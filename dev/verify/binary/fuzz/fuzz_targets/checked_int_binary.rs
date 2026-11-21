#![no_main]
#![allow(dead_code)]

mod syntax {
    pub mod node {
        include!("../../../aeneas/slice/src/binary_op.rs");
    }
}

#[path = "../../../../../src/arith.rs"]
mod arith;

use arith::{IntArithmeticError, checked_int_binary};
use syntax::node::BinaryOp;

const OPERATORS: [BinaryOp; 18] = [
    BinaryOp::ResultFallback,
    BinaryOp::Or,
    BinaryOp::And,
    BinaryOp::Eq,
    BinaryOp::Ne,
    BinaryOp::Lt,
    BinaryOp::Le,
    BinaryOp::Gt,
    BinaryOp::Ge,
    BinaryOp::In,
    BinaryOp::NotIn,
    BinaryOp::Add,
    BinaryOp::Sub,
    BinaryOp::Mul,
    BinaryOp::Div,
    BinaryOp::Rem,
    BinaryOp::Union,
    BinaryOp::Intersect,
];

fn oracle(op: BinaryOp, left: i64, right: i64) -> Option<Result<i64, IntArithmeticError>> {
    let (a, b) = (i128::from(left), i128::from(right));
    let fits = |value: i128| i64::try_from(value).map_err(|_| IntArithmeticError::Overflow);
    match op {
        BinaryOp::Add => Some(fits(a + b)),
        BinaryOp::Sub => Some(fits(a - b)),
        BinaryOp::Mul => Some(fits(a * b)),
        BinaryOp::Div | BinaryOp::Rem if b == 0 => Some(Err(IntArithmeticError::DivisionByZero)),
        BinaryOp::Div => Some(fits(a / b)),
        BinaryOp::Rem => Some(fits(a / b).map(|_| (a % b) as i64)),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn LLVMFuzzerTestOneInput(data: *const u8, size: usize) -> i32 {
    if data.is_null() || size < 17 {
        return 0;
    }
    // SAFETY: the driver passes `size` readable bytes at `data`, and `data` is non-null.
    let bytes = unsafe { std::slice::from_raw_parts(data, size) };
    let op = OPERATORS[usize::from(bytes[0]) % OPERATORS.len()];
    let left = i64::from_le_bytes(bytes[1..9].try_into().unwrap_or([0; 8]));
    let right = i64::from_le_bytes(bytes[9..17].try_into().unwrap_or([0; 8]));
    if checked_int_binary(op, left, right) != oracle(op, left, right) {
        std::process::abort();
    }
    0
}
