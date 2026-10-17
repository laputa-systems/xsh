set -Eeuo pipefail
readonly output=/scratch
readonly product_dir=/target
readonly selectors=(
  core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_name_hash_table_stress
  core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_name_truncate2
  core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_name_truncate4
  core/tests/test-uu-tail-2.xsh::test_uu_tail_retry_descriptor_detects_truncation
)
sha256sum -c "$output/products-container-expected.sha256"
sha256sum "$product_dir/xsh" "$product_dir/xsht" >"$output/products-container-before.sha256"
unset XSH_MODULE_PATH || true
export PATH="$product_dir:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
rm -rf "$output/probe-workspace" "$output/desugared-workspace"
mkdir -p "$output/probe-workspace" "$output/desugared-workspace"
for workspace in "$output/probe-workspace" "$output/desugared-workspace"; do
  cp -a /work/core "$workspace/core"
  cp /work/xsht-config.ini "$workspace/xsht-config.ini"
  ln -s /work/dev "$workspace/dev"
  ln -s /work/tests "$workspace/tests"
  ln -s /work/showcase "$workspace/showcase"
done
cat "$output/probe-source.xsh" >>"$output/probe-workspace/core/tests/test-uu-tail-1.xsh"
probe_selector=core/tests/test-uu-tail-1.xsh::test_tail_hash_table_held_truncate_probe
if (cd "$output/probe-workspace" && timeout -k 5 30s "$product_dir/xsht" test --nocapture --exact "$probe_selector") >"$output/probe.stdout" 2>"$output/probe.stderr"; then probe_status=0; else probe_status=$?; fi
printf '%s\t%s\n' "$probe_status" "$probe_selector" >"$output/probe.tsv"
(( probe_status == 0 ))
grep -Fq 'running 1 tests' "$output/probe.stdout"
grep -Fq 'test_tail_hash_table_held_truncate_probe ... ok' "$output/probe.stdout"
grep -Fq 'in_place_identity=' "$output/probe.stdout"
grep -Fq 'held_identity=' "$output/probe.stdout"
for selector in "${selectors[@]}"; do
  name=${selector##*::}
  safe_name=${name#test_uu_tail_}
  if (cd /work && timeout -k 5 45s "$product_dir/xsht" test --exact "$selector") >"$output/written-$safe_name.stdout" 2>"$output/written-$safe_name.stderr"; then status=0; else status=$?; fi
  printf 'written\t%s\t%s\n' "$status" "$selector" >>"$output/summary.tsv"
  (( status == 0 ))
  grep -Fq 'running 1 tests' "$output/written-$safe_name.stdout"
  grep -Fq "$name ... ok" "$output/written-$safe_name.stdout"
done
for test_path in core/tests/test-uu-tail-1.xsh core/tests/test-uu-tail-2.xsh; do
  base=${test_path##*/}
  if (cd "$output/desugared-workspace" && timeout -k 5 45s "$product_dir/xsht" desugar "$test_path") >"$output/desugared-$base" 2>"$output/desugar-$base.stderr"; then desugar_status=0; else desugar_status=$?; fi
  printf '%s\n' "$desugar_status" >"$output/desugar-$base.exit-status.txt"
  (( desugar_status == 0 ))
  cp "$output/desugared-$base" "$output/desugared-workspace/$test_path"
done
for selector in "${selectors[@]}"; do
  name=${selector##*::}
  safe_name=${name#test_uu_tail_}
  if (cd "$output/desugared-workspace" && timeout -k 5 45s "$product_dir/xsht" test --exact "$selector") >"$output/desugared-$safe_name.stdout" 2>"$output/desugared-$safe_name.stderr"; then status=0; else status=$?; fi
  printf 'desugared\t%s\t%s\n' "$status" "$selector" >>"$output/summary.tsv"
  (( status == 0 ))
  grep -Fq 'running 1 tests' "$output/desugared-$safe_name.stdout"
  grep -Fq "$name ... ok" "$output/desugared-$safe_name.stdout"
done
sha256sum "$product_dir/xsh" "$product_dir/xsht" >"$output/products-container-after.sha256"
cmp -s "$output/products-container-before.sha256" "$output/products-container-after.sha256"
cat "$output/probe.tsv" "$output/summary.tsv"
