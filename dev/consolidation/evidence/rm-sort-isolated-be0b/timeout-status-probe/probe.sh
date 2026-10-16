#!/bin/sh
set +e
probe=/work/.work/tmp/status3-probe-7b
xsh=/work/target/x86_64-unknown-linux-musl/verification/xsh
rm_script=/work/core/rm.xsh
timeout_script=/work/core/timeout.xsh
launcher=/work/core/tests/support/uu-launch.xsh
metadata='{"caller_keys":[],"umask":null}'
printf 'timeout_path=%s\n' "$(command -v timeout)"
timeout --help | head -n 1
/bin/sh -c 'exec timeout 3s "$@"' rm-wrapper "$xsh" "$launcher" "$metadata" "$xsh" "$probe/exit-three.xsh" -- >"$probe/child-exit3.stdout" 2>"$probe/child-exit3.stderr"
printf '%s\n' "$?" >"$probe/child-exit3.status"
/bin/sh -c 'exec timeout 1s /bin/sleep 2' >"$probe/external-timeout.stdout" 2>"$probe/external-timeout.stderr"
printf '%s\n' "$?" >"$probe/external-timeout.status"
"$xsh" "$timeout_script" 0s /bin/sh -c 'exit 3' >"$probe/xsh-timeout-exit3.stdout" 2>"$probe/xsh-timeout-exit3.stderr"
printf '%s\n' "$?" >"$probe/xsh-timeout-exit3.status"
"$xsh" "$timeout_script" 1s /bin/sleep 2 >"$probe/xsh-timeout-expired.stdout" 2>"$probe/xsh-timeout-expired.stderr"
printf '%s\n' "$?" >"$probe/xsh-timeout-expired.status"
mkdir -p "$probe/rmroot/d"
: >"$probe/rmroot/d/1"
: >"$probe/rmroot/d/2"
: >"$probe/rmroot/d/3"
/bin/sh -c 'exec timeout 3s "$@"' rm-wrapper "$xsh" "$launcher" "$metadata" "$xsh" "$rm_script" -- -rf "$probe/rmroot/d" >"$probe/rm.stdout" 2>"$probe/rm.stderr"
printf '%s\n' "$?" >"$probe/rm.status"
python3 "$probe/signal_probe.py" "$xsh" "$timeout_script" "$probe"
for case in child-exit3 external-timeout xsh-timeout-exit3 xsh-timeout-expired rm timeout-signal; do
  printf '%s status=%s stdout_bytes=%s stderr_bytes=%s\n' "$case" "$(cat "$probe/$case.status")" "$(wc -c <"$probe/$case.stdout")" "$(wc -c <"$probe/$case.stderr")"
done
