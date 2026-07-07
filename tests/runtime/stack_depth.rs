use super::common::*;

const SMALL_STACK_ENV: &[(&str, &str)] = &[("XSH_TEST_SMALL_EVAL_STACK", "1")];

fn run_small_stack_stress(name: &str, source: &str) {
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
}

fn nested_expression_source(depth: usize) -> String {
    let mut source = String::from("let value = ");
    for _ in 0..depth {
        source.push('(');
    }
    source.push('1');
    for _ in 0..depth {
        source.push_str(" + 1)");
    }
    source.push_str("\nprint ${value}\n");
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
#[ignore = "stress gate for stack-depth runtime safety work"]
fn small_stack_main_self_recursion_does_not_abort() {
    run_small_stack_stress(
        "stack-depth-main-self-recursion",
        include_str!("../fixtures/runtime/stack-depth/main-self-recursion.xsh"),
    );
}

#[test]
#[ignore = "stress gate for stack-depth runtime safety work"]
fn small_stack_main_mutual_recursion_does_not_abort() {
    run_small_stack_stress(
        "stack-depth-main-mutual-recursion",
        include_str!("../fixtures/runtime/stack-depth/main-mutual-recursion.xsh"),
    );
}

#[test]
#[ignore = "stress gate for stack-depth runtime safety work"]
fn small_stack_deep_expression_nesting_does_not_abort() {
    let source = nested_expression_source(40);
    run_small_stack_stress("stack-depth-deep-expression-nesting", &source);
}

#[test]
#[ignore = "stress gate for stack-depth runtime safety work"]
fn small_stack_main_nested_blocks_do_not_abort() {
    run_small_stack_stress(
        "stack-depth-main-nested-blocks",
        include_str!("../fixtures/runtime/stack-depth/main-nested-blocks.xsh"),
    );
}

#[test]
#[ignore = "stress gate for stack-depth runtime safety work"]
fn small_stack_par_map_worker_recursion_does_not_abort() {
    run_small_stack_stress(
        "stack-depth-par-map-worker-recursion",
        include_str!("../fixtures/runtime/stack-depth/par-map-worker-recursion.xsh"),
    );
}
