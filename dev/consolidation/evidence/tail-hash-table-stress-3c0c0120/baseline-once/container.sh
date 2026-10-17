set -Eeuo pipefail
readonly output=/scratch
readonly product_dir=/target
readonly selector=core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_name_hash_table_stress
readonly test_path=${selector%%::*}
readonly expected_xsh=2576641bd019ea1ba46fa555e351668407dbbacabdf4d87cabcf63de32b5b017
readonly expected_xsht=5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836
sha256sum -c "$output/products-container-expected.sha256"
sha256sum "$product_dir/xsh" "$product_dir/xsht" >"$output/products-container-before.sha256"
unset XSH_MODULE_PATH || true
export PATH="$product_dir:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
if "$product_dir/xsht" test --exact "$selector" >"$output/written.stdout" 2>"$output/written.stderr"; then written_status=0; else written_status=$?; fi
printf '%s\t%s\n' "$written_status" "$selector" >"$output/summary.tsv"
workspace="$output/desugared-workspace"
mkdir -p "$workspace"
cp -a /work/core "$workspace/core"
cp /work/xsht-config.ini "$workspace/xsht-config.ini"
ln -s /work/dev "$workspace/dev"
ln -s /work/tests "$workspace/tests"
ln -s /work/showcase "$workspace/showcase"
if (cd "$workspace" && "$product_dir/xsht" desugar "$test_path") >"$output/desugared-source.xsh" 2>"$output/desugar.stderr"; then desugar_status=0; else desugar_status=$?; fi
printf '%s\n' "$desugar_status" >"$output/desugar.exit-status.txt"
if (( desugar_status != 0 )); then exit "$desugar_status"; fi
cp "$output/desugared-source.xsh" "$workspace/$test_path"
if (cd "$workspace" && "$product_dir/xsht" test --exact "$selector") >"$output/desugared.stdout" 2>"$output/desugared.stderr"; then desugared_status=0; else desugared_status=$?; fi
printf '%s\t%s\n' "$desugared_status" "$selector" >>"$output/summary.tsv"
printf '%s\n' "$written_status" >"$output/written.exit-status.txt"
printf '%s\n' "$desugared_status" >"$output/desugared.exit-status.txt"
for stream in written desugared; do
  grep -Fq 'running 1 tests' "$output/$stream.stdout"
done
grep -Fq 'tail: 9: file truncated' "$output/desugared.stdout"
grep -Fq "test_uu_tail_follow_name_hash_table_stress ... FAILED" "$output/desugared.stdout"
sha256sum "$product_dir/xsh" "$product_dir/xsht" >"$output/products-container-after.sha256"
cmp -s "$output/products-container-before.sha256" "$output/products-container-after.sha256"
awk -F '\t' 'NR == 1 && ($1 != 0 || NF != 2) { bad=1 } NR == 2 && ($1 != 1 || NF != 2) { bad=1 } END { if (NR != 2) bad=1; exit bad }' "$output/summary.tsv"
