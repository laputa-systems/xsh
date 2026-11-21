//! Integer and Duration arithmetic routes.
//!
//! `int_binary_rows_admit_only_the_arithmetic_operators` (XSH-155) and
//! `comparison_rows_admit_only_their_comparison_operators` (XSH-159) are
//! verifier corruption tests: a row whose payload carries a `BinaryOp` must
//! hold one that execution decides for that row family.
//!
//! The `*_route_conformance` tests are finite Rust conformance, not proofs.
//! Each takes one source form of every arithmetic operator, or of every Duration
//! dimension form, confirms which lowered rows carry the operation, and runs it
//! over a fixed operand grid in both execution modes. A program's outcome must
//! be the shared decision's outcome on the same operator and operands: the same
//! value, or the same error code and message. A swapped operand, a substituted
//! operator or a remapped error at a grid point fails the case. Points outside
//! the grid are unchecked, and the error's span is not compared (XSH-157).

use super::noncompletion_tests::{checked_program, function_index};
use super::tests::run_with_large_stack;
use super::*;
use crate::duration::{DurationOperand, DurationResult, checked_duration_binary};
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

/// Every instruction row family whose payload carries a `BinaryOp`, as
/// `schema.rs` declares them, with the operators execution decides for it;
/// `None` admits every operator, which the checked facts then type.
const OPERATOR_ROWS: [(&str, Option<&[BinaryOp]>); 4] = [
    ("IntBinary", Some(&[BinaryOp::Add, BinaryOp::Sub, BinaryOp::Mul, BinaryOp::Div, BinaryOp::Rem])),
    ("BoolIntCompare", Some(&[BinaryOp::Eq, BinaryOp::Ne, BinaryOp::Lt, BinaryOp::Le, BinaryOp::Gt, BinaryOp::Ge])),
    ("BoolLiteralCompareSlot", Some(&[BinaryOp::Eq, BinaryOp::Ne])),
    ("ExprBinary", None),
];

const ARITHMETIC: [(BinaryOp, AssignOp, &str); 5] = [
    (BinaryOp::Add, AssignOp::Add, "+"),
    (BinaryOp::Sub, AssignOp::Sub, "-"),
    (BinaryOp::Mul, AssignOp::Mul, "*"),
    (BinaryOp::Div, AssignOp::Div, "/"),
    (BinaryOp::Rem, AssignOp::Rem, "%"),
];

/// The operand grid of `arith_conformance`, which Lean's `ArithCases` also enumerates.
const INT_VALUES: [i64; 12] = [i64::MIN, i64::MIN + 1, -7, -2, -1, 0, 1, 2, 3, 7, i64::MAX - 1, i64::MAX];
const MILLIS_VALUES: [u64; 8] = [0, 1, 2, 7, 1000, i64::MAX as u64, u64::MAX - 1, u64::MAX];
const COUNT_VALUES: [i64; 8] = [i64::MIN, -7, -1, 0, 1, 2, 7, i64::MAX];

const MIN_INT: &str = "const MIN_INT = -9223372036854775807 - 1\n";

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

/// `program` with `row`'s operator replaced by a fresh pool entry, so rows that
/// share an entry keep theirs.
fn with_operator(program: &FullProgram, row: usize, op: BinaryOp) -> FullProgram {
    let mut forged = program.clone();
    let start = program.store.data[row].range().start as usize;
    forged.store.extra[start] = u32::try_from(forged.store.binary_ops.len()).unwrap();
    forged.store.binary_ops.push(op);
    forged
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

#[test]
fn operator_rows_are_the_schema_rows_that_carry_a_binary_operator() {
    let schema = include_str!("schema.rs");
    let mut pending = false;
    let mut found = Vec::new();
    for line in schema.lines() {
        let line = line.trim();
        if line == "op: BinaryOp," {
            pending = true;
        } else if pending && let Some(rest) = line.strip_prefix("} => ") {
            found.push(rest.split_whitespace().next().unwrap().to_owned());
            pending = false;
        }
    }
    let expected: Vec<_> = OPERATOR_ROWS.iter().map(|(name, _)| (*name).to_owned()).collect();
    assert_eq!(found, expected, "classify each schema row that carries a BinaryOp");
}

const COMPARISON_SOURCE: &str = "pure ordered(left: Int, right: Int) -> Int {\n  if left - right > 0 {\n    return 1\n  }\n  if left - right < 0 or left * right == 7 or left + right >= 100 {\n    return 2\n  }\n  if left - right <= 0 and left + right != 0 {\n    return 3\n  }\n  0\n}\n\
pure named(value: Str) -> Int {\n  if value == \"x\" {\n    return 1\n  }\n  if value != \"y\" {\n    return 2\n  }\n  0\n}\n\
assert ordered(2, 1) == 1\nassert ordered(1, 2) == 2\nassert ordered(1, 1) == 3\nassert named(\"x\") == 1\nassert named(\"z\") == 2\n";

/// The comparison rows of `COMPARISON_SOURCE`: six `BoolIntCompare` rows, one
/// per comparison (an `if` condition and the operands of `or` and `and`), and
/// two `BoolLiteralCompareSlot` rows.
fn comparison_rows(program: &FullProgram) -> Vec<(FullTag, usize)> {
    let mut found = Vec::new();
    for (function, tag, expected) in [
        ("ordered", FullTag::BoolIntCompare, &[BinaryOp::Eq, BinaryOp::Ne, BinaryOp::Lt, BinaryOp::Le, BinaryOp::Gt, BinaryOp::Ge][..]),
        ("named", FullTag::BoolLiteralCompareSlot, &[BinaryOp::Eq, BinaryOp::Ne][..]),
    ] {
        let tagged = rows(program, function_index(program, function), tag);
        let mut ops: Vec<_> = tagged.iter().map(|&row| row_op(program, row)).collect();
        ops.sort_by_key(|op| *op as u8);
        assert_eq!(ops, expected, "{function} lowers its conditions to {tag:?} rows");
        found.extend(tagged.into_iter().map(|row| (tag, row)));
    }
    found
}

/// XSH-159: the typed evaluator decides only comparisons in these rows, and
/// only `==` and `!=` against a literal, so verification must reject every
/// other operator in each comparison row family, wherever the row sits.
#[test]
fn comparison_rows_admit_only_their_comparison_operators() {
    run_with_large_stack(|| {
        let program = Arc::new(checked_program("comparison-operators.xsh", COMPARISON_SOURCE));
        FullVerifier::verify(&program).unwrap();
        for force_frames in [false, true] {
            run_installed(&program, force_frames).unwrap();
        }
        let mut families = std::collections::BTreeSet::new();
        for (tag, row) in comparison_rows(&program) {
            let (_, Some(admitted)) = OPERATOR_ROWS.iter().find(|(name, _)| *name == tag.name()).unwrap() else {
                unreachable!("comparison rows admit a fixed operator set")
            };
            families.insert(tag.name());
            for op in ALL_BINARY_OPS {
                let verified = FullVerifier::verify(&with_operator(&program, row, op));
                if admitted.contains(&op) {
                    verified.unwrap_or_else(|error| panic!("{op:?} in {tag:?} row {row}: {error:?}"));
                } else {
                    let message = verified.expect_err(&format!("{op:?} in {tag:?} row {row}")).message.clone();
                    let expected = if tag == FullTag::BoolIntCompare {
                        "integer comparison row has a non-comparison operator"
                    } else {
                        "literal comparison row has an operator other than == and !="
                    };
                    assert_eq!(message, expected, "{op:?} in {tag:?} row {row}");
                }
            }
        }
        let fixed: std::collections::BTreeSet<_> = OPERATOR_ROWS
            .iter()
            .filter(|(name, admitted)| admitted.is_some() && *name != "IntBinary")
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(families, fixed, "every comparison row family is forged");
    });
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Outcome {
    /// An XSH literal for the value.
    Value(String),
    Failure(&'static str, &'static str),
}

fn int_literal(value: i64) -> String {
    if value == i64::MIN { "MIN_INT".to_owned() } else { value.to_string() }
}

fn int_outcome(op: BinaryOp, left: i64, right: i64) -> Outcome {
    match crate::arith::checked_int_binary(op, left, right).expect("arithmetic operator") {
        Ok(value) => Outcome::Value(int_literal(value)),
        Err(error) => {
            let (code, message) = error.diagnostic();
            Outcome::Failure(code, message)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Dimension {
    Duration,
    Int,
}

impl Dimension {
    fn type_name(self) -> &'static str {
        match self {
            Self::Duration => "Duration",
            Self::Int => "Int",
        }
    }

    fn values(self) -> Vec<DurationOperand> {
        match self {
            Self::Duration => MILLIS_VALUES.iter().map(|&millis| DurationOperand::Millis(millis)).collect(),
            Self::Int => COUNT_VALUES.iter().map(|&count| DurationOperand::Count(count)).collect(),
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::Duration => "0ms",
            Self::Int => "0",
        }
    }
}

fn duration_operand_literal(operand: DurationOperand) -> String {
    match operand {
        DurationOperand::Millis(millis) => format!("{millis}ms"),
        DurationOperand::Count(count) => int_literal(count),
    }
}

fn duration_outcome(op: BinaryOp, left: DurationOperand, right: DurationOperand) -> Outcome {
    match checked_duration_binary(op, left, right) {
        Ok(DurationResult::Millis(millis)) => Outcome::Value(format!("{millis}ms")),
        Ok(DurationResult::Count(count)) => Outcome::Value(int_literal(count)),
        Err(error) => {
            let (code, message) = error.diagnostic();
            Outcome::Failure(code, message)
        }
    }
}

/// The Duration forms SPEC 4.3 admits: operator, operand dimensions and result dimension.
const DURATION_FORMS: [(BinaryOp, AssignOp, &str, Dimension, Dimension, Dimension); 6] = [
    (BinaryOp::Add, AssignOp::Add, "+", Dimension::Duration, Dimension::Duration, Dimension::Duration),
    (BinaryOp::Sub, AssignOp::Sub, "-", Dimension::Duration, Dimension::Duration, Dimension::Duration),
    (BinaryOp::Mul, AssignOp::Mul, "*", Dimension::Duration, Dimension::Int, Dimension::Duration),
    (BinaryOp::Mul, AssignOp::Mul, "*", Dimension::Int, Dimension::Duration, Dimension::Duration),
    (BinaryOp::Div, AssignOp::Div, "/", Dimension::Duration, Dimension::Int, Dimension::Duration),
    (BinaryOp::Div, AssignOp::Div, "/", Dimension::Duration, Dimension::Duration, Dimension::Int),
];

/// One source form of an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Form {
    /// `let result = left OP right` in a function over typed parameters.
    UnannotatedLet,
    /// `let result: T = left OP right` in that function.
    AnnotatedLet,
    /// `left OP right` as the function's value.
    Returned,
    /// `var value = left; value OP= right` in that function.
    LocalCompound,
    /// `left OP right` against the expected literal in two `if` conditions of a
    /// Bool function, one function per case: a bare parameter there would need
    /// type context and keep the condition an expression.
    Compared,
    /// `var value = A; value OP= B` at top level.
    TopLevelCompound,
    /// `A OP B` over literal operands at top level.
    Literal,
    /// `const C = A OP B`.
    Constant,
}

impl Form {
    fn in_function(self) -> bool {
        matches!(self, Self::UnannotatedLet | Self::AnnotatedLet | Self::Returned | Self::LocalCompound | Self::Compared)
    }
}

struct Operation {
    op: BinaryOp,
    assign: AssignOp,
    symbol: &'static str,
    left: Dimension,
    right: Dimension,
    result: Dimension,
}

fn function_source(form: Form, operation: &Operation) -> String {
    let symbol = operation.symbol;
    let (left, right, result) = (operation.left.type_name(), operation.right.type_name(), operation.result.type_name());
    let body = match form {
        Form::UnannotatedLet => format!("  let result = left {symbol} right\n  return result\n"),
        Form::AnnotatedLet => format!("  let result: {result} = left {symbol} right\n  return result\n"),
        Form::Returned => format!("  left {symbol} right\n"),
        Form::LocalCompound => format!("  var value = left\n  value {symbol}= right\n  value\n"),
        Form::Compared | Form::TopLevelCompound | Form::Literal | Form::Constant => return String::new(),
    };
    format!("pure apply(left: {left}, right: {right}) -> {result} {{\n{body}}}\n")
}

/// The statements that compute `left OP right` in `form` and require `expected`.
/// `index` names the case's top-level bindings and functions.
fn case_source(form: Form, operation: &Operation, left: &str, right: &str, expected: &str, index: usize) -> String {
    let symbol = operation.symbol;
    match form {
        Form::Compared => {
            let (left_type, right_type) = (operation.left.type_name(), operation.right.type_name());
            format!(
                "pure apply_{index}(left: {left_type}, right: {right_type}) -> Bool {{\n  if left {symbol} right < {expected} {{\n    return false\n  }}\n  if left {symbol} right > {expected} {{\n    return false\n  }}\n  true\n}}\nassert apply_{index}({left}, {right})\n"
            )
        }
        Form::UnannotatedLet | Form::AnnotatedLet | Form::Returned | Form::LocalCompound => {
            format!("assert apply({left}, {right}) == {expected}\n")
        }
        Form::TopLevelCompound => {
            format!("var value_{index} = {left}\nvalue_{index} {symbol}= {right}\nassert value_{index} == {expected}\n")
        }
        Form::Literal => format!("assert {left} {symbol} {right} == {expected}\n"),
        Form::Constant => format!("const C_{index} = {left} {symbol} {right}\nassert C_{index} == {expected}\n"),
    }
}

/// Requires the lowered rows that carry the operation to be the ones that select
/// `form`'s execution route.
fn assert_route(form: Form, operation: &Operation, source: &str) {
    let program = checked_program("arith-route-shape.xsh", source);
    if form == Form::Compared {
        // Each case has its own function.
        let functions = (0..program.store.functions.len())
            .filter(|&function| program.store.string(program.store.functions[function].name).is_ok_and(|name| name.starts_with("apply_")))
            .collect::<Vec<_>>();
        assert!(!functions.is_empty());
        for function in functions {
            assert_route_in(form, operation, &program, program.store.function_instruction_range(function).unwrap().collect(), source);
        }
        return;
    }
    let scope: Vec<usize> = if form.in_function() {
        program.store.function_instruction_range(function_index(&program, "apply")).unwrap().collect()
    } else {
        (0..program.store.tags.len()).collect()
    };
    assert_route_in(form, operation, &program, scope, source);
}

fn assert_route_in(form: Form, operation: &Operation, program: &FullProgram, scope: Vec<usize>, source: &str) {
    let carrying = |tag: FullTag| {
        scope
            .iter()
            .filter(|&&instruction| program.store.tags[instruction] == tag && row_op(program, instruction) == operation.op)
            .count()
    };
    let (typed, expression) = (carrying(FullTag::IntBinary), carrying(FullTag::ExprBinary));
    let integer = operation.left == Dimension::Int && operation.right == Dimension::Int;
    let shape = match form {
        // The typed Int family: an `IntBinary` row under `StmtLetInt`, or under each `BoolIntCompare`.
        Form::UnannotatedLet if integer => typed == 1 && expression == 0,
        Form::Compared => {
            let comparisons: Vec<_> = scope
                .iter()
                .filter(|&&instruction| program.store.tags[instruction] == FullTag::BoolIntCompare)
                .map(|&instruction| row_op(program, instruction))
                .collect();
            // A negative expected literal may add its own rows.
            typed >= 2 && expression == 0 && comparisons == [BinaryOp::Lt, BinaryOp::Gt]
        }
        Form::UnannotatedLet | Form::AnnotatedLet | Form::Returned | Form::Literal => typed == 0 && expression == 1,
        // The operator travels as the statement's `AssignOp`.
        Form::LocalCompound => {
            let decoder = function_view_decoder(&program.store, function_index(program, "apply"));
            let assignments: Vec<_> = scope
                .iter()
                .filter_map(|&instruction| match decoder.statement_view(instruction) {
                    Ok(schema::StmtView::Assign { op, .. }) => Some(op),
                    _ => None,
                })
                .collect();
            typed == 0 && expression == 0 && assignments == [operation.assign]
        }
        Form::TopLevelCompound => {
            typed == 0
                && expression == 0
                && program.store.driver_steps.iter().filter(|step| step.tag == FullDriverTag::Assign).count() == 1
        }
        // Constant preparation folds the operation away.
        Form::Constant => typed == 0 && expression == 0,
    };
    assert!(shape, "{form:?} {:?}: IntBinary {typed}, ExprBinary {expression}\n{source}", operation.op);
}

fn evaluate(source: &str, force_frames: bool) -> crate::runtime::eval::EvalOutput {
    let name = "arith-route.xsh";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        name,
        crate::loader::entry_source_from_text(name, source.to_owned()),
        Vec::new(),
    );
    assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
    let source_id = parsed.arena.source_text_source_id().unwrap();
    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.force_frames = force_frames;
    evaluator.eval(&parsed.arena, source_id)
}

fn render(output: &crate::runtime::eval::EvalOutput) -> String {
    format!(
        "status {}, diagnostics {:?}, traceback {:?}",
        output.status,
        output.diagnostics.iter().map(|diagnostic| &diagnostic.message).collect::<Vec<_>>(),
        output.traceback.as_ref().map(|traceback| (&traceback.error.kind, &traceback.error.message)),
    )
}

/// Runs every case of one operation: the successes in one program per mode, and
/// each failure alone, since an arithmetic failure ends the program. Returns the
/// number of programs run.
fn run_cases(form: Form, operation: &Operation, cases: &[(String, String, Outcome)]) -> usize {
    let prelude = format!("{MIN_INT}{}", function_source(form, operation));
    // The route is read from `7 OP 2`, which succeeds with a positive result for every form.
    let sample = cases
        .iter()
        .find(|(left, right, _)| left.trim_end_matches("ms") == "7" && right.trim_end_matches("ms") == "2")
        .expect("the sample case");
    let Outcome::Value(value) = &sample.2 else { panic!("7 OP 2 fails") };
    assert_route(form, operation, &format!("{prelude}{}", case_source(form, operation, &sample.0, &sample.1, value, 0)));
    // Constant preparation runs before either execution mode.
    let modes: &[bool] = if form == Form::Constant { &[false] } else { &[false, true] };
    let mut successes = prelude.clone();
    for (index, (left, right, expected)) in cases.iter().enumerate() {
        if let Outcome::Value(value) = expected {
            successes.push_str(&case_source(form, operation, left, right, value, index));
        }
    }
    if form == Form::Compared {
        assert_route(form, operation, &successes);
    }
    let mut programs = 0;
    for &force_frames in modes {
        let output = evaluate(&successes, force_frames);
        programs += 1;
        assert!(
            output.status == 0 && output.traceback.is_none() && output.diagnostics.is_empty(),
            "{form:?} force_frames={force_frames}\n{successes}\n{}",
            render(&output),
        );
    }
    for (left, right, expected) in cases {
        let Outcome::Failure(code, message) = expected else { continue };
        let source = format!("{prelude}{}", case_source(form, operation, left, right, operation.result.placeholder(), 0));
        for &force_frames in modes {
            let output = evaluate(&source, force_frames);
            programs += 1;
            if form == Form::Constant {
                // Preparation declines the failure, and the checker reports the constant.
                assert!(
                    output.status == 2
                        && output.traceback.is_none()
                        && output.diagnostics.iter().any(|diagnostic| diagnostic.message == "invalid constant operation or arithmetic overflow"),
                    "{form:?} expected a declined constant for {code}\n{source}\n{}",
                    render(&output),
                );
            } else {
                assert!(
                    output.stdout.is_empty()
                        && output.traceback.as_ref().is_some_and(|traceback| traceback.error.kind == *code && traceback.error.message == *message),
                    "{form:?} force_frames={force_frames} expected {code}: {message}\n{source}\n{}",
                    render(&output),
                );
            }
        }
    }
    programs
}

fn int_route(form: Form) {
    run_with_large_stack(move || {
        let (mut cases_run, mut programs) = (0, 0);
        for (op, assign, symbol) in ARITHMETIC {
            let operation = Operation { op, assign, symbol, left: Dimension::Int, right: Dimension::Int, result: Dimension::Int };
            let cases: Vec<_> = INT_VALUES
                .iter()
                .flat_map(|&left| INT_VALUES.iter().map(move |&right| (left, right)))
                .map(|(left, right)| (int_literal(left), int_literal(right), int_outcome(op, left, right)))
                .collect();
            cases_run += cases.len();
            programs += run_cases(form, &operation, &cases);
        }
        assert_eq!(cases_run, 720);
        eprintln!("{form:?}: {cases_run} Int cases in {programs} programs");
    });
}

fn duration_route(form: Form) {
    run_with_large_stack(move || {
        let (mut cases_run, mut programs) = (0, 0);
        for (op, assign, symbol, left, right, result) in DURATION_FORMS {
            // `value OP= right` keeps `value`'s dimension.
            if matches!(form, Form::LocalCompound | Form::TopLevelCompound) && left != result {
                continue;
            }
            let operation = Operation { op, assign, symbol, left, right, result };
            let cases: Vec<_> = left
                .values()
                .into_iter()
                .flat_map(|l| right.values().into_iter().map(move |r| (l, r)))
                .map(|(l, r)| (duration_operand_literal(l), duration_operand_literal(r), duration_outcome(op, l, r)))
                .collect();
            cases_run += cases.len();
            programs += run_cases(form, &operation, &cases);
        }
        eprintln!("{form:?}: {cases_run} Duration cases in {programs} programs");
    });
}

#[test]
fn int_unannotated_let_route_conformance() {
    int_route(Form::UnannotatedLet);
}

#[test]
fn int_annotated_let_route_conformance() {
    int_route(Form::AnnotatedLet);
}

#[test]
fn int_returned_route_conformance() {
    int_route(Form::Returned);
}

#[test]
fn int_local_compound_route_conformance() {
    int_route(Form::LocalCompound);
}

#[test]
fn int_compared_route_conformance() {
    int_route(Form::Compared);
}

#[test]
fn int_top_level_compound_route_conformance() {
    int_route(Form::TopLevelCompound);
}

#[test]
fn int_literal_route_conformance() {
    int_route(Form::Literal);
}

#[test]
fn int_constant_route_conformance() {
    int_route(Form::Constant);
}

#[test]
fn duration_unannotated_let_route_conformance() {
    duration_route(Form::UnannotatedLet);
}

#[test]
fn duration_annotated_let_route_conformance() {
    duration_route(Form::AnnotatedLet);
}

#[test]
fn duration_returned_route_conformance() {
    duration_route(Form::Returned);
}

#[test]
fn duration_local_compound_route_conformance() {
    duration_route(Form::LocalCompound);
}

#[test]
fn duration_literal_route_conformance() {
    duration_route(Form::Literal);
}

#[test]
fn duration_constant_route_conformance() {
    duration_route(Form::Constant);
}
