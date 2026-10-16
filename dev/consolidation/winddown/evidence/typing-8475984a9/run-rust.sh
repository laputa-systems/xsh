#!/usr/bin/env bash
set -euo pipefail
TASK_ROOT=/home/josh/d/laputa-systems/xsh
TASK_TREE=$TASK_ROOT/.work/wt/typing-pipeline-integration
TASK_SCRATCH=$TASK_ROOT/.work/tmp/typing-ownership-debug
TASK_IMAGE=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
TASK_BINARY=$TASK_ROOT/.work/target-typing-pipeline/x86_64-unknown-linux-musl/debug/build/xsh/692c8c084af82ba2/out/xsh-692c8c084af82ba2
TASK_SOURCE_PIN=8475984a9ce1e8dbc987a8cf409f0747583c4878
TASK_SELECTORS=$TASK_SCRATCH/final-source-8475984a9-289-selectors.txt
TASK_LOG=$TASK_SCRATCH/logs/final-source-8475984a9-289-exact.stdout
TASK_RUN_LOG=$TASK_SCRATCH/logs/final-source-8475984a9-289-tests.log
TASK_STDERR=$TASK_SCRATCH/logs/final-source-8475984a9-289-exact.stderr
TASK_EXIT=$TASK_SCRATCH/logs/final-source-8475984a9-289-exact.exit
TASK_COUNT=$(wc -l < "$TASK_SELECTORS")
TASK_HEAD=$(git -C "$TASK_TREE" rev-parse HEAD)
if [ "$TASK_HEAD" != "$TASK_SOURCE_PIN" ]; then
  printf 'source pin mismatch: expected %s, found %s\n' "$TASK_SOURCE_PIN" "$TASK_HEAD" >&2
  exit 2
fi
if [ "$TASK_COUNT" -ne 289 ]; then
  printf 'expected 288 selectors, found %s\n' "$TASK_COUNT" >&2
  exit 2
fi
set +e
docker run --rm --init --platform linux/amd64 --cpus=8 --memory=16g --memory-swap=16g --network=none \
  -v "$TASK_TREE:$TASK_TREE" \
  -v "$TASK_ROOT/.work/target-typing-pipeline:$TASK_ROOT/.work/target-typing-pipeline:ro" \
  -v "$TASK_SCRATCH:$TASK_SCRATCH" \
  -v "$TASK_ROOT/.git:$TASK_ROOT/.git:ro" \
  -w "$TASK_TREE" \
  -e TMPDIR="$TASK_SCRATCH/tmp" \
  -e LC_ALL=C -e TZ=UTC \
  "$TASK_IMAGE" /bin/bash -c '
    set -euo pipefail
    task_binary=$1
    task_selectors=$2
    task_log=$3
    : >"$task_log"
    task_count=0
    while IFS= read -r task_selector; do
      task_result=$("$task_binary" "$task_selector" --exact --quiet 2>&1) || {
        printf "selector failed: %s\n%s\n" "$task_selector" "$task_result" >&2
        exit 1
      }
      case "$task_result" in
        *"1 passed; 0 failed"*) ;;
        *)
        printf "selector did not run exactly one test: %s\n%s\n" "$task_selector" "$task_result" >&2
        exit 1
        ;;
      esac
      printf "%s\n" "$task_result" >>"$task_log"
      task_count=$((task_count + 1))
    done < "$task_selectors"
    printf "selected=%s\n" "$task_count"
  ' bash "$TASK_BINARY" "$TASK_SELECTORS" "$TASK_RUN_LOG" \
  >"$TASK_LOG" 2>"$TASK_STDERR"
TASK_STATUS=$?
printf '%s\n' "$TASK_STATUS" >"$TASK_EXIT"
printf 'source=%s selectors=%s exit=%s\n' "$TASK_HEAD" "$TASK_COUNT" "$TASK_STATUS"
exit "$TASK_STATUS"
