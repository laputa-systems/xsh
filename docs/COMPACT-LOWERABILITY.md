# Compact Lowerability Gap Guide

This guide is for closing `compact.unlowered-*` and `runtime.unlowered-*`
failures without weakening compact lowerability. The goal is strict detection:
`xsht check` should report every construct that cannot run in the compact
runtime, and `xsh` should not silently accept a source shape by falling back to
an unsupported path.

## Operating Rule

Treat every unlowered diagnostic as one of these cases:

1. The compact runtime can already express the behavior, but the lowerer is
   missing type information or a construction case.
2. The compact runtime needs a new `LoweredExpr`, `LoweredStmt`, type, or
   runtime operation path.
3. The source genuinely should remain unsupported, and the diagnostic should say
   what construct blocked lowering.

Do not make `Any`, `Unknown`, records, modules, or dynamic calls permissive just
to pass a corpus check. If the receiver or callee is dynamic, keep it rejected
unless there is a concrete checked type that proves the lowered method is valid.

## Fast Loop

Start with the smallest failing command:

```sh
cargo build --bin xsh --bin xsht
./target/debug/xsht check ../laputa
cargo test --test runtime SOME_TEST -- --nocapture
```

For local corpus work, prefer one script or one test first:

```sh
./target/debug/xsh examples/archive.xsh
./target/debug/xsht check tools/xsh-ir-coverage.xsh
cargo test --test runtime coverage::ir_coverage_scans_multiline_top_level_regions_once -- --nocapture
```

When the public diagnostic only points at a top-level statement, reduce the
script by deleting surrounding statements until one expression remains. If that
is still unclear, add a temporary env-gated trace near the `lower_expr` blocker
path in `src/runtime/eval/lower.rs`, run with `XSH_DEBUG_LOWER=1`, then remove
the trace before finishing.

## Where To Look

- `src/runtime/eval/lower.rs`: AST to compact lowered IR construction, type
  inference used by lowering, blocker counters, top-level lowerability.
- `src/runtime/eval/lowered_run.rs`: execution of lowered expressions,
  statements, methods, and module runtime ops.
- `src/runtime/eval.rs`: compact install, `xsht check` lowerability diagnostics,
  runtime execution of installed compact plans.
- `src/sema/check/compact.rs`: compact probe checker used before lowering.
- `crates/xsh-registry/src/signature/*`: standard API signatures and runtime
  operation ids.
- `crates/xsh-registry/src/records.rs`: standard record schemas that should be
  preserved when a lowered API returns structured data.

## Common Failure Classes

**Concrete type lost as `Any` or `Unknown`**

Symptoms: a valid method such as `.len()`, `.trim()`, `.keys()`, or `.cancel()`
is rejected because the local slot type is dynamic.

Fix the compact lowerer inference, not the method gate. Check:

- local binding type selection;
- `Try(...)` ok-type inference;
- top-level slot metadata;
- module/method return schemas;
- env pseudo-fields such as `env.Str.NAME?` and `env.Path.NAME?`;
- standard records returned by APIs such as `metadata`, `fs.files`, archive
  list APIs, and byte-copy helpers.

**Known method missing receiver-specific return type**

Symptoms: a chain lowers until a method result is used as the next receiver, for
example `path.read_text()?.trim()` or `bytes.slice(...).dump(...)`.

Add checked return inference for the method on the concrete receiver type. Keep
receiver support strict: `Type::Any` and `Type::Unknown` should not pass method
support checks.

**Known module op missing compact construction**

Symptoms: the checker accepts the API and `RuntimeOp` exists, but expression
lowering records a call blocker.

Check `lowered_module_op_supported`, `lowered_module_call_args`, special
construction in `lower_call`, and execution in `lowered_run.rs`. Add the
smallest exact lowered form. Reuse registry signatures and existing runtime
helpers.

**Top-level statement lowers in pieces but is rejected**

Symptoms: subexpressions lower, but `xsht check` reports the whole statement.

The construct probe substitutes placeholders while counting blockers. The real
top-level lowerer rejects any statement that produced blocker events. Find the
first blocker event, not the final placeholder.

**Checker-only forms**

`reveal_type(...)` is checker-only. `xsht check` may skip it for lowerability,
but normal `xsh` execution must still reject it. Keep that distinction explicit
in `compact_top_level_stmt_is_skippable`.

## Diagnostics Work

`compact.unlowered-statement` is often too broad. Prefer diagnostics that name
the blocking construct:

- unsupported statement kind;
- unsupported call or method label;
- dynamic receiver type (`Any` or `Unknown`) when strict method lowering blocks;
- unsupported module op;
- unsupported function body dependency for `main`.

The construct probe already records blocker counters and sample spans. Prefer
threading those details into diagnostics over adding ad hoc string matching.

## Verification

Use the narrowest command that proves the fix, then widen:

```sh
cargo test --test runtime TARGET -- --nocapture
cargo test -p xsht --test cli TARGET -- --nocapture
cargo test --test sema
cargo test --test runtime
./target/debug/xsht check ../laputa
```

Do not run formatters or autofixers for this work. Do not change tests to avoid
strict lowerability unless the test source itself uses intentionally removed
language behavior. If a test asserts old behavior such as removed error fields,
update the source to the current language contract separately from compact
lowering changes.

## Current Useful Corpus Targets

As of this writing, useful gap-closing targets include:

- `./target/debug/xsht check ../laputa`
- `./target/debug/xsht check tools/xsh-ir-coverage.xsh`
- `cargo test --test runtime coverage::ir_coverage_scans_multiline_top_level_regions_once -- --nocapture`
- `cargo test --test runtime collections::fs_files_recurses_with_raw_walk_and_preserves_entry_ext -- --nocapture`
- `cargo test --test runtime streams::reduce_by_parallel_jobs_matches_serial -- --nocapture`
- `cargo test --test runtime coverage::showcase_standalone_scripts_are_self_testable -- --nocapture`

Expect these to change as gaps close. Keep this list current when a broad
cluster is resolved.
