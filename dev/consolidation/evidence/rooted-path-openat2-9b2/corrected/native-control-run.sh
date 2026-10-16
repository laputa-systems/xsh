#!/bin/sh
set -eu
scratch=$1
if /usr/bin/strace -f -qq -e trace=openat2,readlinkat -e status=failed -o "$scratch/native.trace" /products/xsht test --exact dev/tests/test-system-report-check.xsh::test_system_report_pci_capture_replays_raw_identity_links_and_rejects_tampering; then
  status=0
else
  status=$?
fi
printf 'native_xsht_exit=%s\n' "$status"
exit "$status"
