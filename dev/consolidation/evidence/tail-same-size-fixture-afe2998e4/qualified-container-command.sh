set -Eeuo pipefail
readonly output=/scratch
readonly product_dir=/target
readonly xsht=$product_dir/xsht
readonly expected_xsh=c137d1d1ccff63c3ae492af1106aebb1872ae23b3d0e2af1d017968a4aa19a6c
readonly expected_xsht=5dccc9f21f30b4b26e8f96a7815303dd3b7a13c94b132d421ad02cee3fada7e1
readonly selectors=(
  core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_when_files_are_pointing_to_same_relative_file_and_file_stays_same_size
  core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_detects_file_truncation
  core/tests/test-tail.xsh::test_tail_follow_descriptor_reports_truncation_and_reads_from_start
)

sha256sum "$product_dir/xsh" "$product_dir/xsht" >"$output/products-container-before.sha256"
printf '%s  %s\n' "$expected_xsh" "$product_dir/xsh" "$expected_xsht" "$product_dir/xsht" >"$output/products-container-expected.sha256"
sha256sum --check "$output/products-container-expected.sha256"
unset LD_PRELOAD || true
export PATH="$product_dir:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
: >"$output/selectors.txt"
: >"$output/summary.tsv"
index=0
for selector in "${selectors[@]}"; do
  index=$((index + 1))
  test_log=$(printf '%02d' "$index")
  printf '%s\n' "$selector" >>"$output/selectors.txt"
  if "$xsht" test --exact "$selector" >"$output/$test_log.stdout" 2>"$output/$test_log.stderr"; then status=0; else status=$?; fi
  printf '%s\t%s\n' "$status" "$selector" >>"$output/summary.tsv"
done
sha256sum "$product_dir/xsh" "$product_dir/xsht" >"$output/products-container-after.sha256"
cmp -s "$output/products-container-before.sha256" "$output/products-container-after.sha256" || exit 3
awk -F '\t' 'NF != 2 || $1 != 0 { bad=1 } END { if (NR != 3) bad=1; exit bad }' "$output/summary.tsv"
