#!/bin/sh
set -u

mode=$1
pair=$2
label=$3
bin="/opt/$pair"
output="/results/$label.log"

case "$mode" in
  list)
    xsht_command="$bin/xsht test --list"
    ;;
  suite)
    xsht_command="$bin/xsht test --timeout 120s"
    ;;
  help)
    xsht_command="$bin/xsht test --help"
    ;;
  *)
    printf 'unknown mode: %s\n' "$mode" >&2
    exit 2
    ;;
esac

{
  printf 'uid=%s\ngid=%s\nmode=%s\npair=%s\nlabel=%s\n' \
    "$(id -u)" "$(id -g)" "$mode" "$pair" "$label"
  printf 'command=%s\n' "$xsht_command"
  printf 'HOME=%s\nXSH_BIN=%s\nXSH_MODULE_PATH=%s\nPATH=%s\n' \
    "$HOME" "$XSH_BIN" "$XSH_MODULE_PATH" "$PATH"
  sed -n 's/^Cpus_allowed_list:[[:space:]]*/cpus_allowed_list=/p' /proc/self/status
} >"/results/$label.environment.txt"

capture_resources() {
  for stat in cpu.stat cpu.max cpuset.cpus.effective memory.max memory.current memory.peak memory.swap.current memory.swap.max memory.events pids.current pids.peak pids.max; do
    path="/sys/fs/cgroup/$stat"
    printf '[%s]\n' "$path"
    if [ -r "$path" ]; then
      cat "$path"
    else
      printf 'unavailable\n'
    fi
  done
}

child_pid=0
forward_interrupt() {
  if [ "$child_pid" -gt 0 ]; then
    kill -INT "$child_pid" 2>/dev/null || true
  fi
}

run_xsht() {
  "$@" >"$output" 2>&1 &
  child_pid=$!
  wait "$child_pid"
  status=$?
  while kill -0 "$child_pid" 2>/dev/null; do
    wait "$child_pid"
    status=$?
  done
  child_pid=0
}

trap forward_interrupt INT
capture_resources >"/results/$label.cgroup-before.txt"
case "$mode" in
  list)
    run_xsht "$bin/xsht" test --list
    ;;
  suite)
    run_xsht "$bin/xsht" test --timeout 120s
    ;;
  help)
    run_xsht "$bin/xsht" test --help
    ;;
esac
trap - INT
capture_resources >"/results/$label.cgroup-after.txt"
printf 'exit_code=%s\n' "$status" >>"/results/$label.cgroup-after.txt"
printf '%s\n' "$status" >"/results/$label.exit-code"
exit "$status"
