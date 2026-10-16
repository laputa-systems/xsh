set -eu
run_dir=/scratch/typing-814-runtime
harness="$XSH_TEST_HARNESS"
unset LD_PRELOAD || true
run_case() {
  name=$1
  test=$2
  set +e
  busybox timeout -k 5 300 "$harness" "$test" --exact > "$run_dir/$name.stdout" 2> "$run_dir/$name.stderr"
  status=$?
  set -e
  printf '%s\n' "$status" > "$run_dir/$name.status"
  [ "$status" -eq 0 ]
}
run_case checker-local-probe-checked-history 'sema::check::local_inference::tests::local_constraint_probe_does_not_copy_checked_expression_history'
run_case checker-local-probe-body-history 'sema::check::local_inference::tests::local_constraint_probe_does_not_copy_completed_body_binding_history'
run_case checker-imported-effects 'sema::check::module_driver::tests::imported_effects_flow_into_entry_inference'
