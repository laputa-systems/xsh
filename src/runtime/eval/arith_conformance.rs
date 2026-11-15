use crate::arith::{IntArithmeticError, checked_int_binary};
use crate::syntax::node::BinaryOp;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};

const HEADER: &str = "int-arith-v1";
const VALUES: [i64; 12] = [i64::MIN, i64::MIN + 1, -7, -2, -1, 0, 1, 2, 3, 7, i64::MAX - 1, i64::MAX];
const OPS: [(&str, BinaryOp); 5] = [
    ("add", BinaryOp::Add),
    ("sub", BinaryOp::Sub),
    ("mul", BinaryOp::Mul),
    ("div", BinaryOp::Div),
    ("rem", BinaryOp::Rem),
];
const ROWS: usize = OPS.len() * VALUES.len() * VALUES.len();

fn number(text: &str) -> i64 {
    let digits = text.strip_prefix('-').unwrap_or(text);
    assert!(!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()), "malformed row: number {text:?}");
    assert!((digits == "0" || !digits.starts_with('0')) && text != "-0", "malformed row: noncanonical number {text}");
    text.parse().expect("malformed row: number out of range")
}

fn outcome(result: Result<i64, IntArithmeticError>) -> String {
    match result {
        Ok(value) => value.to_string(),
        Err(IntArithmeticError::DivisionByZero) => "division-by-zero".to_owned(),
        Err(IntArithmeticError::Overflow) => "overflow".to_owned(),
    }
}

#[test]
#[ignore = "manual Lean-generated production/model differential check"]
fn production_matches_lean_int_arithmetic() {
    let path = std::env::var_os("XSH_ARITH_CASES").expect("XSH_ARITH_CASES is required");
    let file = std::fs::File::open(path).expect("cannot open arithmetic corpus");
    let mut lines = BufReader::new(file).lines();
    assert_eq!(lines.next().expect("missing header").expect("invalid header"), HEADER);
    let mut seen = BTreeSet::new();
    for line in lines {
        let line = line.expect("invalid corpus input");
        assert!(seen.len() < ROWS, "corpus exceeds bound");
        let fields: Vec<_> = line.split(' ').collect();
        assert_eq!(fields.len(), 4, "malformed row: {line}");
        let (_, op) = *OPS.iter().find(|(name, _)| *name == fields[0]).expect("out-of-domain input: operator");
        let left = number(fields[1]);
        let right = number(fields[2]);
        assert!(VALUES.contains(&left) && VALUES.contains(&right), "out-of-domain input: {line}");
        assert!(seen.insert((fields[0].to_owned(), left, right)), "duplicate input: {line}");
        let actual = checked_int_binary(op, left, right).expect("arithmetic operator");
        assert_eq!(outcome(actual), fields[3], "model/source disagreement: {line}");
    }
    assert_eq!(seen.len(), ROWS, "incomplete corpus");
}
