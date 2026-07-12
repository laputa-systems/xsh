# XSH Performance Suite

This directory holds realistic XSH workloads for finding allocation, memory,
and syscall problems before making runtime or module optimizations.

For interpreter-only microbenchmarks, use:

```sh
cargo bench --bench bench interpreter
```

Those Criterion benchmarks include recursive Fibonacci and loop-heavy scripts
that run in-process, separating parse/check and eval costs from `xsh` process
startup. Treat them as evaluator stress tests; use this directory's scenarios
for representative systems-script performance. The current local interpreter
snapshot is tracked in `interpreter-baseline.json`.

For front-end floor overhead on ordinary small scripts, use the Criterion
frontend small-corpus lens:

```sh
cargo bench -p xshi --bench bench small_corpus -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1
```

That benchmark selects checked-in standalone `.xsh` files with at most 200 lines
and 16 KiB, then measures parse-only, parse/check, and parse/check/lower over
the same file set. The lower helper uses an explicit cwd so this lens measures
front-end setup instead of host `current_dir()` latency once per corpus file.
The current local baseline is tracked in
`perf/small-corpus-baseline-aarch64.json`.

## Frontend profiling workflow

Use this path for token table, arena layout, parser, declaration-probe, compact
module graph, body-probe, and compact lowering work. Start with the narrowest
command that answers the question, then refresh the Linux baseline only when the
change is meant to move the checked-in frontend metrics.

```sh
cargo check --no-default-features
cargo test --test runtime <filter> --no-default-features
RUST_MIN_STACK=16777216 cargo test --test runtime --no-default-features
git diff --check
cargo run --quiet --bin xsh-parse-corpus-report --no-default-features \
  --features "tools perf-metrics" -- --root . --repeat 1
make prof-baseline-frontend
```

Do not run formatters or autofixers as part of frontend profiling. The project
docs gate also avoids `make docs` for that reason; use the formatter-free docs
commands in `docs/TEST-MAP.md` when docs or examples change.

`xsh-parse-corpus-report` is the compact-front-end boundary-readiness report. In
addition to phase allocation/timing data, it emits:

- `per_file_counts`: bytes, tokens, statements, imports, exports, declarations,
  and executable top-level statements per file.
- `per_phase_file_summaries`: total, p50, p95, and max per-file timing for
  parse, declaration probe, body probe, lowering probes, and runtime declaration
  registration.
- `module_graph_readiness`: import edges, unique modules, qualified declaration
  count, duplicate diagnostics, and largest dependency component.
- `function_lowering_readiness`: attempted/lowered/blocked function counts,
  dependency edges, SCC count, blocker counts, and qualified vs unqualified call
  counts.
- `top_level_readiness`: lowered/skipped/blocked top-level statement counts and
  fallback reason counts.

`make prof-baseline-frontend` runs inside the Linux Docker profiling container
and writes `perf/*-baseline-$(uname -m).json`. On Apple Silicon that means the
`aarch64` baseline is a Linux baseline, not a macOS one. Compare baselines only
when target OS, target arch, profile, repeat count, and corpus root match.

The `unix::` spawn tests leak a child that holds the stdout pipe open, so a
foreground `cargo test --test runtime` can appear to hang after the `test
result:` line prints. Wait for that line, then `pkill -f deps/runtime`.

Regenerate checked-in performance baselines in the Linux test container, not on
the host. `Dockerfile.test` carries the musl toolchain and profiling tools used
for release-like measurements. Run only the lens that is relevant to the work:
full profiling is expensive and often hides the signal you need.

```sh
make prof-baseline-frontend  # arena/token/span/layout/parser memory work
make prof-baseline-runtime   # runtime/module/evaluator allocation or Ir work
make prof-baseline           # full refresh only when both areas changed
```

`perf/layout-baseline-<arch>.json` records `size_of` and `align_of` for the
registry, signature, compact syntax, semantic, runtime value, source-span,
symbol, and trace structs that dominate interpreter memory layout.
`perf/parse-corpus-baseline-<arch>.json` tracks front-end wall time,
allocation counters, and peak RSS across compact parse, declaration probing,
body probing, checking, and IR lowering over the checked-in `.xsh` corpus.
`make prof` writes current reports under
`target/prof/`; `make prof-baseline-frontend` refreshes the checked-in layout
and parse-corpus baselines. Use `interpreter-baseline.json` for timing
direction, the parse-corpus baseline for front-end memory pressure, and
`make prof-baseline-runtime` only when dhat/Callgrind allocation or
instruction-count validation is relevant.

The corpus generator creates a small package/repository tree: source files,
docs, test fixtures, JSONL service logs, package install roots, executable
files, and hidden cache files. The scenarios then exercise broad XSH surfaces:

- `extension-count.xsh`: `fs.walk`, path records, regex filtering, grouping,
  sorting, and the zero-subprocess equivalent of `fd -tf | awk ...`.
- `manifest-hash.xsh`: package manifest creation with file reads, bytes,
  SHA-256 hashing, path stripping, JSON encoding, and aggregation.
- `json-log-rollup.xsh`: JSONL ingestion, text joining, structured stream
  grouping, and numeric totals.
- `archive-package.xsh`: tar.gz creation, listing, extraction, file reads, and
  payload hashing.
- `value-churn.xsh`: large `Value` lists, record construction, field access,
  sorting, grouping, and repeated map updates.
- `record-stream.xsh`: synthetic record streams with nested records,
  projections, grouping, and aggregation.
- `stream-heavy.xsh`: range pipelines, `flat-map`, `where`, `map`, `enumerate`,
  grouping, and collection.
- `parse-check-heavy.xsh`: a heavier typed source file with structured
  literals, typed helper functions, records, lists, and stream stages.

Run all scenarios with release codegen, allocation counters, and platform memory
stats:

```sh
target/release/xsh perf/run.xsh
```

The runner writes a normalized allocation report to
`target/perf/YYYYMMDD-HHMMSS/allocation.json`. Compare a run against the checked-in
allocation baseline with:

```sh
target/release/xsh perf/allocation-compare.xsh -- perf/allocation-baseline.json target/perf/YYYYMMDD-HHMMSS/allocation.json
```

## xsht corpus benchmark

`perf/fmt-corpus.sh` measures read-only `xsht fmt --check`, `xsht check`, or
`xsht lint` over the repository XSH corpus (`core`, `examples`, `showcase`,
`tests/xsh`, and `tools`). These are end-to-end CLI measurements including file
loading and module processing. It writes per-operation Hyperfine and allocation
output under `target/perf/` and reports the corpus file and byte counts:

```sh
perf/fmt-corpus.sh
perf/fmt-corpus.sh --runs 20 --warmup 5
perf/fmt-corpus.sh --operation all --alloc
```

The normal timing run uses a plain release binary. `--alloc` rebuilds with
`perf-metrics` and runs one allocation-counting pass after timing for every
selected operation; treat that pass as a memory diagnostic, not as a timing
comparison. Use `--no-build` with an already-built `target/release/xsht` when
iterating on the tools.

## Comprehensive profiling and PGO (`make prof`)

`allocation-compare` answers *how much* is allocated; the `make prof` family
answers *where* native CPU and allocations go — per Rust function and per
allocation call stack. Start from the question you have:

| Question | Lens | Entry point |
|---|---|---|
| What code is exercised by the tests? | LLVM source coverage | `make cov` |
| Make the shipped binary faster, automatically | Profile-guided optimization | `make prof-pgo` |
| How many bytes does each call stack allocate? | In-process dhat heap profiler | `make prof-dhat` |
| How many instructions does each function execute? | Callgrind, deterministic | `make prof-callgrind` |
| Where do cache/branch misses come from? | Cachegrind | `make prof-cachegrind` |
| Which syscalls does each operation make? | ptrace syscall tracer | `make test-trace` |
| Did a change actually help? | Callgrind / dhat before-vs-after diff | `make prof-compare` |

The `prof-*` family shares one set of ground rules:

- **musl, in Docker** (`Dockerfile.test`, Alpine). We profile what we ship.
  There is no glibc build and no macOS profiling path.
- **the `profiling` profile** (`release` codegen + `debug = true`, unstripped)
  so per-function attribution has symbols.
- **`net` excluded** (`--no-default-features --features tools`). Net
  (HTTP/DNS) is not the interpreter hot path; excluding it keeps the profile
  focused. The shipped release still includes net. `make cov` is the exception:
  it runs with default features, net included, so coverage reports the net
  module too.
- **only the `xsh` binary** is built, profiled, and optimized. `xsht`/`xshi` are
  tooling, not the hot path.

Where the profiling code lives:

| Piece | File |
|---|---|
| `make` targets (`prof`, `prof-pgo`, `prof-dhat`, …) | `Makefile` |
| musl + Valgrind + dhat test image | `Dockerfile.test` |
| PGO orchestrator (generate → merge → use) | `tools/prof-linux.xsh` |
| Coverage orchestrator | `tools/cov-linux.xsh` |
| dhat JSON → top-sites + diffable summary | `tools/dhat-summarize.xsh` |
| before/after diff scripts | `perf/callgrind-compare.xsh`, `perf/dhat-compare.xsh` |
| core struct layout baseline | `tests/helpers/layout_report.rs`, `perf/layout-baseline-<arch>.json` |
| front-end corpus wall time, allocations, and peak RSS | `tests/helpers/parse_corpus_report.rs` |
| checked-in PGO profiles + docs | `perf/pgo/xsh-*.profdata`, `perf/pgo/README.md` |
| in-process dhat allocator + guard | `src/perf.rs`, `src/entrypoints/xsh.rs` |

The main targets are:

- `make prof` — one-stop run: core layout report + parse-corpus metrics + PGO
  build + dhat allocs + Callgrind + Cachegrind. Use this only when broad
  profiling coverage is worth the runtime.
- `make prof-parse-corpus` — parse, desugar, convert to the arena/index AST,
  check, and IR-lower every
  checked-in `.xsh` source file that can reach each phase cleanly (skipping
  generated/build directories) and write `target/prof/parse-corpus.<arch>.json`.
  The report includes per-phase wall time, source/AST/arena/check/lowering
  counts, allocation counters, and peak RSS from the in-process `xsh::perf`
  allocator metrics.
- `make prof-baseline-frontend` — refresh the checked-in front-end baselines:
  `perf/layout-baseline-<arch>.json` and
  `perf/parse-corpus-baseline-<arch>.json`. This is the right target for AST,
  token, span, parser, and arena/index representation changes.
- `make prof-baseline-runtime` — refresh the checked-in runtime baselines:
  `perf/dhat-baseline-<arch>.json` and
  `perf/callgrind-baseline-<arch>.txt`. This is the right target for evaluator,
  module, allocation hot path, or instruction-count changes.
- `make prof-baseline` — run both baseline groups. Avoid this during tight
  iteration unless the change spans front-end representation and runtime
  behavior.
- `make prof-pgo` — profile-guided optimization. Instruments `xsh`, exercises it
  via the perf scenarios, direct interpreter scripts, startup/frontend fixtures,
  and the real-codebase showcases, then merges counters and rebuilds with
  `-Cprofile-use`. The architecture- and OS-specific merged profile is checked in
  under `perf/pgo/xsh-*.profdata`. Release builds consume it only with
  `RELEASE_USE_PGO=1`. The optimized binary lands in `target/pgo/xsh`.
- `make prof-dhat` — per-call-stack allocation stats via the in-process `dhat`
  allocator (`--features dhat-heap`). The raw `target/prof/dhat.*.json` opens in
  `dhat/dh_view.html`; `tools/dhat-summarize.xsh` writes the top-sites text and a
  diffable `*.summary.json`.
- `make prof-callgrind` — deterministic per-function instruction (Ir) and call
  counts. Ir does not vary run-to-run, so it is the before/after regression
  signal.
- `make prof-cachegrind` — per-function cache + branch-misprediction simulation
  (advisory; miss counts are address-sensitive).

By default, `make prof-dhat`, `make prof-callgrind`, and `make prof-cachegrind`
run the `PROF_SCENARIOS` matrix: `extension-count`, `value-churn`,
`record-stream`, `stream-heavy`, and `parse-check-heavy`. Override the matrix
with `PROF_SCENARIOS="..."`, or run one workload with `SCENARIO=<name>`.
Use `XSH_PROF_SCALE=<n>` for corpus size. Artifacts land in `target/prof/` and
`target/pgo/`; merged PGO profiles are checked in under `perf/pgo/xsh-*.profdata`.

### Profile-guided optimization (`make prof-pgo`)

PGO has three phases, all on the `profiling` profile, net-excluded, xsh-only:

1. **generate** — build an instrumented `xsh` (`-Cprofile-generate`), then
   exercise it through representative xsh-binary execution: each
   `perf/scenarios/*.xsh` over a generated corpus, direct interpreter scripts,
   startup/frontend fixtures, and the analysis showcases (`tokei`, `loc`,
   `ecount`, `file-report`, `secret-scan`, `todo-scan`, `dedup`, `rgrep`) run
   over this repo's own source. Counters land as `.profraw`. `cargo test` is
   excluded because its subprocess tests need the cov-style multi-binary shim
   PATH that conflicts with this xsh-only build; `cargo bench` is excluded
   because the bench binary links dev-dependencies that would bloat the
   checked-in profile with counters the release never uses.
2. **merge** — `llvm-profdata merge` into
   `perf/pgo/xsh-<rust-host-triple>.profdata`.
3. **use** — rebuild `xsh` with `-Cprofile-use` and write the optimized binary
   to `target/pgo/xsh`.

```sh
make prof-pgo                       # XSH_PROF_SCALE=<n> to size the corpus
```

The merged profiles are committed under `perf/pgo/xsh-*.profdata`. Release
builds consume them only with `RELEASE_USE_PGO=1`. Regenerate the matching
profile with `make prof-pgo` on native musl hardware after meaningful code
changes or a toolchain bump; see `perf/pgo/README.md` for the maintenance rules.

The scenarios and showcases together cover the interpreter broadly: startup,
fs walks, multi-language parsing, regex, records, and pipelines over both
synthetic corpora and this repo's own source. `-Cllvm-args=-pgo-warn-missing-function`
keeps functions absent from the profile, such as std, the net feature, or code
drift, non-fatal.

### Allocations per call stack (`make prof-dhat`)

Heap profiling is done in process with the `dhat` crate (`--features dhat-heap`,
which installs `dhat::Alloc` as the global allocator) rather than Valgrind
DHAT, whose malloc interception is written against glibc and unreliable on
musl. Because dhat-rs is the allocator, it needs no interception and works
natively on the shipped musl target.

```sh
make prof-dhat SCENARIO=extension-count
```

`tools/dhat-summarize.xsh` prints the top sites by total bytes and writes a
normalized summary. The raw `target/prof/dhat.*.json` opens in
`dhat/dh_view.html` for the full interactive call tree. dhat also reports live
bytes at exit, which covers the leak-detection angle (`-Zsanitizer=leak` is
unsupported on musl).

To attribute allocations to a single XSH-language call site, read the Rust call
stack: `dhat::Alloc → __rust_alloc → <the allocating Rust function> →
<interpreter frames>`. The summarizer skips the pure-allocator plumbing frames
when picking the line to display.

### Instructions, cache, and branches

Callgrind simulates the CPU and counts instructions read (`Ir`) per function.
It does not touch the allocator, so it runs fine on a static musl binary. `Ir`
is deterministic and does not vary run to run, which makes it the regression
signal; wall-clock from `perf` is noisy, instruction counts are not.

```sh
make prof-callgrind SCENARIO=extension-count
make prof-cachegrind SCENARIO=extension-count
```

Cache-miss counts are address-sensitive and not perfectly stable, so they are
advisory: useful for spotting a hot data-structure access pattern, not for
gating.

### Validate an optimization (`make prof-compare`)

```sh
make prof-compare                       # 816ea0a (btreemap/fxhash) vs its parent
make prof-compare BEFORE=<rev> AFTER=<rev> SCENARIO=manifest-hash
```

This builds each revision in an isolated git worktree, runs Callgrind over a
shared corpus, and diffs total instructions with `perf/callgrind-compare.xsh`.
"after" should execute fewer instructions. For an allocation diff, run
`make prof-dhat` at each revision and compare the summaries:

```sh
target/release/xsh perf/dhat-compare.xsh -- before.summary.json after.summary.json
```

Per-function attribution can be smeared by thin-LTO inlining (small hot functions
fold into callers); the totals and large buckets are stable.

Run a larger generated corpus and request syscall tracing (Linux container):

```sh
make perf-linux XSH_PERF_SCALE=32
```

On macOS, use the Linux test container for the direct
`examples/extension-count.xsh` comparison:

```sh
make perf-linux-extension-count
```

Use the Linux test container for the generated-corpus scenarios:

```sh
make perf-linux
```

Generate a native CPU flamegraph for the `extension-count` scenario:

```sh
make perf-linux-flamegraph
open target/perf/extension-count.svg
```

Run each standalone showcase script's matching `showcase/tests` suite with
allocation counters, platform memory stats, `/usr/bin/time`, and Linux syscall
summaries:

```sh
make perf-linux-showcases
make perf-linux-showcases SHOWCASE=csv-query
```

Generate per-showcase native CPU flamegraphs as well:

```sh
make perf-linux-showcase-flamegraphs SHOWCASE=csv-query
open target/perf/showcase-tests-*/csv-query.svg
```

The flamegraph target repeats each selected showcase until it has run for at
least one second by default. Tune short-suite sampling with:

```sh
make perf-linux-showcase-flamegraphs SHOWCASE=csv-query XSH_PERF_MIN_DURATION_MS=5000
make perf-linux-showcase-flamegraphs SHOWCASE=csv-query XSH_PERF_REPEAT=100
```

The underlying runner also accepts these options directly:

```sh
target/release/xsh perf/showcase-tests.xsh -- --showcase csv-query --flamegraphs --repeat 100
target/release/xsh perf/showcase-tests.xsh -- --showcase csv-query --flamegraphs --min-duration-ms 5000
```

Showcase perf artifacts are written under
`target/perf/showcase-tests-YYYYMMDD-HHMMSS/`. For each showcase, the runner
emits:

- `NAME.stdout` and `NAME.stderr` for the timed `xsht test` run, including
  allocation counters and repeat summaries when built with
  `--features perf-metrics`.
- `NAME.strace`, `NAME.strace.stdout`, and `NAME.strace.stderr` when syscall
  summaries are requested.
- `NAME.perf.data`, `NAME.perf.script`, `NAME.folded`, `NAME.svg`, and
  `NAME.top` when flamegraphs are requested.

The container targets build `Dockerfile.test` and run XSH with
`--cap-add SYS_PTRACE --security-opt seccomp=unconfined`; those flags are
required for the native ptrace supervisor.

The flamegraph target also needs Linux `perf`, installed in `Dockerfile.test`
as the Alpine `perf` package. It runs the container with elevated perf
permissions, records native Rust stacks with `perf record -g`, converts
`perf script` output through `showcase/perf-collapse.xsh`, and renders the SVG
with `showcase/flamegraph.xsh`. Artifacts are copied out of the Docker target
volume after the run. The key artifacts are:

- `target/perf/extension-count.perf.data`
- `target/perf/extension-count.perf.script`
- `target/perf/extension-count.folded`
- `target/perf/extension-count.svg`
- `target/perf/extension-count.top`

On Linux, `--syscalls` uses `xsht trace --syscalls`, which supervises the
measured script with native ptrace syscall tracing. macOS syscall tracing is
not supported; use the Linux container targets for syscall data.

```sh
target/release/xsh perf/compare-extension-count.xsh -- --syscalls
```

Syscall reports include total syscall count/time plus a sorted
`top_syscalls_by_count` summary, per-stage top syscall counts, and per-process
top syscall counts. The `*.syscalls` artifacts are XSH trace files with the
native syscall summary appended.

The runner builds the release `xsh` binary with `--features perf-metrics`. Pass
`--xsh` after the script argument separator to measure an existing binary. Use
`--profile profiling` only when native symbol attribution is needed; the `dist`
profile is for CI release packaging, not local perf iteration.

## Aggregate allocation counter (perf-metrics)

`--features perf-metrics` + `XSH_PERF_ALLOC=1` installs `CountingAllocator`
wrapping `MiMalloc`, giving process-total alloc/dealloc/realloc counts and a
size histogram (`<=16b`, `<=64b`, `<=256b`, `<=4096b`, `>4096b`) plus peak RSS
on stderr. This is the cheap, always-available aggregate; `make prof-dhat` is
the per-call-stack upgrade.

```sh
XSH_PERF_ALLOC=1 cargo run --release --features perf-metrics --bin xsh -- SCRIPT \
  2>&1 | grep "xsh perf:"
```

| Field | Meaning |
|---|---|
| `allocation_calls` | `alloc` invocations (`Box::new`, `Vec`/`String` growth, …) |
| `allocation_bytes` | Sum of `layout.size()` passed to `alloc` |
| `reallocation_calls` | `realloc` invocations; high values mean `Vec`/`String` growth reducible with `with_capacity` |

`perf/run.xsh` writes these as `allocation.json`; `perf/allocation-compare.xsh`
diffs against `perf/allocation-baseline.json`. `perf-metrics` and `dhat-heap`
each install a `#[global_allocator]`, so they are mutually exclusive. If both
are enabled, as in `cargo clippy --all-features`, `perf-metrics` takes
precedence and dhat goes inert, so the crate still builds with a single
allocator.

## Showcase-vs-native corpus benchmarking (tokei.xsh)

`showcase/tokei.xsh` reimplements a `tokei`-style line counter in XSH. Pointing
it at a large real checkout and comparing wall time and peak RSS against the
native `tokei` binary is the methodology behind the `tokei.xsh` Native Parity
stretch goal tracked in `INTERPRETER-PERF.md`; current numbers, accepted and
rejected trials, and the byte-parity gate live there, not here — this section
is only the reusable "how to take a sample" reference.

Build a plain release binary first. A `perf-metrics` build installs a
different allocator and changes both wall time and peak RSS, so it is not a
valid stand-in even though it also builds `--release`:

```sh
cargo build --release --bin xsh
```

Sample serially, one command at a time, nothing else running. `/usr/bin/time
-l` (macOS) reports `real` wall time and `maximum resident set size` right
after the command it wraps:

```sh
/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- ROOT          # table
/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- --json ROOT   # JSON
/usr/bin/time -l tokei ROOT                                             # native, table
/usr/bin/time -l tokei -o json ROOT                                     # native, JSON
```

The showcase's own flag is `--json`; native tokei's `-o json`/`--output json`
is not accepted and errors out (`unknown argument at argv[0]: --output`)
rather than silently doing the wrong thing, so a bad flag is easy to notice.

Take several samples of each and use the tightest cluster, not the mean of
everything — `real` and RSS both swing by up to ~2x under concurrent load
(another build, another benchmark, even a busy background process), and a
contaminated sample must be discarded rather than averaged in. Treat native
`tokei` as a performance baseline and an accuracy lens only, never the output
oracle: XSH may intentionally differ from native tokei's line classification,
child-language treatment, JSON field order, and report ordering.

The actual correctness gate is XSH-vs-XSH, not XSH-vs-native: save a
known-good table/JSON output before a change, then `cmp` a fresh run against
it after. These snapshots are plain scratch files, not checked in (`target/`
is gitignored), so you own saving your own "before" copy:

```sh
mkdir -p target/perf/tokei-current
target/release/xsh showcase/tokei.xsh -- ROOT > target/perf/tokei-current/table-before.txt
target/release/xsh showcase/tokei.xsh -- --json ROOT > target/perf/tokei-current/json-before.json
# ...make the change, rebuild...
target/release/xsh showcase/tokei.xsh -- ROOT > target/perf/tokei-current/table-after.txt
cmp target/perf/tokei-current/table-before.txt target/perf/tokei-current/table-after.txt
```

A large real checkout exercises the full language/extension mix, gitignore
rules, and file-size distribution that the stretch goal cares about better
than the synthetic `perf/make-corpus.xsh` tree does. The checked-in
`INTERPRETER-PERF.md` samples use `/Users/josh/dev/sentry` (~3.1 GB, ~140,000
files); any comparably large, gitignore-heavy, multi-language repo works as a
substitute.

## Architecture notes

- **Why musl, not glibc.** We profile the shipped target. Valgrind's malloc
  interception is the only thing that struggles on musl, and we sidestep it
  entirely by doing heap profiling in process. Callgrind and Cachegrind are pure
  CPU simulators that do not touch the allocator, so they run fine on musl.
- **Why dhat-rs over Valgrind DHAT.** Valgrind's heap interception is written and
  tested against glibc and is unreliable on musl. Installing `dhat::Alloc` as
  the global allocator profiles allocations from the inside: no interception,
  fully musl-native.
- **Why Callgrind for regressions.** Instruction counts are deterministic; a
  drop in `Ir` is a real win independent of machine noise. Wall-clock and cache
  misses are not, so they stay advisory.
- **LTO smearing.** thin-LTO + `opt-level=3` inlines small hot functions into
  their callers, so some functions vanish from per-function attribution or land
  in the caller. Totals and large buckets are stable; read
  `callgrind_annotate --auto=yes` for source-line granularity when a function
  disappears.
- **Coverage vs PGO instrumentation.** `-Cinstrument-coverage` and
  `-Cprofile-generate` produce different, non-mergeable `.profraw`, so
  `make cov` and `make prof-pgo` each compile and run their own instrumented
  pass with isolated target dirs and Docker volumes.
