# Typing campaign test inputs

Implementation is paused. Remaining work is in
`../../xsh-typing-inference-campaign.md`. Benchmarking comes only after functional
implementation, annotation removal, migration, and consolidation are complete.

The latest source checkpoint builds all three debug binaries. Its semantic
integration suite passes 181 tests. The library suite passes 1,114, fails 56,
and ignores one. `wind-down-failures.json` keeps the failing test names and
useful diagnostics from source revision `e5004ea5`; it is a debugging aid,
not a claim that the campaign passes.

Retained inputs:

- `operations.json`: operation contracts consumed by Rust regression tests.
- `annotations.json`: eligible and protected annotation sites and targets.
- `cohort/original/` and `cohort/stabilized/`: compatibility and annotation-removal
  inputs; `cohort/source-correspondence.json` and `cohort/stabilization.patch`
  identify the deliberate semantic stabilizers.
- `semantic-cases/` and `semantic-cases.json`: independent semantic witnesses.
- `runtime-fixtures/`: reusable programs for runtime comparisons.
- `scaling.py`, `scaling/`, `scaling-manifest.json`, and `resource-limits.json`:
  existing generated stress inputs, deferred until the final performance check.

Keep tests and current actionable failures. Do not accumulate successful-run
logs, compiler output, duplicate snapshots, checkpoint reports, or archived
plans. Historical versions are available in Git.
