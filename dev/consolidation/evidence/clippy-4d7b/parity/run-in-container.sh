#!/bin/bash
set -u
run_dir=/scratch
{
  printf 'LD_PRELOAD=unset\n'
  env | sort
  grep '^Cpus_allowed_list:' /proc/self/status || true
  for cap in /sys/fs/cgroup/cpuset.cpus.effective /sys/fs/cgroup/cpu.max /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max; do
    if [ -r "$cap" ]; then printf '%s=' "$cap"; cat "$cap"; fi
  done
} > "$run_dir/container-environment.txt"
: > "$run_dir/actual-argv.tsv"
run_one() {
  local label="$1" project="$2" binary="$3" command="$4"
  local stdout="$run_dir/$label.stdout" stderr="$run_dir/$label.stderr" status_file="$run_dir/$label.status"
  printf '%s\t%s %s .\tworkdir=%s\n' "$label" "$binary" "$command" "$project" >> "$run_dir/actual-argv.tsv"
  set +e
  (cd "$project" && busybox timeout -k 5 240 "$binary" "$command" .) >"$stdout" 2>"$stderr"
  local status=$?
  set -e
  printf '%s\n' "$status" > "$status_file"
  printf '%s=%s\n' "$label" "$status"
}
run_one baseline-repository-check /work /reference/xsht check
run_one baseline-repository-lint /work /reference/xsht lint
run_one baseline-laputa-check /laputa /reference/xsht check
run_one baseline-laputa-lint /laputa /reference/xsht lint
run_one candidate-repository-check /work /candidate/xsht check
run_one candidate-repository-lint /work /candidate/xsht lint
run_one candidate-laputa-check /laputa /candidate/xsht check
run_one candidate-laputa-lint /laputa /candidate/xsht lint
