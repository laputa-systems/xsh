set -eu
run_dir=/scratch/typing-814-runtime
harness="$XSH_TEST_HARNESS"
unset LD_PRELOAD || true
{
  printf 'LD_PRELOAD=unset\n'
  printf 'XSH_TEST_HARNESS=%s\n' "$harness"
  grep '^Cpus_allowed_list:' /proc/self/status || true
  for cap in /sys/fs/cgroup/cpuset.cpus.effective /sys/fs/cgroup/cpu.max /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max; do
    if [ -r "$cap" ]; then printf '%s=' "$cap"; cat "$cap"; fi
  done
} > "$run_dir/container-resource-caps.txt"
"$harness" --list > "$run_dir/tests-list.txt"
run_case() {
  name=$1
  filter=$2
  exact=${3:-false}
  set +e
  if [ "$exact" = true ]; then
    busybox timeout -k 5 300 "$harness" "$filter" --exact > "$run_dir/$name.stdout" 2> "$run_dir/$name.stderr"
  else
    busybox timeout -k 5 300 "$harness" "$filter" > "$run_dir/$name.stdout" 2> "$run_dir/$name.stderr"
  fi
  status=$?
  set -e
  printf '%s\n' "$status" > "$run_dir/$name.status"
  [ "$status" -eq 0 ]
}
run_case constraints-module 'sema::constraints::tests::'
run_case schemes-module 'sema::schemes::tests::'
run_case checker-local-probe-checked-history 'sema::check::local_inference::tests::local_constraint_probe_does_not_copy_checked_expression_history' true
run_case checker-local-probe-body-history 'sema::check::local_inference::tests::local_constraint_probe_does_not_copy_completed_body_binding_history' true
run_case checker-imported-effects 'sema::check::module_driver::tests::imported_effects_flow_into_entry_inference' true
