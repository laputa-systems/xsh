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
files are ignored by Git because timing data is machine-specific. The normal
path runs one discarded warmup suite followed by three measured suites and
records the median of those three runs. This keeps the default reasonably quick
while reducing cold page-cache and one-off scheduler effects. The report also
shows the timing spread across the three measured runs so unstable results are
visible. The helper always prints and records whole-suite wall time
(`wall_s`, plus measured and warmup totals) so iteration cost is explicit.

Each row records:

```text
benchmark_name    median_ns    allocation_count    allocation_bytes    max_alloc_count    max_alloc_bytes
```

Latency is the primary signal for ordinary performance work. Allocation count,
allocated bytes, and Divan `max alloc` (peak live requested bytes and count on
the benchmark thread) explain memory and representation changes. Added and
removed benchmarks are reported explicitly.

### Fast memory iteration

For representation and allocation work, prefer:

```sh
make bench-fast
```

which is `scripts/bench-baseline.py --fast`. That mode uses zero warmup suites,
one measured suite, Divan `--sample-count 1 --sample-size 1`, and a memory-only
report that omits per-benchmark time and run spread. Allocation
traffic is deterministic enough for single-sample comparison. The default
baseline path gets a `-fast` suffix so fast runs do not overwrite normal
latency baselines. Override with `--baseline`, `--variant`, `--warmup-runs`,
`--runs`, `--sample-count`, or `--sample-size` when needed.

Do not mix fast and normal baseline files when judging deltas: single-sample
runs start colder because XSH has process-global interners and caches.

For the frontend campaign, `scripts/frontend-campaign-phase0` captures two fast
runs and compares only the allocation and peak columns. Whole-suite wall time
is retained as iteration telemetry, not as the Phase 0 pass/fail signal. The
same driver records stage-split frontend stats, layout, coverage, line counts,
and host metadata under `target/frontend-campaign/phase-0/`.

## Interpreter And IR Diagnostics

The user-facing suite remains the decision point for interpreter and lowered-IR
work. Diagnostics narrow down a regression from that suite; they do not define
a second benchmark corpus or a separate PGO workload.

Start with the affected operation and keep the exact workload while iterating:

```sh
make bench-fast
# or one operation:
cargo bench -p xsh-multicall --bench bench xsht_check_xsh_repository --   --sample-count 1 --sample-size 1
```

`make bench-fast` already records Divan `max alloc` in the baseline. `alloc`
measures total allocation traffic; `max alloc` measures the peak live requested
bytes and allocation count observed on the benchmark thread. The latter is the
first retained-memory lens for parser arenas and lowered IR. Divan does not
count allocations performed by threads it does not control, so use process RSS
only when the workload is substantially multithreaded or allocator retention is
the question. Use multi-sample Divan settings only when stabilizing latency,
not when iterating on allocation bytes.

Run benchmark processes serially. XSH has process-global interners and caches,
so the first sample can be visibly colder than the median, and parallel
benchmark processes contaminate latency results. Layout and allocation-byte
deltas are normally deterministic enough to decide representation changes;
latency is noisier. For a timing decision, repeat the focused before/after
measurement with identical sample settings and require the direction to repeat.
Treat a small single-run delta—especially below roughly 5% for sub-millisecond
work—as inconclusive. `make bench` is a regression sweep, not statistical proof
that a marginal timing change is real.

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
scripts/ir-layout.py --only LoweredExpr --only LoweredStmt
```

This is a thin view over rustc's `-Zprint-type-sizes` output. By default it
reports the summary plus the variants and fields of every tracked hot arena,
builder, semantic type, lowered-IR, runtime value, lowering-probe, and evaluator
type. Use repeatable `--only TYPE` filters when a focused report is easier to
compare. Compare it before and after representation changes, then check Divan's
allocated bytes and `max alloc` on the affected real workflow. Type size alone
is not a memory result: multiply it by realistic node volume mentally, and
account for heap-owned `Vec`, `Arc`, map, and boxed payloads through the
allocation measurements.

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
5. stop when the ordinary gate fails; PGO does not make a regressed
   implementation acceptable.

## PGO

Do not run PGO during ordinary runtime, IR, or representation iteration. The
instrumented rebuild is intentionally expensive and provides low-signal
feedback while non-PGO latency, allocation, behavior, or coverage results are
still changing. First make `make bench` and the relevant correctness gates pass.

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
them only for a stable release candidate or an explicit PGO investigation. PGO
should improve the same user-facing workflows used to justify it.

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
