set +e
unset LD_PRELOAD
binary=/target/x86_64-unknown-linux-musl/debug/build/xsh/692c8c084af82ba2/out/xsh-692c8c084af82ba2
output=/scratch/procps-exit-redgreen/green/attempt2
sha256sum "$binary" > "$output/runtime-binary.sha256"
printf '%s\n' "$binary -Z unstable-options linux_sample_process_tests --nocapture --format=json" > "$output/test.argv.txt"
"$binary" -Z unstable-options linux_sample_process_tests --nocapture --format=json > "$output/test.stdout" 2> "$output/test.stderr"
test_status=$?
printf '%s\n' "$test_status" > "$output/test.status"
printf 'linux_sample_process_tests %s\n' "$test_status"
sha256sum "$binary" > "$output/runtime-binary-after.sha256"
