#!/bin/bash
set -u

TERMINATION_GRACE_SECONDS=5
outputs=()
while (($#)); do
  case "$1" in
    --output)
      if (($# < 2)) || [[ "$2" != /* ]]; then
        printf 'container build output must be an absolute path\n' >&2
        exit 2
      fi
      outputs+=("$2")
      shift 2
      ;;
    --)
      shift
      break
      ;;
    *)
      printf 'unknown container build argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if ((${#outputs[@]} == 0)) || (($# == 0)); then
  printf 'container build needs output paths and a command\n' >&2
  exit 2
fi

uid=${TASK_UID-}
gid=${TASK_GID-}
if [[ ! "$uid" =~ ^[0-9]+$ ]] || [[ ! "$gid" =~ ^[0-9]+$ ]]; then
  printf 'container build needs numeric TASK_UID and TASK_GID values\n' >&2
  exit 2
fi

child=

finish() {
  local status=$?
  local cleanup_status=0
  local output
  trap - EXIT
  trap '' INT TERM HUP

  for output in "${outputs[@]}"; do
    if [[ -L "$output" ]]; then
      printf 'container build output is a symbolic link: %s\n' "$output" >&2
      cleanup_status=1
    elif [[ -e "$output" ]]; then
      if [[ ! -d "$output" ]]; then
        printf 'container build output is not a directory: %s\n' "$output" >&2
        cleanup_status=1
      else
        chown -Rh -- "$uid:$gid" "$output"
        current_status=$?
        if ((current_status != 0)); then
          printf 'output ownership cleanup failed for %s with status %s\n' "$output" "$current_status" >&2
          if ((cleanup_status == 0)); then
            cleanup_status=$current_status
          fi
        fi
      fi
    fi
  done

  if ((status == 0 && cleanup_status != 0)); then
    status=$cleanup_status
  fi
  exit "$status"
}

interrupt() {
  local status=$1
  local process_group
  local watchdog
  trap '' INT TERM HUP
  if [[ -n "$child" ]]; then
    process_group=$child
    if ! kill -TERM -- "-$process_group" 2>/dev/null && kill -0 -- "-$process_group" 2>/dev/null; then
      printf 'failed to stop container build process group %s\n' "$process_group" >&2
    fi
    (
      trap - INT TERM HUP
      timer=
      trap 'if [[ -n "$timer" ]]; then kill "$timer" 2>/dev/null || true; wait "$timer" 2>/dev/null || true; fi; exit 0' TERM
      sleep "$TERMINATION_GRACE_SECONDS" &
      timer=$!
      wait "$timer"
      kill -KILL -- "-$process_group" 2>/dev/null || true
    ) &
    watchdog=$!
    wait "$child"
    if kill -0 -- "-$process_group" 2>/dev/null; then
      kill -KILL -- "-$process_group" 2>/dev/null || true
    fi
    kill "$watchdog" 2>/dev/null || true
    wait "$watchdog" 2>/dev/null || true
    child=
  fi
  exit "$status"
}

trap finish EXIT
trap 'interrupt 129' HUP
trap 'interrupt 130' INT
trap 'interrupt 143' TERM
setsid "$@" &
child=$!
wait "$child"
status=$?
child=
exit "$status"
