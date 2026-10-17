set -u
run_case() {
  case_name=$1
  selector=$2
  "$XSHT_BIN" test --exact "$selector" > "$TAIL_EVIDENCE_DIR/$case_name.stdout" 2> "$TAIL_EVIDENCE_DIR/$case_name.stderr"
  case_status=$?
  printf '%s\n' "$case_status" > "$TAIL_EVIDENCE_DIR/$case_name.exit"
}
run_case gnu-debug core/tests/test-gnu-tail.xsh::test_gnu_tail_debug_log
gnu_status=$(cat "$TAIL_EVIDENCE_DIR/gnu-debug.exit")
run_case follow-banners core/tests/test-tail.xsh::test_tail_follow_name_switches_banners_between_files
banners_status=$(cat "$TAIL_EVIDENCE_DIR/follow-banners.exit")
if [ "$gnu_status" -eq 0 ] || [ "$banners_status" -eq 0 ]; then
  exit 1
fi
exit 0
