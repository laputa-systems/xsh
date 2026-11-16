#!/bin/sh
set -eu
cidfile=$1
control=$2
shift 2
"$@" &
child=$!
trap 'kill "$child" 2>/dev/null || :; if [ -s "$cidfile" ]; then docker rm -f "$(cat "$cidfile")" >/dev/null 2>&1 || :; fi; wait "$child" 2>/dev/null || :' EXIT HUP INT TERM
attempt=0
while [ ! -e "$control/ready" ]; do
  if ! kill -0 "$child" 2>/dev/null; then
    wait "$child"
    exit 1
  fi
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 36000 ]; then
    printf '%s\n' 'split test did not reach the quota handshake' >&2
    exit 1
  fi
  sleep 0.1
done
docker update --cpus=0.01 "$(cat "$cidfile")" >/dev/null
: > "$control/go"
wait "$child"
trap - EXIT HUP INT TERM
