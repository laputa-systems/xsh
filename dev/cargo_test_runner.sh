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

test_executable=$1
shift

test_filter=0
passthrough_request=0
unsupported_option=0
expects_option_value=0
for argument do
  if [ "$expects_option_value" -eq 1 ]; then
    expects_option_value=0
    continue
  fi

  case "$argument" in
    --help|-h|--version|-V|--list|--list=*)
      passthrough_request=1
      ;;
    --format|--color|--logfile|--shuffle-seed|--skip|--test-threads|-Z)
      expects_option_value=1
      ;;
    --format=*|--color=*|--logfile=*|--report-time=*|--shuffle-seed=*|--skip=*|--test-threads=*)
      ;;
    --bench)
      passthrough_request=1
      ;;
    --exact|--ignored|--include-ignored|--nocapture|--no-capture|--show-output|--quiet|-q|--shuffle|--test|--fail-fast|--exclude-should-panic|--force-run-in-process|--report-time|--ensure-time)
      ;;
    --)
      ;;
    --*)
      unsupported_option=1
      ;;
    -*)
      unsupported_option=1
      ;;
    '')
      ;;
    *)
      test_filter=1
      ;;
  esac
done

if [ "$passthrough_request" -eq 1 ] || [ "$test_filter" -eq 0 ]; then
  exec "$test_executable" "$@"
fi

if [ "$unsupported_option" -eq 1 ]; then
  printf '%s\n' 'cargo test runner: cannot verify a filter with an unsupported harness option' >&2
  exit 125
fi

listing=$("$test_executable" --list "$@")
case "$listing" in
  *': test'*)
    ;;
  *)
    if printf '%s\n' "$listing" | grep -Eq '"event"[[:space:]]*:[[:space:]]*"discovered"'; then
      :
    else
      case "$listing" in
        *'0 tests, 0 benchmarks'*|*'0 test, 0 benchmarks'*)
          printf '%s\n' 'cargo test runner: no tests selected by forwarded filter' >&2
          exit 125
          ;;
        *)
          printf '%s\n' 'cargo test runner: could not verify selected tests' >&2
          exit 125
          ;;
      esac
    fi
    ;;
esac

exec "$test_executable" "$@"
