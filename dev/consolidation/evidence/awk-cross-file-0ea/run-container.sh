#!/bin/bash
set -euo pipefail
TASK_OUT=/scratch/awk-original-0ea-replay/runtime
TASK_TEMP=$TASK_OUT/native-temp
TASK_SOURCE=/replay
TASK_XSHT=/work/target/x86_64-unknown-linux-musl/release/xsht
mkdir -p "$TASK_OUT/evidence" "$TASK_TEMP"
TASK_EVIDENCE=$TASK_OUT/evidence
cd "$TASK_SOURCE"
if [ "${XSH_MODULE_PATH+x}" = x ]; then printf 'XSH_MODULE_PATH=set\n' > "$TASK_EVIDENCE/module-path.txt"; else printf 'XSH_MODULE_PATH=unset\n' > "$TASK_EVIDENCE/module-path.txt"; fi
unset XSH_MODULE_PATH
{
  id
  printf 'LC_ALL=%s\nTZ=%s\nTMPDIR=%s\n' "${LC_ALL-unset}" "${TZ-unset}" "${TMPDIR-unset}"
  printf 'nproc='; nproc
  printf 'cpu.max='; cat /sys/fs/cgroup/cpu.max
  printf 'cpuset.cpus.effective='; cat /sys/fs/cgroup/cpuset.cpus.effective
  printf 'memory.max='; cat /sys/fs/cgroup/memory.max
  printf 'memory.swap.max='; cat /sys/fs/cgroup/memory.swap.max
} > "$TASK_EVIDENCE/runtime-context.txt"
sha256sum /work/target/x86_64-unknown-linux-musl/release/xsh /work/target/x86_64-unknown-linux-musl/release/xsht > "$TASK_EVIDENCE/runtime-binaries.sha256"
sha256sum core/tests/test-awk.xsh core/tests/test-curl.xsh core/tests/data/awk/matrix/streams_and_processes.json > "$TASK_EVIDENCE/source-written.sha256"
cp core/tests/test-awk.xsh "$TASK_EVIDENCE/test-awk.written.xsh"
record_paths() {
  TASK_LABEL=$1
  python3 - "$TASK_EVIDENCE/$TASK_LABEL.lstat" /nonexistent /nonexistent/dir /nonexistent/dir/out /nonexistent/dir/f <<'PY'
import os
import stat
import sys
with open(sys.argv[1], "w", encoding="utf-8") as output:
    for path in sys.argv[2:]:
        try:
            item = os.lstat(path)
            output.write(f"{path}: {stat.filemode(item.st_mode)} uid={item.st_uid} gid={item.st_gid} size={item.st_size}\n")
        except OSError as error:
            output.write(f"{path}: {type(error).__name__} errno={error.errno}\n")
PY
}
run_test() {
  TASK_LABEL=$1
  TASK_FILTER=$2
  TASK_TRACE=$3
  set +e
  if [ "$TASK_TRACE" = yes ]; then
    strace -f -yy -s 8192 -e trace=execve,openat,write,writev,exit_group -o "$TASK_EVIDENCE/$TASK_LABEL.strace" "$TASK_XSHT" test --jobs 8 --exact "$TASK_FILTER" > "$TASK_EVIDENCE/$TASK_LABEL.stdout" 2> "$TASK_EVIDENCE/$TASK_LABEL.stderr"
  else
    "$TASK_XSHT" test --jobs 8 --exact "$TASK_FILTER" > "$TASK_EVIDENCE/$TASK_LABEL.stdout" 2> "$TASK_EVIDENCE/$TASK_LABEL.stderr"
  fi
  TASK_STATUS=$?
  set -e
  printf '%s\n' "$TASK_STATUS" > "$TASK_EVIDENCE/$TASK_LABEL.status"
}
record_paths before_written_awk
run_test written_awk_001 core/tests/test-awk.xsh::test_awk_matrix_streams_and_processes_shard_001 yes
record_paths after_written_awk
run_test written_curl_failure core/tests/test-curl.xsh::test_curl_write_failure_exits_23 no
record_paths after_written_curl
set +e
"$TASK_XSHT" desugar core/tests/test-awk.xsh > "$TASK_EVIDENCE/test-awk.desugared.xsh" 2> "$TASK_EVIDENCE/desugar.stderr"
TASK_DESUGAR_STATUS=$?
set -e
printf '%s\n' "$TASK_DESUGAR_STATUS" > "$TASK_EVIDENCE/desugar.status"
if [ "$TASK_DESUGAR_STATUS" -ne 0 ]; then exit "$TASK_DESUGAR_STATUS"; fi
cp "$TASK_EVIDENCE/test-awk.desugared.xsh" core/tests/test-awk.xsh
sha256sum core/tests/test-awk.xsh core/tests/test-curl.xsh core/tests/data/awk/matrix/streams_and_processes.json > "$TASK_EVIDENCE/source-transformed.sha256"
run_test desugared_awk_001 core/tests/test-awk.xsh::test_awk_matrix_streams_and_processes_shard_001 yes
record_paths after_desugared_awk
if [ -e /nonexistent/dir/f ]; then cp /nonexistent/dir/f "$TASK_EVIDENCE/awk-case-11-target"; sha256sum /nonexistent/dir/f > "$TASK_EVIDENCE/awk-case-11-target.sha256"; fi
{
  cat /sys/fs/cgroup/cpu.stat
  cat /sys/fs/cgroup/memory.peak
  cat /sys/fs/cgroup/memory.events
} > "$TASK_EVIDENCE/runtime-resource-after.txt"
chown -R "$HOST_UID:$HOST_GID" "$TASK_OUT"
