#!/bin/sh
set -eu
scratch=$1
mkdir -p "$scratch/unrelated"
python3 /scratch/rename-worker-ready.py "$scratch/unrelated" 10000000 "$scratch/renamer.ready" "$scratch/renamer.done" >"$scratch/renamer.log" 2>&1 &
renamer=$!
cleanup() {
  kill "$renamer" 2>/dev/null || true
  wait "$renamer" 2>/dev/null || true
}
trap cleanup EXIT INT TERM
i=0
while [ ! -e "$scratch/renamer.ready" ] && [ "$i" -lt 200 ]; do
  sleep 0.01
  i=$((i + 1))
done
if [ ! -e "$scratch/renamer.ready" ]; then
  cat "$scratch/renamer.log" >&2
  printf 'renamer did not become ready\n' >&2
  exit 2
fi
if /usr/bin/strace -f -qq -e trace=openat2,readlinkat -e status=failed -o "$scratch/native.trace" /products/xsht test --exact dev/tests/test-system-report-check.xsh::test_system_report_pci_capture_replays_raw_identity_links_and_rejects_tampering; then
  status=0
else
  status=$?
fi
printf 'native_xsht_exit=%s\n' "$status"
if [ -e "$scratch/renamer.done" ]; then
  printf 'renamer_completed=1\n'
else
  printf 'renamer_completed=0\n'
fi
exit "$status"
