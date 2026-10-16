#!/bin/sh
set -eu

if [ "$#" -lt 1 ]; then
  printf '%s\n' 'cargo test runner: missing test executable' >&2
  exit 125
fi

if [ -z "${XSH_CARGO_TEST_LOCK_FILE-}" ]; then
  printf '%s\n' 'cargo test runner: missing Cargo lock path' >&2
  exit 125
fi

lock_fd=
lock_fd_count=0
for descriptor in /proc/self/fd/*; do
  [ -e "$descriptor" ] || continue
  if target=$(readlink -- "$descriptor" 2>/dev/null) && [ "$target" = "$XSH_CARGO_TEST_LOCK_FILE" ]; then
    lock_fd=${descriptor##*/}
    lock_fd_count=$((lock_fd_count + 1))
  fi
done

if [ "$lock_fd_count" -ne 1 ]; then
  printf 'cargo test runner: expected one inherited Cargo lock descriptor, found %s\n' "$lock_fd_count" >&2
  exit 125
fi

case "$lock_fd" in
  ''|*[!0-9]*)
    printf '%s\n' 'cargo test runner: inherited Cargo lock descriptor is not numeric' >&2
    exit 125
    ;;
esac

if [ "$lock_fd" -lt 3 ]; then
  printf '%s\n' 'cargo test runner: inherited Cargo lock descriptor is a standard stream' >&2
  exit 125
fi

unset XSH_CARGO_TEST_LOCK_FILE
unset CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUNNER CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER
if ! flock -u "$lock_fd"; then
  printf '%s\n' 'cargo test runner: failed to release the inherited Cargo lock' >&2
  exit 125
fi
if ! eval "exec ${lock_fd}>&-"; then
  printf '%s\n' 'cargo test runner: failed to close the inherited Cargo lock descriptor' >&2
  exit 125
fi
exec "$@"
