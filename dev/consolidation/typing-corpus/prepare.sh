#!/usr/bin/env bash
set -Eeuo pipefail
lane=$(git rev-parse --show-toplevel)
scratch=$lane/.work/tmp/corpus
mkdir -p "$scratch/original/xsh" "$scratch/original/laputa" "$scratch/tmp"
[ -z "$(ls -A "$scratch/original/xsh")" ]
[ -z "$(ls -A "$scratch/original/laputa")" ]
git archive ddc17d3d53c819c343f9ecaa738ac67d59e9033a | tar -x -C "$scratch/original/xsh"
git -C /home/josh/d/laputa-systems/laputa archive ca658ce1da9d89c55209c25268dfdf687ff4fdc6 | tar -x -C "$scratch/original/laputa"
python3 "$lane/dev/consolidation/typing-corpus/cohort.py" "$scratch/original" --reduced "$scratch/reduced-final" --retain "$lane/dev/consolidation/typing-corpus/retained.tsv"
python3 "$lane/dev/consolidation/typing-corpus/apply_witness_fixture.py" "$scratch/original/xsh" "$scratch/reduced-final/xsh"
python3 "$lane/dev/consolidation/typing-corpus/validate.py" "$scratch/original" "$scratch/reduced-final"
