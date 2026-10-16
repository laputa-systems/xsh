#!/usr/bin/env bash
set -Eeuo pipefail

readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/typing-pipeline-integration
readonly source_tmp=$lane/.work/tmp
readonly scratch=$root/.work/tmp
readonly output=$scratch/typing-native-witnesses-e20914bb9
readonly target_dir=$root/.work/target-typing-c2d8f87
readonly image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly target=x86_64-unknown-linux-musl
readonly expected_head=e20914bb9c9091cab36bfe219184e2f9dbb5dca1
readonly expected_tree=1cae8ed274160dcb0df3e4168e28012ff1681647
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

[[ -d $lane && -d $source_tmp && -d $scratch ]] || fail 'source, source scratch, or output scratch missing'
git -C "$lane" check-ignore -q .work/tmp || fail 'source scratch is not ignored'
[[ $(git -C "$lane" rev-parse HEAD) == "$expected_head" ]] || fail 'source HEAD differs from approved pin'
[[ $(git -C "$lane" rev-parse 'HEAD^{tree}') == "$expected_tree" ]] || fail 'source tree differs from approved pin'
[[ -z $(git -C "$lane" status --porcelain=v1 --untracked-files=all) ]] || fail 'source checkout is not clean'
[[ -d $target_dir/x86_64-unknown-linux-musl/verification ]] || fail 'verification products missing'
[[ ! -e $output && ! -L $output ]] || fail 'output directory already exists'

mkdir "$output" "$output/tmp"
snapshot "$output/source-before.txt"
sha256sum "$target_dir/x86_64-unknown-linux-musl/verification/xsh" \
  "$target_dir/x86_64-unknown-linux-musl/verification/xsht" \
  "$target_dir/x86_64-unknown-linux-musl/verification/xshi" >"$output/products-before.sha256"
cp "$output/products-before.sha256" "$output/products-expected.sha256"
printf 'head=%s\ntree=%s\nimage=%s\nplatform=linux/amd64\ncpus=4\nmemory=8g\nmemory_swap=8g (no swap)\nnetwork=none\nprofile=verification\ntest selectors=20\n' \
  "$expected_head" "$expected_tree" "$image" >"$output/environment-and-limits.txt"
printf '%s\n' "$(docker image inspect --format '{{.Id}} {{.Os}} {{.Architecture}}' "$image")" >"$output/image-identity.txt"
[[ $(cat "$output/image-identity.txt") == "$image linux amd64" ]] || fail 'pinned image identity mismatch'

container_script='set -eu
run_dir=/scratch/typing-native-witnesses-e20914bb9
product_dir=/target/x86_64-unknown-linux-musl/verification
{
  grep "^Cpus_allowed_list:" /proc/self/status || true
  for cap in /sys/fs/cgroup/cpuset.cpus.effective /sys/fs/cgroup/cpu.max /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max; do
    if [ -r "$cap" ]; then printf "%s=" "$cap"; cat "$cap"; fi
  done
} >"$run_dir/test-resource-caps.txt"
sha256sum "$product_dir/xsh" "$product_dir/xsht" "$product_dir/xshi" >"$run_dir/products-container-before.sha256"
unset LD_PRELOAD || true
export PATH="$product_dir:$PATH"
xsht=$product_dir/xsht
selectors=(
  tests/xsh/typing-capability-witnesses.xsh::test_typing_identity_and_open_record_projection
  tests/xsh/typing-capability-witnesses.xsh::test_typing_identity_rejects_a_wrong_context
  tests/xsh/typing-capability-witnesses.xsh::test_typing_open_record_projection_requires_the_projected_field
  tests/xsh/typing-capability-witnesses.xsh::test_typing_annotated_projection_missing_field_remains_rejected
  tests/xsh/typing-capability-witnesses.xsh::test_typing_dynamic_values_do_not_gain_a_concrete_type
  tests/xsh/typing-capability-witnesses.xsh::test_typing_exported_scheme_is_independent_of_other_importers
  tests/xsh/rank1-inference.xsh::rank1_exported_identity_instantiates_each_import_root
  tests/xsh/rank1-inference.xsh::rank1_immutable_alias_instantiates_independent_direct_calls
  tests/xsh/rank1-inference.xsh::rank1_named_required_calls_preserve_source_argument_order
  tests/xsh/rank1-inference.xsh::rank1_generic_rest_calls_preserve_prefix_and_splice_shapes
  tests/xsh/rank1-inference.xsh::rank1_generic_function_cannot_be_stored_as_typed_callable
  tests/xsh/rank1-inference.xsh::rank1_generic_rest_items_reject_wrong_element_types
  tests/xsh/rank1-inference.xsh::rank1_generic_rest_splice_before_fixed_slot_fails_closed
  tests/xsh/rank1-inference.xsh::rank1_generic_default_parameters_fail_closed_in_lowering
  tests/xsh/rank1-inference.xsh::rank1_private_and_exported_projections_accept_different_rows
  tests/xsh/rank1-inference.xsh::rank1_explicit_any_and_monomorphic_prefix_body_keep_their_contracts
  tests/xsh/rank1-inference.xsh::rank1_quantified_bodies_reject_prefix_returns_and_optional_fields
  tests/xsh/rank1-inference.xsh::rank1_nongeneric_unresolved_function_material_still_needs_annotation
  tests/xsh/size-literals.xsh::test_size_literal_is_a_plain_unsigned_integer
  tests/xsh/size-literals.xsh::test_size_literal_binding_is_unsigned
)
printf "%s\n" "${selectors[@]}" >"$run_dir/selectors.txt"
: >"$run_dir/summary.tsv"
index=0
for selector in "${selectors[@]}"; do
  index=$((index + 1))
  test_log=$(printf "%02d" "$index")
  if "$xsht" test --exact "$selector" >"$run_dir/$test_log.stdout" 2>"$run_dir/$test_log.stderr"; then status=0; else status=$?; fi
  printf "%s\t%s\n" "$status" "$selector" >>"$run_dir/summary.tsv"
done
sha256sum "$product_dir/xsh" "$product_dir/xsht" "$product_dir/xshi" >"$run_dir/products-container-after.sha256"
[[ $(sha256sum "$product_dir/xsh" "$product_dir/xsht" "$product_dir/xshi") == "$(cat "$run_dir/products-container-before.sha256")" ]] || {
  diff -u "$run_dir/products-container-before.sha256" <(sha256sum "$product_dir/xsh" "$product_dir/xsht" "$product_dir/xshi") || true
  exit 3
}
printf "selected=%s\n" "${#selectors[@]}" >"$run_dir/selection-count.txt"
'

set +e
docker run --rm --init --platform linux/amd64 --cpuset-cpus=12-15 --cpus=4 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
  --volume "$lane:/work:ro" \
  --volume "$source_tmp:/work/.work/tmp:rw" \
  --volume "$target_dir:/target:ro" \
  --volume "$output:/scratch/typing-native-witnesses-e20914bb9:rw" \
  --volume "$common_git:$common_git:ro" \
  --workdir /work \
  --env TMPDIR=/scratch/typing-native-witnesses-e20914bb9/tmp \
  "$image" -c "$container_script" >"$output/docker-tests.stdout" 2>"$output/docker-tests.stderr"
docker_status=$?
set -e
printf '%s\n' "$docker_status" >"$output/docker-tests.exit-status.txt"
snapshot "$output/source-after.txt"
cmp -s "$output/source-before.txt" "$output/source-after.txt" || fail 'source changed during native tests'
sha256sum "$target_dir/x86_64-unknown-linux-musl/verification/xsh" \
  "$target_dir/x86_64-unknown-linux-musl/verification/xsht" \
  "$target_dir/x86_64-unknown-linux-musl/verification/xshi" >"$output/products-after.sha256"
cmp -s "$output/products-before.sha256" "$output/products-after.sha256" || fail 'verification products changed during native tests'
if (( docker_status != 0 )); then
  printf 'native test runner infrastructure exited %s; inspect %s\n' "$docker_status" "$output" >&2
  exit "$docker_status"
fi
awk -F '\t' 'BEGIN { pass=0; fail=0 } $1 == 0 { pass++; next } { fail++ } END { printf "passed=%d failed=%d total=%d\n", pass, fail, pass+fail }' "$output/summary.tsv"
printf 'evidence=%s\n' "$output"
