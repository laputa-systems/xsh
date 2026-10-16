#!/bin/sh
set +e
probe=/work/.work/tmp/status3-probe-7b
xsh=/work/target/x86_64-unknown-linux-musl/verification/xsh
timeout_script=/work/core/timeout.xsh
launcher=/work/core/tests/support/uu-launch.xsh
metadata='{"caller_keys":[],"umask":null}'
/bin/sh -c 'exec timeout 1s "$@"' rm-wrapper "$xsh" "$launcher" "$metadata" "$xsh" "$probe/sleep-five.xsh" -- >"$probe/busybox-timeout-xsh.stdout" 2>"$probe/busybox-timeout-xsh.stderr"
printf '%s\n' "$?" >"$probe/busybox-timeout-xsh.status"
"$xsh" "$timeout_script" 1s "$xsh" "$probe/sleep-five.xsh" >"$probe/xsh-timeout-xsh.stdout" 2>"$probe/xsh-timeout-xsh.stderr"
printf '%s\n' "$?" >"$probe/xsh-timeout-xsh.status"
for case in busybox-timeout-xsh xsh-timeout-xsh; do
  printf '%s status=%s stdout_bytes=%s stderr_bytes=%s\n' "$case" "$(cat "$probe/$case.status")" "$(wc -c <"$probe/$case.stdout")" "$(wc -c <"$probe/$case.stderr")"
done
