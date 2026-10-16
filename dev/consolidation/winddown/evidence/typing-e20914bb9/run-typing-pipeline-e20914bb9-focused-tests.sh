#!/usr/bin/env bash
set -Eeuo pipefail
readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/typing-pipeline-integration
readonly run=$root/.work/tmp/typing-pipeline-e20914bb9
readonly test_run=$run/focused-tests
readonly target=$root/.work/target-typing-c2d8f87
readonly image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly expected_head=e20914bb9c9091cab36bfe219184e2f9dbb5dca1
readonly expected_tree=1cae8ed274160dcb0df3e4168e28012ff1681647
readonly harness=$target/x86_64-unknown-linux-musl/debug/build/xsh/692c8c084af82ba2/out/xsh-692c8c084af82ba2
readonly harness_sha=ab2cc085211ae353f99a348f8de4cd28978b51f49aef1be12b31bb40adda4423
readonly container_harness=/target/x86_64-unknown-linux-musl/debug/build/xsh/692c8c084af82ba2/out/xsh-692c8c084af82ba2
readonly common_git=$(git -C "$lane" rev-parse --path-format=absolute --git-common-dir)
fail() { printf '%s\n' "$*" >&2; exit 1; }
snapshot() {
  local destination=$1
  {
    printf 'head='; git -C "$lane" rev-parse HEAD
    printf 'tree='; git -C "$lane" rev-parse 'HEAD^{tree}'
    printf 'status='; git -C "$lane" status --porcelain=v1 --untracked-files=all
    sha256sum "$lane/Cargo.lock" "$lane/rust-toolchain.toml"
  } >"$destination"
}
[[ $(git -C "$lane" rev-parse HEAD) == "$expected_head" ]] || fail 'source HEAD differs from qualified pin'
[[ $(git -C "$lane" rev-parse 'HEAD^{tree}') == "$expected_tree" ]] || fail 'source tree differs from qualified pin'
[[ -z $(git -C "$lane" status --porcelain=v1 --untracked-files=all) ]] || fail 'source checkout is not clean'
[[ -x $harness && ! -L $harness ]] || fail 'qualified harness is missing or unsafe'
[[ $(sha256sum "$harness" | cut -d ' ' -f 1) == "$harness_sha" ]] || fail 'qualified harness hash differs'
[[ -d $lane/.work/tmp && -d $target ]] || fail 'source scratch or target missing'
[[ ! -e $test_run && ! -L $test_run ]] || fail 'test output already exists'
mkdir -p "$test_run/tmp"
snapshot "$test_run/source-before-tests.txt"
sha256sum "$harness" | head -c 64 >"$test_run/harness-before.sha256"
container_script='set -eu
run=/scratch/focused-tests
harness=$1
{
  grep "^Cpus_allowed_list:" /proc/self/status || true
  for cap in /sys/fs/cgroup/cpuset.cpus.effective /sys/fs/cgroup/cpu.max /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max; do
    if [ -r "$cap" ]; then printf "%s=" "$cap"; cat "$cap"; fi
  done
} >"$run/resource-caps.txt"
"$harness" --list >"$run/harness-tests.txt" 2>"$run/harness-list.stderr"
mapfile -t full < <(sed -n "s/^\\(runtime::eval::indexed::full::tests::[^:]*\\): test$/\\1/p" "$run/harness-tests.txt")
mapfile -t constraints < <(sed -n "s/^\\(sema::constraints::tests::[^:]*\\): test$/\\1/p" "$run/harness-tests.txt")
mapfile -t schemes < <(sed -n "s/^\\(sema::schemes::tests::[^:]*\\): test$/\\1/p" "$run/harness-tests.txt")
mapfile -t stats < <(sed -n "s/^\\(frontend_stats::tests::[^:]*\\): test$/\\1/p" "$run/harness-tests.txt")
test "${#full[@]}" -gt 0
test "${#constraints[@]}" -gt 0
test "${#schemes[@]}" -gt 0
test "${#stats[@]}" -gt 0
metadata=(
  sema::check::module_driver::tests::checked_identity_scheme_is_rank1_and_reuses_one_binder
  sema::check::module_driver::tests::scheme_instantiation_is_fresh_and_does_not_escape
  sema::check::module_driver::tests::captured_mutable_bindings_are_not_generalized
  sema::check::module_driver::tests::imported_identity_and_projection_schemes_are_order_independent_across_roots
  sema::check::module_driver::tests::repeated_generic_calls_add_call_facts_without_increasing_final_checker_body_entries
  sema::check::module_driver::tests::recursive_component_remains_monomorphic
)
for selector in "${metadata[@]}"; do
  grep -Fqx "$selector: test" "$run/harness-tests.txt"
done
selectors=(
  "${full[@]}"
  runtime::eval::indexed::full::tests::user_schemes_execute_after_frontend_drop_on_both_routes
  "${constraints[@]}"
  "${schemes[@]}"
  "${stats[@]}"
  "${metadata[@]}"
)
printf "%s\n" "${selectors[@]}" >"$run/selectors.txt"
printf 'full=%s\nconstraints=%s\nschemes=%s\nfrontend_stats=%s\nmetadata=%s\nexplicit_prepared=%s\ntotal=%s\n' \
  "${#full[@]}" "${#constraints[@]}" "${#schemes[@]}" "${#stats[@]}" "${#metadata[@]}" 1 "${#selectors[@]}" >"$run/selection-counts.txt"
: >"$run/summary.tsv"
index=0
for selector in "${selectors[@]}"; do
  index=$((index + 1))
  log_name=$(printf "%03d" "$index")
  if "$harness" "$selector" --exact --nocapture >"$run/$log_name.stdout" 2>"$run/$log_name.stderr"; then status=0; else status=$?; fi
  printf "%s\t%s\n" "$status" "$selector" >>"$run/summary.tsv"
done
sha256sum "$harness" | head -c 64 >"$run/harness-after.sha256"'
set +e
docker run --rm --init --platform linux/amd64 --cpuset-cpus=12-15 --cpus=4 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
  --volume "$lane:/work:ro" \
  --volume "$lane/.work/tmp:/work/.work/tmp:rw" \
  --volume "$target:/target:ro" \
  --volume "$test_run:/scratch/focused-tests:rw" \
  --volume "$common_git:$common_git:ro" \
  --workdir /work \
  --env TMPDIR=/scratch/focused-tests/tmp \
  "$image" -c "$container_script" _ "$container_harness" >"$test_run/docker.stdout" 2>"$test_run/docker.stderr"
status=$?
set -e
printf '%s\n' "$status" >"$test_run/docker-exit-status.txt"
snapshot "$test_run/source-after-tests.txt"
cmp -s "$test_run/source-before-tests.txt" "$test_run/source-after-tests.txt" || fail 'source changed during tests'
cmp -s "$test_run/harness-before.sha256" "$test_run/harness-after.sha256" || fail 'qualified harness changed during tests'
[[ $status == 0 ]] || fail "focused test container exited $status"
awk -F '\t' 'BEGIN { pass=0; fail=0 } $1 == 0 { pass++; next } { fail++ } END { printf "passed=%d failed=%d total=%d\n", pass, fail, pass+fail }' "$test_run/summary.tsv"
cat "$test_run/selection-counts.txt"
cat "$test_run/summary.tsv"
