use super::common::*;
use std::sync::{Mutex, OnceLock};

const SMALL_STACK_ENV: &[(&str, &str)] = &[("XSH_TEST_SMALL_EVAL_STACK", "1")];

fn small_stack_stress_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn run_small_stack_stress(name: &str, source: &str) -> std::process::Output {
    let _lock = small_stack_stress_lock()
        .lock()
        .expect("small-stack stress lock");
    let output = run_temp_script_with_env(name, source, [], SMALL_STACK_ENV);
    assert!(
        output.status.code().is_some(),
        "{name} aborted instead of producing a structured XSH result\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "{name} did not complete under small-stack mode\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn nested_expression_source(depth: usize) -> String {
    // Nest to the right: `+` is left-associative, so every pair of
    // parentheses in `1 + (1 + (... 1))` is required.
    let mut source = String::from("let value = ");
    for _ in 1..depth {
        source.push_str("1 + (");
    }
    source.push_str("1 + 1");
    for _ in 1..depth {
        source.push(')');
    }
    source.push_str("\nprint ${value}\n");
    source
}

fn nested_native_test_source(depth: usize) -> String {
    let mut source = String::from("test test_nested [error] {\n");
    for _ in 0..depth {
        source.push_str("  if true {\n");
    }
    source.push_str("  test.eq(\"ok\", \"ok\")?\n");
    for _ in 0..depth {
        source.push_str("  }\n");
    }
    source.push_str("}\n");
    source
}

#[test]
fn small_stack_mode_runs_simple_script() {
    let output = run_temp_script_with_env(
        "small-stack-smoke",
        "let value = 40 + 2\nprint ${value}\n",
        [],
        SMALL_STACK_ENV,
    );

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "42\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn small_stack_global_record_constructor_named_arguments_do_not_abort() {
    let field_count = 64;
    let fields = (0..field_count)
        .map(|index| format!("field_{index}: Int"))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = (0..field_count)
        .rev()
        .map(|index| format!("field_{index}: {index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut source = format!(
        "type Profile = {{{fields}}}\nexport let profile: Profile = Profile({arguments})\n"
    );
    for index in 0..field_count {
        source.push_str(&format!("assert profile.field_{index} == {index}\n"));
    }
    source.push_str("print complete\n");
    let output = run_small_stack_stress("stack-depth-global-record-constructor", &source);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "complete\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn small_stack_main_self_recursion_does_not_abort() {
    run_small_stack_stress(
        "stack-depth-main-self-recursion",
        include_str!("../fixtures/runtime/stack-depth/main-self-recursion.xsh"),
    );
}

#[test]
fn small_stack_main_mutual_recursion_does_not_abort() {
    run_small_stack_stress(
        "stack-depth-main-mutual-recursion",
        include_str!("../fixtures/runtime/stack-depth/main-mutual-recursion.xsh"),
    );
}

#[test]
fn small_stack_deep_expression_nesting_does_not_abort() {
    let source = nested_expression_source(40);
    run_small_stack_stress("stack-depth-deep-expression-nesting", &source);
}

#[test]
fn small_stack_main_nested_blocks_do_not_abort() {
    run_small_stack_stress(
        "stack-depth-main-nested-blocks",
        include_str!("../fixtures/runtime/stack-depth/main-nested-blocks.xsh"),
    );
}

#[test]
fn small_stack_par_map_worker_recursion_does_not_abort() {
    run_small_stack_stress(
        "stack-depth-par-map-worker-recursion",
        include_str!("../fixtures/runtime/stack-depth/par-map-worker-recursion.xsh"),
    );
}

#[test]
fn small_stack_nested_lowered_result_calls_do_not_abort() {
    let output = run_small_stack_stress(
        "stack-depth-nested-lowered-result",
        include_str!("../fixtures/runtime/stack-depth/nested-lowered-result.xsh"),
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2\n2\n");
}

#[test]
fn small_stack_nested_result_record_materialization_does_not_abort() {
    let output = run_small_stack_stress(
        "stack-depth-nested-result-record-materialization",
        include_str!("../fixtures/runtime/stack-depth/nested-process-result.xsh"),
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "631\n");
}

#[test]
fn small_stack_result_projections_in_native_arguments_do_not_abort() {
    let output = run_small_stack_stress(
        "stack-depth-nested-result-projections",
        include_str!("../fixtures/runtime/stack-depth/nested-result-projections.xsh"),
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "7\n");
}

#[test]
fn small_stack_projection_arguments_preserve_order_and_error_cleanup() {
    let output = run_small_stack_stress(
        "stack-depth-projection-argument-order",
        include_str!("../fixtures/runtime/stack-depth/projection-argument-order.xsh"),
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "base\nbase cleanup\nindex\nexpected\nbase\nbase cleanup\nouter cleanup\ntrue\n"
    );
}

#[test]
fn small_stack_nested_format_method_calls_do_not_abort() {
    let output = run_small_stack_stress(
        "stack-depth-nested-format-method-calls",
        include_str!("../fixtures/runtime/stack-depth/nested-result-string-transform.xsh"),
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "MT_TOOL_MAX\n");
}

#[test]
fn small_stack_indexed_frames_run_defers_on_abort() {
    let output = run_temp_script_with_env(
        "stack-depth-indexed-frame-defer",
        r#"
defer run printf "%s\\n" top ?

proc main() -> Result[Unit] {
  defer run printf "%s\\n" proc ?
  fail()?
  return Ok()
}

proc fail() -> Result[Unit] {
  defer run printf "%s\\n" nested ?
  exit 9
  return Ok()
}

main()?
"#,
        [],
        SMALL_STACK_ENV,
    );

    assert_eq!(output.status.code(), Some(9));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nested\nproc\ntop\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn small_stack_xsht_native_test_body_does_not_abort() {
    let root = temp_path("stack-depth-xsht-native-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tests")).expect("create native test dir");
    std::fs::write(root.join("tests/deep.xsh"), nested_native_test_source(120))
        .expect("write native test");

    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "--exact", "tests/deep.xsh::test_nested"])
        .env("XSH_TEST_SMALL_EVAL_STACK", "1")
        .current_dir(&root)
        .output()
        .expect("run xsht test");
    let _ = std::fs::remove_dir_all(root);

    assert!(
        output.status.success(),
        "xsht native test aborted or failed under small-stack mode\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn small_stack_yield_delegation_pulls_and_cancels_deep_chains() {
    let output = run_small_stack_stress(
        "stack-depth-yield-delegation",
        include_str!("../fixtures/runtime/stack-depth/yield-delegation.xsh"),
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "full 7\nfull 8\nclosed 0\nclosed 3000\nearly 7\nclosed 0\nclosed 3000\n"
    );
}

#[test]
fn small_stack_yield_delegation_late_error_unwinds_deep_chain() {
    let _lock = small_stack_stress_lock()
        .lock()
        .expect("small-stack stress lock");
    let output = run_temp_script_with_env(
        "stack-depth-yield-delegation-error",
        include_str!("../fixtures/runtime/stack-depth/yield-delegation-error.xsh"),
        [],
        SMALL_STACK_ENV,
    );
    assert_eq!(
        output.status.code(),
        Some(3),
        "delegation failure must be structured"
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "row 7\nclosed 0\nclosed 3000\n"
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("late-delegation-error")
    );
}
