//! Integer arithmetic routes.
//!
//! `int_binary_rows_admit_only_the_arithmetic_operators` is a verifier
//! corruption test (XSH-155): an `IntBinary` row's operator must be one of the
//! five that `crate::arith::checked_int_binary` decides.

use super::noncompletion_tests::{checked_program, function_index};
use super::tests::run_with_large_stack;
use super::*;
use crate::runtime::value::RuntimeError;

const ALL_BINARY_OPS: [BinaryOp; 18] = [
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

fn row_op(program: &FullProgram, instruction: usize) -> BinaryOp {
    let start = program.store.data[instruction].range().start as usize;
    program.store.binary_ops[program.store.extra[start] as usize]
}

fn row_pool(program: &FullProgram, instruction: usize) -> usize {
    program.store.extra[program.store.data[instruction].range().start as usize] as usize
}

fn rows(program: &FullProgram, owner: usize, tag: FullTag) -> Vec<usize> {
    program
        .store
        .function_instruction_range(owner)
        .unwrap()
        .filter(|&instruction| program.store.tags[instruction] == tag)
        .collect()
}

fn run_installed(program: &Arc<FullProgram>, force_frames: bool) -> Result<(), RuntimeError> {
    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
    evaluator.force_frames = force_frames;
    evaluator.indexed_program = Some(Arc::clone(program));
    for step in 0..program.driver_step_count().unwrap() {
        evaluator
            .eval_indexed_driver_step(step, Span::at(program.store.source_id, 0))
            .expect("indexed driver step")?;
    }
    Ok(())
}

/// XSH-155: the typed in-place route expects `checked_int_binary` to decide the
/// row, and the frame route would compute a comparison in an `Int` position, so
/// verification must reject every other operator in an `IntBinary` row, wherever
/// the row sits.
#[test]
fn int_binary_rows_admit_only_the_arithmetic_operators() {
    run_with_large_stack(|| {
        let source = "pure apply(left: Int, right: Int) -> Int {\n  let result = left + right\n  return result\n}\n\
pure nested(left: Int, right: Int) -> Int {\n  let result = (left - right) * 2\n  return result\n}\n\
pure compared(left: Int, right: Int) -> Int {\n  if left / right > 2 {\n    return 1\n  }\n  0\n}\n\
assert apply(7, 2) == 9\nassert nested(7, 2) == 10\nassert compared(7, 2) == 1\n";
        let program = Arc::new(checked_program("int-binary-operators.xsh", source));
        FullVerifier::verify(&program).unwrap();
        for force_frames in [false, true] {
            run_installed(&program, force_frames).unwrap();
        }
        let mut forged_rows = Vec::new();
        for (function, expected) in [
            ("apply", &[BinaryOp::Add][..]),
            ("nested", &[BinaryOp::Sub, BinaryOp::Mul][..]),
            ("compared", &[BinaryOp::Div][..]),
        ] {
            let owner = function_index(&program, function);
            let binary = rows(&program, owner, FullTag::IntBinary);
            let mut ops: Vec<_> = binary.iter().map(|&row| row_op(&program, row)).collect();
            ops.sort_by_key(|op| *op as u8);
            assert_eq!(ops, expected, "{function} lowers its arithmetic to IntBinary rows");
            forged_rows.extend(binary);
        }
        let pools: std::collections::BTreeSet<_> = forged_rows.iter().map(|&row| row_pool(&program, row)).collect();
        assert_eq!(pools.len(), forged_rows.len(), "each IntBinary row has its own operator entry");
        for &row in &forged_rows {
            for op in ALL_BINARY_OPS {
                let mut forged = (*program).clone();
                forged.store.binary_ops[row_pool(&program, row)] = op;
                let verified = FullVerifier::verify(&forged);
                if crate::arith::checked_int_binary(op, 7, 2).is_some() {
                    verified.unwrap_or_else(|error| panic!("{op:?} in row {row}: {error:?}"));
                } else {
                    assert_eq!(
                        verified.expect_err(&format!("{op:?} in row {row}")).message,
                        "integer binary row has a non-arithmetic operator",
                    );
                }
            }
        }
    });
}
