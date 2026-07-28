# Frontend Benchmark Report

Date: July 27, 2026  
Host: Apple M1, 10 cores, 32 GiB, macOS 26.5  
Corpus: 287 files, 545,254 source bytes

## Verdict

The grand frontend redesign is a **qualified success**.

It succeeds at its primary ownership and retained-memory goals: the recursive
lowered program is gone from production, one compact program owns executable
behavior, lower-stage retained storage falls 77.81%, and peak live frontend
storage after scratch is dropped falls 20.70%. Repository-check allocation
traffic is also below Phase 0 after removing repeated call-graph construction.

It is not an unconditional performance success. Lowering allocates more total
bytes while constructing the smaller result, and final explicit execution
frames are larger than the initial Phase 9 layout capture. Generated docs were
excluded by explicit direction. The JSON regression recorded in the original
Stage 11 snapshot was subsequently closed; see `JSON-IMPROVEMENTS.md`.

## Lowered `par-map` Follow-Up

The current lowered `par-map` path now honors `--jobs=N` and uses bounded
worker threads while preserving input order. The parent polls workers so signal
hooks remain observable while workers run. Traced `par-map` remains serial to
preserve ordered parallel-job trace events.

On the July 27, 2026 Sentry showcase A/B, forcing `--jobs=1` measured 5.181 s
for the default table and 7.509 s for JSON. The default ten-worker path measured
2.216 s and 4.459 s respectively, a 2.34× and 1.68× improvement. Native release
tokei measured 0.918 s and 0.863 s on the same warm seven-run comparison.

## All-Three Optimization Pass

The July 28, 2026 follow-up added direct lowered-function target caching,
`par-map |> identity flat-map |> reduce-by` worker-local fusion, and
ownership-preserving extraction of reducer fields. The Sentry result was
2.269 s for the default table and 4.298 s for JSON, versus 0.736 s and 0.799 s
for native release tokei on the same warm seven-run comparison.

Compared with the parallel-only baseline above, JSON improved modestly from
4.459 s to 4.298 s. The default table moved from 2.216 s to 2.269 s, which is
within the run spread and is not a material latency win. Keep the prior
parallel-only numbers as the stable baseline until a fresh profile explains
the remaining dynamic-value and interpreter costs.

## Hybrid Execution And JSON Aggregation

The July 28 follow-up adds a release-only shallow-call fast path that uses the
recursive lowered evaluator while retaining explicit frames for deep calls and
all debug builds. The Tokei JSON path now aggregates file reports with fused
worker-local `par-map |> reduce-by` instead of a serial per-file language
switch. The output fingerprint remains unchanged.

The latest seven-run release comparison measures 1.418 s for the default table
and 1.326 s for JSON, versus native 0.698 s and 0.695 s. That is approximately
2.03× and 1.91× slower than native. A single-run RSS check measured 42.9 MiB
versus native 48.1 MiB for the default path, and 56.3 MiB versus 56.3 MiB for
JSON. Memory is at parity; the remaining gap is execution CPU work.

## Focused Evaluator Benchmark

The new `xsh_lowered_scanner_1000_calls` workload isolates the repeated
`scan_hash()` lowered function path in
`crates/xsh-multicall/benches/scripts/lowered-scanner.xsh`. The ordinary
prepared-plus-execution benchmark measured 10.87 ms, 5,559 allocations, and
645.2 KiB allocated. Its ignored execution-only companion measured 6.234 ms
median over five samples, with 4 allocations and 2.36 KiB allocated. Future
evaluator dispatch changes
should use the execution-only row for the decision and the ordinary row to
catch frontend/setup regressions.

## Pre-Rewrite Checkpoint

Commit `3d848b6c8ed08419801b929c1af5ed22c26a49a3` (`another opt`, July 24,
2026) was benchmarked in a detached release worktree on July 28 against the
same Sentry corpus and native binary. It measured 0.969 s for the default table
and 0.987 s for JSON, versus native 0.754 s and 0.775 s: only about 1.29× and
1.27× slower. Its default and JSON outputs also matched the current XSH output
fingerprints. The large gap therefore appeared after this pre-rewrite
checkpoint, not before the frontend rewrite.

## Rewrite Bisect Checkpoints

Representative release checkpoints used one warmup and three measured runs on
the same Sentry workload. Native release tokei stayed around 0.72–0.79 s across
these short runs.

| checkpoint | default table | JSON |
| --- | ---: | ---: |
| `3181c9a` (`p5`) | 0.927 s | 1.027 s |
| `4ddda99` (`p6-progress`) | 3.480 s | 5.902 s |
| `b28ac2b` (`p6`) | 3.435 s | 5.899 s |
| `868208a` (`p9`) | 4.969 s | 7.389 s |
| `274e73a` (`p11`) | 4.962 s | 7.443 s |
| `c114870` (parallel lowered `par-map`) | 2.069 s | 4.457 s |

The first buildable bad checkpoint is `4ddda99`: the indexed-runtime rewrite
between `p5` and `p6-progress` introduces the first large regression. `p9`
adds a second major step when explicit indexed execution frames replace the
older call path. `6ed9d20` (`p6-wip`) was not benchmarkable because its checkout
does not compile (`LOWERED_SHARED_LIST_THRESHOLD` is missing). The later
parallel `par-map` fix recovers wall time but does not remove the underlying
indexed interpreter overhead.

## Post-Stage 11 JSON Result

The accepted regular release median for `xsh_json_log_rollup_10000_rows` is
13.58 ms, compared with 13.81 ms in the repeated Phase 0 measurement and
23.13 ms in the original Stage 11 focused result. The final regular result is
1.67% faster than Phase 0 and does not depend on PGO.

An ignored execution-only diagnostic established that preparation contributed
less than 1 ms. The fixes restore borrowed indexed field chains, compile direct
field/literal `where` predicates once per stage, reuse direct field projections
in `sort-by`, `group-by`, and `map`, preserve owned dynamic record keys during
runtime-to-lowered conversion, and construct shaped JSON records without a
temporary dynamic map. The ignored diagnostic does not alter the curated suite
or PGO workload.

## Frontend Memory

The before and after reports use the same corpus and source bytes.

| stage | Phase 0 retained | final retained | delta | Phase 0 peak | final peak | delta |
|---|---:|---:|---:|---:|---:|---:|
| tokens | 899,881 B | 899,884 B | +0.00% | 522,076 B | 531,010 B | +1.71% |
| CST | 7,180,862 B | 7,180,870 B | +0.00% | 2,122,895 B | 2,174,763 B | +2.44% |
| AST/check | 14,412,245 B | 14,397,053 B | -0.11% | 6,475,480 B | 6,532,540 B | +0.88% |
| lower | 12,106,023 B | 2,686,770 B | **-77.81%** | 9,688,845 B | 8,300,303 B | **-14.33%** |
| after drop | 539,307 B | 531,248 B | **-1.49%** | 8,768,635 B | 6,953,227 B | **-20.70%** |

Lower construction performs 413,749 allocations versus 402,828 in Phase 0
(+2.71%) and allocates 55,940,116 bytes versus 49,644,031 (+12.68%). The
redesign therefore wins retained and peak memory, not construction traffic.

The repository-check fast sample improves from Phase 0's 547,004 allocations
and 80.11 MiB to 506,972 allocations and 65.45 MiB. This includes the Stage 11
fix that computes dependency and SCC metadata once per program instead of
rebuilding the complete call graph for every emitted function.

## Representation

Phase 0 used 144-byte `LoweredStmt`, 72-byte `LoweredExpr`, and 56-byte
`LoweredPattern` rows with recursive nested allocations. The final store uses
one-byte tags, 20-byte `FullBlock`, 32-byte `FullFunction`, 12-byte parameter
and capture rows, and 4-byte construction IDs.

The frozen executable corpus stores 43,589 instructions in 1,843,105 bytes,
57.11% below the conservative 4,297,136-byte recursive-row lower bound. That
lower bound excludes the recursive representation's nested heap allocations.

Final active execution layouts are:

| type | size |
|---|---:|
| `CallFrame` | 368 B |
| `FrameWork` | 168 B |
| `FrameContinuation` | 128 B |
| `FullStore` header | 968 B |
| `FullProgram` header | 984 B |
| `AstArena` header | 1,488 B |

The explicit-frame rows scale with active call depth rather than retained
frontend size. They are larger than the Phase 9 capture after match, require,
and method continuations were added to restore complete behavior.

## Stage 11 Regular And PGO Snapshot

This table predates the post-Stage 11 JSON improvements above. Both columns are
medians of three measured full-suite runs after one warmup.
The regular suite took 199.8 seconds including warmup; PGO took 190.7 seconds.
Allocation totals are identical between regular and PGO; two sub-microsecond
workloads show tiny peak-sample differences.

| benchmark | regular | PGO | PGO delta | allocated | allocs/op |
|---|---:|---:|---:|---:|---:|
| `runtime_dynamic_record_build_8_fields` | 386 ns | 345 ns | -10.62% | 912 B | 9 |
| `runtime_dynamic_record_clone_drop_8_fields` | 43 ns | 43 ns | +0.00% | 720 B | 1 |
| `runtime_scalar_clone_drop` | 2 ns | 2 ns | +0.00% | 0 B | 0 |
| `runtime_shaped_record_build_8_fields` | 264 ns | 248 ns | -6.06% | 1.26 KiB | 4 |
| `runtime_shaped_record_clone_drop_8_fields` | 3 ns | 2 ns | -33.33% | 0 B | 0 |
| `runtime_shaped_record_thread_transfer_8_fields` | 7.50 µs | 13.89 µs | +85.22% | 0 B | 0 |
| `xsh_extension_count_1000_files` | 1.84 ms | 1.68 ms | -8.53% | 42.35 KiB | 357 |
| `xsh_json_log_rollup_10000_rows` | 23.35 ms | 19.11 ms | -18.16% | 53.77 KiB | 409 |
| `xsh_manifest_hash_1000_files` | 108.20 ms | 102.80 ms | -4.99% | 43.26 KiB | 352 |
| `xsh_process_pipeline` | 14.80 ms | 14.46 ms | -2.30% | 18.56 KiB | 218 |
| `xsh_short_script` | 210.90 µs | 192.50 µs | -8.72% | 35.49 KiB | 314 |
| `xshi_cd_list_complete_1000_entries` | 2.75 ms | 2.45 ms | -11.05% | 452.60 KiB | 14,043 |
| `xshi_completion_navigation_1000_entries` | 19.33 µs | 16.41 µs | -15.11% | 255 B | 7 |
| `xshi_dynamic_name_session` | 4.63 ms | 4.70 ms | +1.49% | 8.86 KiB | 88 |
| `xshi_history_search_render_45000_entries` | 3.24 µs | 2.16 µs | -33.40% | 2.05 KiB | 1 |
| `xshi_prompt_render_long_command` | 101 ns | 131 ns | +29.70% | 2.05 KiB | 1 |
| `xsht_check_xsh_repository` | 174.00 ms | 158.10 ms | -9.14% | 65.28 MiB | 505,315 |
| `xsht_format_check_xsh_repository` | 80.40 ms | 80.01 ms | -0.49% | 274.20 KiB | 844 |
| `xsht_lint_xsh_repository` | 64.83 ms | 64.09 ms | -1.14% | 525.90 KiB | 6,828 |

PGO improves 14 of 19 workloads. The thread-transfer and prompt-render rows are
short enough that their negative deltas require focused repetition before they
justify implementation changes.

The repeated Phase 0 comparison measured JSON rollup at 13.81 ms and repository
checking at 156.1 ms. The Stage 11 focused regular measurements were 23.13 ms
and 161.9 ms; the Stage 11 focused PGO measurements were 19.28 ms and 157.8 ms.
Repository checking was near Phase 0 after the graph fix. JSON later reached
13.58 ms in the regular build and closed its follow-up without PGO.

## Syscalls

The Docker `strace` diagnostic completed for every benchmark. Key operation
counts include:

| benchmark | selected syscall counts |
|---|---|
| `xsh_process_pipeline` | 2 `execve`, 3 `clone`, 3 `wait4`, 1 `pipe2`, 2 `socketpair` |
| `xsh_json_log_rollup_10000_rows` | 22 `read`, 12 `openat`, 12 `close`, 2 `getdents64` |
| `xsht_check_xsh_repository` | 1,212 `read`, 818 `openat`, 814 `close`, 436 `getdents64` |

No retained Phase 0 `strace` artifact exists for an exact final delta. Earlier
campaign evidence records the same process path, and the final diagnostic shows
the expected two process executions and wait boundaries for the pipeline.

## Verification

Passing gates:

- frontend retained-stat tests;
- indexed builder and verifier tests;
- semantic integration tests;
- the complete xsht CLI test suite;
- 251 serial runtime integration tests;
- exact native coverage;
- full fast memory benchmark;
- full regular and PGO benchmark suites;
- Docker syscall diagnostics.

The unfiltered `cargo test` run reaches 450 passes, including native coverage,
then fails `ambient_fs_policy::ambient_filesystem_use_is_allowlisted` because
test-only code in `src/runtime/eval/indexed/full.rs` uses `std::env::temp_dir`,
`fs::create_dir_all`, `fs::write`, and `fs::remove_dir_all`. This is unrelated
to the Stage 11 optimization and was not modified.

Generated docs checks were intentionally not run to completion. The generated
artifacts remain stale by explicit direction.
