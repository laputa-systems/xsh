set -euo pipefail
run_selector() {
  selector=$1
  logfile="$TIMEOUT_EVIDENCE_DIR/${selector##*::}.log"
  xsht test --nocapture --exact "$selector" 2>&1 | tee "$logfile"
}
run_selector core/tests/test-timeout.xsh::timeout_stops_a_cpu_bound_xsh_child_and_runs_its_cleanup
run_selector core/tests/test-timeout.xsh::timeout_stops_an_xsh_child_blocked_in_stdin_read
run_selector core/tests/test-timeout.xsh::timeout_sends_selected_signal_and_preserves_status
run_selector core/tests/test-timeout.xsh::timeout_forwards_interrupt_signals_and_reports_shell_status
