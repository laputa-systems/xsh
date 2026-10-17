#!/usr/bin/env bash
set -Eeuo pipefail
lane=/home/josh/d/laputa-systems/xsh/.work/wt/coverage-discovery-repair
phase=$1
filter=$2
cargo_log=$lane/.work/tmp/discovery/logs/$phase-build/cargo.stdout.jsonl
executable=$(jaq -r 'select(.reason == "compiler-artifact" and .target.name == "xsht" and .profile.test and .executable != null) | .executable' "$cargo_log" | tail -1)
test -n "$executable"
docker run --rm --init --platform linux/amd64 --user 1000:1000 --cpuset-cpus=0-3 --cpus=4 --memory=8g --memory-swap=8g --network=none \
  --volume "$lane:/work:ro" \
  --volume "$lane/.work/target-discovery:/target:ro" \
  --volume "$lane/.work/tmp/discovery:/scratch:rw" \
  --workdir /work --env TMPDIR=/scratch/tmp \
  xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  "$executable" "$filter" --nocapture --test-threads=1 \
  > "$lane/.work/tmp/discovery/logs/$phase-test.stdout" \
  2> "$lane/.work/tmp/discovery/logs/$phase-test.stderr"
