set -u
xsht=/target/xsht
selectors=(
  "core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_when_files_are_pointing_to_same_relative_file_and_file_stays_same_size"
  "core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_detects_file_truncation"
  "core/tests/test-tail.xsh::test_tail_follow_descriptor_reports_truncation_and_reads_from_start"
)
: > /scratch/summary.tsv
index=0
for selector in "${selectors[@]}"; do
  index=$((index + 1))
  number=$(printf '%02d' "$index")
  if "$xsht" test --exact "$selector" >"/scratch/$number.stdout" 2>"/scratch/$number.stderr"; then status=0; else status=$?; fi
  printf '%s\t%s\n' "$status" "$selector" >> /scratch/summary.tsv
done
awk -F '\t' '$1 != 0 { bad=1 } END { exit !bad }' /scratch/summary.tsv
