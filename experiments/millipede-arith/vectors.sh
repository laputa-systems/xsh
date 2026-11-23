#!/bin/sh
set -eu
cd "$(dirname "$0")"
export TMPDIR="$PWD/../../.work/tmp"
mkdir -p "$TMPDIR"
lake env lean --run Vectors.lean
.lake/millipede-arith < .lake/requests.bin > .lake/actual.bin
cmp .lake/expected.bin .lake/actual.bin
printf '%s\n' 'all vectors match Xsh.Core.Arith.checked'
