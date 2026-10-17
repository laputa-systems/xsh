set -euo pipefail
xsht test --nocapture --exact core/tests/test-timeout.xsh::timeout_stops_an_xsh_child_blocked_in_stdin_read
