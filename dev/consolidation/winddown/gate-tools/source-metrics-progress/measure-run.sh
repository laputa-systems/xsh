#!/usr/bin/env bash
set +e
TASK_ART=/home/josh/d/laputa-systems/xsh/.work/final-review-evidence/source-metrics-progress
TASK_ROOT=/home/josh/d/laputa-systems/xsh
TASK_INFRA=/home/josh/d/laputa-systems/xsh-consolidation/.work/consolidation
TASK_BIN=$TASK_INFRA/restart/master-7fe-bin
TASK_WRAPPER=$TASK_INFRA/infra/run-native.sh
TASK_START=$(date +%s)
printf 'started_utc=%s\n' "$(date -u +%FT%TZ)" > "$TASK_ART/full-measure.log"
/usr/bin/time -p timeout -k 10 900 "$TASK_WRAPPER" "$TASK_ROOT" run /usr/bin/env "XSH_BIN=$TASK_BIN/xsh" "PATH=$TASK_BIN:/usr/local/bin:/usr/bin:/bin" "$TASK_BIN/xsh" "$TASK_ROOT/dev/consolidation/metrics.xsh" measure --root "$TASK_ROOT" --source-root src --xsht-source-root crates/xsht/src --output "$TASK_ART/current-report.json" --item-inventory "$TASK_ART/current-item-allowances.json" >> "$TASK_ART/full-measure.log" 2>&1
TASK_RESULT=$?
TASK_END=$(date +%s)
printf 'exit_code=%s\nelapsed_seconds=%s\nended_utc=%s\n' "$TASK_RESULT" "$((TASK_END - TASK_START))" "$(date -u +%FT%TZ)" >> "$TASK_ART/full-measure.log"
exit 0
