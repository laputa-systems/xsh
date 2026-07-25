# Benchmarking

XSH benchmarks latency-sensitive workflows that users experience through the
`xsh`, `xshi`, and `xsht` surfaces. The same curated suite drives regression
tracking and profile-guided optimization (PGO).

The suite lives in `crates/xsh-multicall/benches/bench.rs` as one Divan
benchmark binary. Divan's global allocation profiler records median wall-clock
latency, allocation count per operation, and allocated bytes per operation.
Linux release builds use mimalloc because the musl allocator regresses these
allocation-heavy workloads. The benchmark wraps the same mimalloc allocator
with `AllocProfiler`, so allocation accounting does not change the allocator
being measured. Other hosts use the system allocator.

## Workloads

The suite covers complete operations rather than isolated helpers:

- running a short typed script and a process pipeline;
- counting extensions across a 1,000-file tree;
- parsing and aggregating 10,000 JSON log rows;
- hashing and encoding a 1,000-file package manifest;
- rendering a long interactive prompt;
- navigating completion over 1,000 directory entries;
- searching and rendering a 45,000-entry history;
- completing a `cd` workflow over 1,000 entries;
- checking, format-checking, and linting this repository's real XSH corpus.

The XSH programs exercised by the runtime benchmarks are checked-in files under
`crates/xsh-multicall/benches/scripts/`; benchmark source is not embedded in
Rust. Generated filesystem and log fixtures are deterministic. Corpus creation
and other setup that users would not wait for during the measured operation
stay outside the measured closure. A benchmark belongs in this suite only when
making it faster would directly improve a user-visible workflow.

## Baselines

Run:

```sh
make bench
```

The baseline helper compares the current run with a host-specific file under
`crates/xsh-multicall/benches/`, then replaces that local baseline. Baseline
files are ignored by Git because timing data is machine-specific. Each row
records:

```text
benchmark_name    median_ns    allocation_count    allocation_bytes
```

Latency is the primary signal. Allocation count and bytes are secondary signals
that can explain latency changes on repeated paths. Added and removed
benchmarks are reported explicitly.

## Interpreter And IR Diagnostics

The user-facing suite remains the decision point for interpreter and lowered-IR
work. Diagnostics narrow down a regression from that suite; they do not define
a second benchmark corpus or a separate PGO workload.

Start with the affected operation and keep the exact workload while iterating:

```sh
cargo bench -p xsh-multicall --bench bench xsht_check_xsh_repository -- \
  --sample-count 10 --sample-size 1
```

The unabridged Divan output adds `max alloc` to the baseline metrics. `alloc`
measures total allocation traffic; `max alloc` measures the peak live requested
bytes and allocation count observed on the benchmark thread. The latter is the
first retained-memory lens for parser arenas and lowered IR. Divan does not
count allocations performed by threads it does not control, so use process RSS
only when the workload is substantially multithreaded or allocator retention is
the question.

Use the existing complete operations as phase lenses:

- `xsht_format_check_xsh_repository` emphasizes lexing, parsing, and CST
  construction;
- `xsht_check_xsh_repository` covers parsing, semantic checking, and lowering
  across real checked-in XSH;
- `xsht_lint_xsh_repository` adds lint traversal;
- the `xsh_*` workloads include parse, check, lower, and execution over real
  scripts and deterministic data.

These are attribution clues, not isolated phase scorecards. If a change only
helps a synthetic parse or evaluator loop but does not improve a represented
operation, it should not shape the implementation or PGO profile.

For structural IR memory, run:

```sh
scripts/ir-layout.py
scripts/ir-layout.py --details LoweredExpr --details LoweredStmt
```

This is a thin view over rustc's `-Zprint-type-sizes` output. The summary tracks
the hot arena, builder, and lowered-IR types; `--details` shows the variants and
fields that set an enum's size. Compare it before and after representation
changes, then check Divan's allocated bytes and `max alloc` on the affected real
workflow. Type size alone is not a memory result: multiply it by realistic node
volume mentally, and account for heap-owned `Vec`, `Arc`, map, and boxed
payloads through the allocation measurements.

Use `tools/xsh-ir-coverage.xsh` to find frequent constructs in real XSH code
that fail to lower. Use `LLVM-LINES.md` and
`tools/llvm-lines-repeat-offenders.xsh` when the concern is generated code size
rather than runtime memory. A useful interpreter or IR change therefore has a
short evidence chain:

1. a real suite workload exposes the cost;
2. one diagnostic attributes it to execution, allocation traffic, peak live
   memory, type layout, lowerability, syscalls, or generated code;
3. the nearest runtime and lowering parity tests protect behavior;
4. the focused workload improves, followed by `make bench`;
5. `make bench-pgo` is required only when evaluating PGO results.

## PGO

Run:

```sh
make pgo-profile
make release-pgo
```

`pgo-profile` removes the previous profile directory, runs the entire benchmark
suite with instrumentation and `--sample-size 1`, and merges the generated
profiles with `llvm-profdata`. There is no separate PGO filter: the curated
benchmark suite defines what the product should optimize. The single-sample
workload keeps fast operations from dominating the profile through repetition.

Use:

```sh
make bench-pgo
```

to benchmark regular and PGO builds into separate host baselines and compare
them. PGO should improve the same user-facing workflows used to justify it.

## Syscall Diagnostics

Run:

```sh
make bench-syscalls
```

The syscall helper builds `Dockerfile.test`, runs each benchmark under `strace`,
and reports calls and failures between optional benchmark markers. This is a
separate diagnostic path for detecting unexpected subprocesses, filesystem
churn, or kernel work; normal timing runs do not include tracing.

The public `xsht trace --syscalls` and `xsht trace --trace-format flamegraph`
features remain user tooling. They are independent of the benchmark and native
profiler workflows.
