# Interpreter Hyperfine Scenarios

These scripts are stable CLI-facing counterparts to the in-process Criterion
`interpreter` group. They measure `xsh` startup, parse/check, and evaluation
together, so use them for end-to-end interpreter comparisons rather than
isolated evaluator timing. The local snapshot is tracked in
`../interpreter-hyperfine-baseline.json`.

Static lowered-IR surface coverage is reported by:

```sh
target/release/xsh tools/xsh-ir-coverage.xsh -- --json target/perf/ir-coverage.json
```

The report compares `src/syntax/ast.rs` enum variants against the current
lowered capability map, extracts the lowered method whitelist from
`src/runtime/eval.rs`, and conservatively scans pure functions, restricted proc
bodies, and executable top-level script statements across the current XSH
corpus: this repository, `../packages`, and `../laputa`. Top-level scanning is
region-oriented for obvious continuations such as multiline structured
pipelines, list literals, and argument lists, and it skips triple-quoted string
contents and proc/pure definition bodies. Treat the percentages as expansion
maps, not whole-language runtime coverage. Each lowerability section reports
raw fallback reasons and grouped summaries so import/runtime/effect boundaries
are visible separately from method, statement, expression, and type coverage
gaps.

Run one scenario:

```sh
hyperfine --warmup 3 'target/release/xsh perf/interpreter/method-dispatch-5k.xsh >/dev/null'
```

Run the full set:

```sh
hyperfine --warmup 3 --export-json target/perf/interpreter-hyperfine.json \
  'target/release/xsh perf/interpreter/fib-20.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/loop-sum-10k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/method-dispatch-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/record-map-2k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/result-propagation-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/result-fallback-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/result-context-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/error-helper-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/pure-loop-20k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/pure-call-chain-20k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/pure-result-validate-10k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/stream-pipeline-2k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/stream-callback-pure-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/text-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/text-lines-for-ir-glue-10k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/count-lines-ir-glue-20k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/text-scanner-ir-10k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/hash-scanner-unindented-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/hash-scanner-marker-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/html-scanner-ir-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/slash-scanner-plain-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/slash-scanner-nested-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/list-push-assignment-ir-10k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/map-set-assignment-ir-10k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/ast-self-assignment-glue-10k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/report-assembly-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/scan-aggregation-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/nested-record-field-glue-20k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/lang-dispatch-glue-20k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/count-language-dispatch-glue-20k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/ext-filter-glue-20k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/direct-scanner-dispatch-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/record-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/collection-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/collection-helpers-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/path-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/nominal-record-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/pipeline-slice-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/pipeline-filter-map-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/text-lines-pipeline-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/group-by-pipeline-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/tag-union-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/enumerate-list-comp-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/pipeline-sort-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/regex-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/status-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/qualified-pure-ir-glue-5k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/script-ir-top-level-glue-10k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/json-record-glue-1k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/mixed-glue-2k.xsh >/dev/null' \
  'target/release/xsh perf/interpreter/mixed-proc-ir-glue-2k.xsh >/dev/null'
```
