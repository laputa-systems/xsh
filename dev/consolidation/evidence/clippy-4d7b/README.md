# Clippy integration qualification

Source `4d7b0d0d103ab2bff68bf0a019e7a172cb9ce6ae`, tree
`bf7a566fbf2ca782a4473307746cec9be01d7ff1`, is integrated at `c1c6e4c87`.
Main has identical Rust, test, and core source for this batch.

- [Strict Clippy](lint/exit-status.txt): exit 0, no diagnostics.
- [Builds](build/): debug library harness and verification XSH/XSHT, both exit 0.
- [Focused debug tests](debug/tests-runner.stdout): 40 exact selections pass.
- [Native selections](focused-gates.md): 23 tests pass across 16 commands.
- [Paired diagnostics](parity/diagnostic-byte-comparison.tsv): all eight commands
  exit 0. Stdout is empty. Diagnostic bytes match after removing exactly the
  final run-varying timing summary. Raw stderr is retained.
- [Source review](source-review.md) and [factory review](factory-review.md): GO.
- [Current metrics](structure/current-report.json): all four hard zeros remain.
  Counts are 152 instructions, 17 patterns, 46 stages, one predicate, two
  storage mappings, and five factories. Production source is 268,504 lines,
  25 above the qualified baseline.
- [Module trace](module-trace/result.json): one shared User/Str key has eight
  requests, one prepare, seven reuses, one check, and seven check hits.
  These are cache events, not body-traversal counts.
- [Synthetic compiler probe](feature-probe/result.md): 35 XSH edits and 19
  XSHT arms are demanded. Diagnostic commit `18c9a6c10` must never merge.

The two failed combined lint iterations remain under `failed-5c4e` and
`failed-ffa1`. The first debug run's eight route-fixture failures came from a
read-only `/work/.work/tmp` mount. `debug-mount-failure` retains that attempt;
`debug` retains the corrected 40-test run. The interrupted baseline metric
scan exited 3 and is unused. The retained 6198 report qualifies the baseline
because its scanned Rust source matches 577 and 0d6.

These focused checks do not close canonical, full-native, W9, platform,
scheduler, or comparative-performance acceptance. No timing result is claimed.
Scratch paths in raw records describe the executed commands, not required
future locations.
