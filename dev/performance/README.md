# Performance tools

This directory holds the workload catalogues, the focused, Tokei and Laputa
measurement adapters, and their shared helpers. `check.py` owns those helpers:
strict JSON reading, explicit absolute paths, bounded Git queries, SHA-256
digests, and `checked_docker`, which runs one owned container and resolves its
cleanup. It requires Python 3.11 or newer and adds no production dependency.
`xsht runtime-stats` supplies phase diagnostics.

XSH-19 retired the campaign receipt validators that `check.py` used to
dispatch: functional close, the inherited baseline close, the representative
measurement, and the inference performance runner. Each validated
consolidation receipts through the former campaign gate tools. The owner-approved
[performance baseline](../../docs/TESTING.md#performance-baseline) replaces
them with ordinary readiness-checked interleaved measurements. Their code is in
Git history at `d03e90f45`.

## Owned containers

`owned_containers.py` labels each container with an owner and stage, writes a
cidfile, and removes exactly the labelled container after the command exits,
times out or is cancelled. `checked_docker` keeps the child's status and output
with any cleanup failure. Unresolved cleanup retains the cidfile under the
caller's scratch directory and fails.

## Runtime phase diagnostic

`xsht runtime-stats --json REPORT SCRIPT` reports allocation, status, output
size, and phase durations. Wall time uses a monotonic clock. CPU time counts
threads in the XSH process and excludes descendants. Both clocks include
observer overhead, so use the fields for attribution only. Ordinary execution
does not collect phase data. A `null` phase was not reached; zero is an observed
zero-length phase. The report schema and callable boundaries live in
`crates/xsht/src/stats.rs` and `src/runtime_stats.rs`.

## Separate process-tree attribution

Use a representative `SCRIPT` and the profiling build described in
[Profiling](../../docs/TESTING.md#profiling). Run each command separately, never
with a measured sample:

```sh
perf record -g --inherit -o .work/tmp/tree-perf.data -- "$PROFILING_BIN/xsh" SCRIPT
valgrind --tool=massif --trace-children=yes --massif-out-file=.work/tmp/massif.%p "$PROFILING_BIN/xsh" SCRIPT
strace -f -c "$PROFILING_BIN/xsh" SCRIPT
```

`perf` samples process-tree CPU with sampling overhead. Massif attributes heap
allocations in each traced process with substantial instrumentation overhead;
it does not measure a simultaneous tree-wide memory peak. Inspect its per-process
files with `ms_print`. `strace` counts syscalls across the tree with tracing
overhead. None of these instrumented runs is a timing sample.

## Inference measurements

`inference_workloads.py` generates annotated and reduced source maps for the
nine inference families at 64, 128, 256, and 512 units. The original eight
families and 36 cases are unchanged. Named generic constructors add four shapes:
independent binders, shared defaults, ground return carriers and explicitly
anchored phantom contexts, for 52 cases in total. Deep-record
constructors and projections keep each syntax expression shallow while the
named record types retain the requested semantic depth. The 64, 128, and 256
depth outcomes remain pending probes. The 512 deep-record input must reach the
semantic type-depth rejection with documented check exit status 2 for both
source variants and both compilers. It has no runtime distribution.

The module-diamond source sums its `2 * size` imported values through `size`
shallow accumulator bindings, so the source measures shared imports without a
single expression reaching the parser nesting limit. The stream sizing source
uses one `Stream[Int]` producer with inputs 7 and 9; it has one item type.
Native inference capability fixtures cover independent `Int` and `Str` stream
instances separately.

`workloads.py` keeps the original fifteen cases first: two startup cases, six
streaming cases, three JSON cases, three directory cases, and one 128-child
spawn case. It appends six demanding data cases: JSON and directory workloads
at package-range sizes 16,384, 32,768, and 65,536. Case names and shapes
include the package-range size. The JSON fixture adds one duplicate input
record, and the workload adds one new record. `completed_records` reports the
exact output count. `bench/consolidation/fixtures.py` owns the fixture bytes,
modes, links, and goldens.

The largest directory fixture contains 34,309 entries, including 32,768
regular files. The JSON and directory fixtures use synthetic package data and
a warm filesystem.

Only the original fifteen cases have R13 provisional observations. The six
demanding cases require a new same-source diagnostic and timed cohort. Reports
from the R13 run retain their original fifteen-case denominator.

Every case requires status zero, empty stderr, exact stdout, and unchanged
working-directory effects. Useful output is the first complete expected stdout
line on the host pipe, not first-byte latency. Timing ends after process exit
and full stdout and stderr capture. Failed checks, timeout, or unresolved
cleanup receive no timing credit.
Helper attribution checks pipeline completions against explicitly declared
segment counts and distinct trace event IDs. It rejects incomplete pipelines
and mixed pipeline and ordinary helper traces. An incomplete pipeline trace cannot establish a zero-helper count.
Ordinary run completions must match distinct starts. Successful non-detached
spawns must match their ready and wait events; a no-op cancel after wait is
accepted. Live cancellation, detached children, `wait_any`, and `wait_ready`
remain incomplete. Trace records are streamed from regular files, with a 128 MiB
file limit and a 1 MiB record limit. Missing root events, event-ID gaps,
truncated, changed, or malformed streams are incomplete.

Common and demanding names do not assert a measured cost. The later accepted
baseline must establish which substantial cases exceed 50 ms of CPU and qualify
resource, allocation, syscall, and completed-work counts. Post-inference source
and products remain the final measurement baseline.

### Canonical inference work

`xsht frontend-stats --json` reports an `inference_work` object with exactly
these twenty nonnegative integer fields on each entry-root record. Solver counts
cover parse, load, checker, imported-module, and speculative-probe work in one
measurement invocation. Cloned constraint probes share that invocation's
counters, so failed and rolled-back work contributes once. The collector is
invocation-scoped; ordinary checks do not retain a global statistics record.

| Field | Counted work or output |
|---|---|
| `fresh_variables` | Type, row, and effect inference cells created, including cells created by failed probes. |
| `generated_constraints` | Submitted type and row constraints and effect predicates, including speculative work. |
| `representative_hops` | Parent links traversed while finding type and row representatives. |
| `compression_writes` | Type or row parent links changed by path compression. |
| `unifications` | Type and row unification pairs processed. |
| `occurs_visits` | Type nodes visited by occurs checks. |
| `row_field_visits` | Record fields inspected while solving row constraints. |
| `scheme_instantiations` | Callable schemes instantiated by the checker. |
| `successful_generic_call_resolutions` | Generic user-call resolutions that completed successfully. |
| `worklist_wakeups` | Type and row solve steps processed plus effect predicates and callable applications enqueued for solving. |
| `host_candidate_trials` | Host overload candidates whose constraints were tested. |
| `host_obligation_requeues` | Host method obligations requeued while receiver types remain unresolved. |
| `rollbacks` | Constraint transactions and candidate probes whose journal was undone. |
| `rollback_changes` | Journal entries undone by those rollbacks. |
| `level_adjustments` | Type, row, or effect cell levels lowered during solving. |
| `effect_edge_visits` | Effect dependency edges inspected during deduplication, binding, fixed-point solving, and contract walks. |
| `candidate_tests` | Calls to the canonical constraint-trial operation, including failed probes. |
| `diagnostic_work` | Diagnostic records constructed across the source invocation, including discarded checker probes and reveal types cleared before publication. |
| `output_type_nodes` | Structural `Type` and `SchemeType` nodes in the final compact declaration and body outputs, including call-instantiation type and row arguments; shared `Arc` payloads count once. |
| `evidence_nodes` | Final compact effect facts, static callable aliases, stream-stage plans, body evidence records, and nested call-instantiation evidence. |

Each retained scheme-body expression fact, tail fact, registered-call plan,
read binding, constructor role and Proc return site counts as one evidence node.
Constructor roles and return-site IDs do not create output type nodes. Runtime-owned callable substitutions remain
outside these frontend output counts.

The same record reports `function_body_check_entries` separately. It counts
actual Pure, Proc, Stream and signal-hook body entries across the root,
imported modules and checker probes; repeated generic calls may increase `scheme_instantiations` without
increasing source body checks. `output_type_nodes` and `evidence_nodes` are
counted from the final merged compact output once, not reconstructed from
solver counters or intermediate checker snapshots. `semantic_retained_bytes`
remains a separate retained-typing measure.

### Fixture qualification scope

The 2026-10-18 check used the immutable `laputa-xsh-seed-1` products from clean
source `429f6ff080efac12e5706ab45ec05539a24e57e6`, with the seed manifest's
`release, lto=false, incremental` profile and declared net, tools, and native-test
features. All fifteen frozen cases passed exact stdout, zero status, empty
stderr, useful-line observation, and unchanged working-directory checks.
Both timing fields remained null. Product hashes matched the existing seed
manifest before, inside, and after the pinned offline container.

The original 2,048-record fixture retained its pinned manifest. The earlier
tranche using mutable shared product paths remains unqualified because those
paths changed during verification. Its passing outputs do not supply source
credit. This qualification covers the selected fixture behavior on the seed
products; it does not establish current canonical acceptance, CPU thresholds,
observed helper counts, first-output latency, or a performance baseline.

## Tokei Rust corpus projection

`dev/performance/tokei_corpus.py` projects tracked, regular, non-hidden `.rs`
files from uutils commit `e7c9f3194280835c4487c2945c68d5f01ccacc8d`. It
requires at least 500 files and limits Git entries, source bytes, report bytes,
and projection traversal. The selected revision must be available in a local
uutils checkout.

Run `project-uutils-rust --source-root ROOT --output PATH` with a fresh output
path under this checkout's `.work/tmp`. Run `reconcile-uutils-rust` with
`--source-root ROOT`, `--projection PATH`, `--xsh-json XSH_REPORT`, and
`--tokei-json TOKEI_REPORT` to compare selected paths with the pinned Git
objects. The helper does not execute or identify the binaries, and its result
does not qualify timing or memory measurements.

Each call gives its Git subprocesses a shared 60-second deadline. Projection
writes and report processing have byte and file limits but no end-to-end time
deadline. A staging-cleanup failure is attached to the original error.

## Run the Tokei Rust correctness scenario

Set `XSH_TOKEI_REFERENCE` to a Linux x86-64 Tokei executable that runs in the
pinned test image. Set `XSH_TOKEI_CORPUS_ROOT` to a local uutils checkout that
contains the pinned commit. Then select the scenario:

```sh
XSH_TOKEI_REFERENCE=/absolute/path/to/tokei \
XSH_TOKEI_CORPUS_ROOT=/absolute/path/to/uutils \
cargo dev gate --case showcase/tokei/uutils-rust-correctness
```

The scenario checks the full Tokei `--version` output, captures both executable
hashes and modes, and checks that the products remain stable during the run.
Tokei runs with `RAYON_NUM_THREADS=2` and `--no-ignore`. Both scanners run once
in the pinned network-disabled container. The scenario records exact file-set
reconciliation and no timing.
It also reports Rust code, comment, and blank-line counts for each scanner and
their differences. Different counts do not fail the file-set check.

The expected Tokei source commit is
`8cdd6fa3a54f8cd69442d2f00effb29aa3110353`, but the scenario cannot prove that
the executable came from that commit. It reports Tokei source provenance and
P0 qualification as unqualified.

## Source census

Run `python3 -B dev/performance/census.py --repo . --repo ../../../../laputa --json .work/tmp/census.json`
to inventory tracked XSH sources. Each repository uses its verification or
debug `xsht`, falling back to `xsht` on PATH. The ordered role rules are in
the table at the top of `census.py`.

The JSON contains `tool_version`, repository roots, HEADs and parsers, file
paths, roles and matching rules, parse statuses, SHA-256 digests, static imports
and their resolved census paths, textual dynamic loads, duplicate digest groups,
and summary counts. Unresolved static imports have a null resolved path.
