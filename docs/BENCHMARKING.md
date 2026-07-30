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

The executable frontend has no separate legacy benchmark runner or shadow
execution mode. Use the normal suite for all final evidence: `make bench-fast`
for deterministic allocation and peak-live comparisons, then `make bench` for
latency repeats. The focused `xsht_check_xsh_repository` and
`xsht_format_check_xsh_repository` operations cover the parse/check/lower and
tooling paths; `xsh_short_script` covers installed execution. Keep the before
and after command, profile, host, allocator, sample count, and sample size the
same. Do not infer a timing regression from an unpaired thermally unstable run.

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
- repeated lowered scanner calls, with a prepared-execution variant that
  isolates `eval_indexed_expr`/`eval_indexed_stmt` from frontend setup.

The XSH programs exercised by the runtime benchmarks are checked-in files under
`crates/xsh-multicall/benches/scripts/`; benchmark source is not embedded in
Rust. Generated filesystem and log fixtures are deterministic. Corpus creation
and other setup that users would not wait for during the measured operation
stay outside the measured closure. A benchmark belongs in this suite only when
making it faster would directly improve a user-visible workflow.

### External loopback HTTP benchmark

`xsh_net_http1_10000_requests_blocking` exercises the `net.request` path in
`crates/xsh-multicall/benches/scripts/net-http1-requests-10000-blocking.xsh`.
It first constructs the same 10,000-record request list as the batch case,
then requests a three-byte file 10,000 times serially from a local `darkhttpd`
server with HTTP keep-alive enabled. The server and its temporary document root
are created outside the measured closure. This is intentionally an ignored
benchmark so the ordinary suite does not depend on a host-installed server.
Install `darkhttpd` (or set `DARKHTTPD` to its executable) and run:

```sh
cargo bench -p xsh-multicall --bench bench xsh_net_http1_10000_requests_blocking -- \
  --include-ignored --sample-count 1 --sample-size 1
```

`xsh_net_http1_10000_requests_batch_8` runs the same workload through
`net.request_many` with eight concurrent HTTP/1.1 connections. Use the same host
settings and this command for its paired measurement:

```sh
cargo bench -p xsh-multicall --bench bench xsh_net_http1_10000_requests_batch_8 -- \
  --include-ignored --sample-count 1 --sample-size 1
```

The loopback workload measures XSH and client overhead without WAN latency; a
delayed server is required to measure latency hiding separately.

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

For frontend representation work, run the focused frontend-stat command in
`docs/TEST-MAP.md`, then compare a serial `make bench-fast` baseline. Whole-suite
wall time is iteration telemetry for that gate; allocation and peak-live columns
are the decision signals. `scripts/ir-layout.py` supplies the complementary
type-layout view.

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

Use the existing complete operations as cost lenses:

- `xsht_format_check_xsh_repository` emphasizes lexing, parsing, and CST
  construction;
- `xsht_check_xsh_repository` covers parsing, semantic checking, and lowering
  across real checked-in XSH;
- `xsht_lint_xsh_repository` adds lint traversal;
- the `xsh_*` workloads include parse, check, lower, and execution over real
  scripts and deterministic data.

These are attribution clues, not isolated stage scorecards. If a change only
helps a synthetic parse or evaluator loop but does not improve a represented
operation, it should not shape the implementation or PGO profile.

For structural IR memory, run:

```sh
scripts/ir-layout.py
scripts/ir-layout.py --only FullTag --only FullBlock --only FullFunction
```

This is a thin view over rustc's `-Zprint-type-sizes` output. By default it
reports the summary plus the variants and fields of every tracked hot arena,
builder, semantic type, indexed IR, runtime value, lowering-probe, and evaluator
type. Deleted recursive executable types are intentionally absent. Use
repeatable `--only TYPE` filters when a focused report is easier to compare.
Compare it before and after representation changes, then check Divan's
allocated bytes and `max alloc` on the affected real workflow. Type size alone
is not a memory result: multiply it by realistic node volume mentally, and
account for heap-owned `Vec`, `Arc`, map, and boxed payloads through the
allocation measurements.

### Focused lowered scanner

Use `xsh_lowered_scanner_1000_calls` for changes to lowered expression and
statement dispatch, function calls, line scanning, or scalar loop execution.
The checked-in workload is `crates/xsh-multicall/benches/scripts/lowered-scanner.xsh`;
its hot path is the `scan_hash()` call inside the 1,000-iteration loop. Run the
ordinary operation to include parse/check/lower setup:

```sh
cargo bench -p xsh-multicall --bench bench xsh_lowered_scanner_1000_calls -- \
  --sample-count 1 --sample-size 1
```

Run `xsh_lowered_scanner_1000_calls_execution` with `--include-ignored` to
reuse the prepared indexed program and measure execution only:

```sh
cargo bench -p xsh-multicall --bench bench xsh_lowered_scanner_1000_calls_execution -- \
  --include-ignored --sample-count 1 --sample-size 1
```

The prepared result is the decision signal for evaluator changes; the gap to
the ordinary operation is frontend/setup work and should not be attributed to
`eval_indexed_expr` or `eval_indexed_stmt`.

Use `tools/xsh-ir-coverage.xsh` to find frequent constructs in real XSH code
that fail to lower. Use `tools/llvm-lines-repeat-offenders.xsh` with a
`cargo llvm-lines` capture when the concern is generated code size rather than
runtime memory. A useful interpreter or IR change therefore has a
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
