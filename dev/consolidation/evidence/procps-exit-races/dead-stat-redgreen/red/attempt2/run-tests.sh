set +e
unset LD_PRELOAD
binary=/target/x86_64-unknown-linux-musl/debug/build/xsh/692c8c084af82ba2/out/xsh-692c8c084af82ba2
output=/scratch/procps-exit-redgreen/red/attempt2
sha256sum "$binary" > "$output/runtime-binary.sha256"
names=(
  sample_processes_skips_exited_stat_with_invalid_process_group_fields
  sample_processes_rejects_invalid_process_group_fields_for_live_stat
  sample_processes_keeps_valid_live_stat
)
for name in "${names[@]}"; do
  test_name="modules::process::linux_sample_process_tests::$name"
  printf '%s\n' "$binary -Z unstable-options --exact $test_name --nocapture --format=json" > "$output/$name.argv.txt"
  "$binary" -Z unstable-options --exact "$test_name" --nocapture --format=json > "$output/$name.stdout" 2> "$output/$name.stderr"
  test_status=$?
  printf '%s\n' "$test_status" > "$output/$name.status"
  printf '%s %s\n' "$name" "$test_status"
done
sha256sum "$binary" > "$output/runtime-binary-after.sha256"
