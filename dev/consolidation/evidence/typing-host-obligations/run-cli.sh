#!/usr/bin/env bash
set -Eeuo pipefail
out=/scratch/logs/ordinary-cli
mkdir -p "$out"
positives=(domain-controls-annotated trim-result-annotated trim-result-reduced trim-chain-annotated trim-chain-reduced shared-receiver-annotated shared-receiver-reduced reordered-annotated reordered-reduced context-result byte-str-annotated byte-bytes-annotated)
for name in "${positives[@]}"; do
  /products/xsht check "/inputs/$name.xsh" > "$out/$name.check.stdout" 2> "$out/$name.check.stderr"
  /products/xsh "/inputs/$name.xsh" > "$out/$name.run.stdout" 2> "$out/$name.run.stderr"
  printf '%s check=0 run=0\n' "$name"
done
negatives=(ambiguous-record-map-keys:check.ambiguous-overload uninferred-numeric-domain:check.local-inference ambiguous-replace-str:check.ambiguous-overload ambiguous-replace-regex:check.ambiguous-overload ambiguous-byte:check.ambiguous-overload invalid-argument:check.type-mismatch invalid-effect:check.pure-effect invalid-dynamic:check.dynamic-boundary generic-row-body-gap:check.local-inference)
for entry in "${negatives[@]}"; do
  name=${entry%%:*}
  code=${entry#*:}
  for operation in check run; do
    set +e
    if [ "$operation" = check ]; then
      /products/xsht check "/inputs/$name.xsh" > "$out/$name.$operation.stdout" 2> "$out/$name.$operation.stderr"
    else
      /products/xsh "/inputs/$name.xsh" > "$out/$name.$operation.stdout" 2> "$out/$name.$operation.stderr"
    fi
    status=$?
    set -e
    test "$status" -eq 2
    grep -Fq "[$code]" "$out/$name.$operation.stderr"
    printf '%s %s=%s %s\n' "$name" "$operation" "$status" "$code"
  done
done
