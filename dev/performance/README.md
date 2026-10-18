# Performance adapter reference

`check.py` dispatches to existing measurement and acceptance owners. Python 3.11
or newer is required. It adds no production dependency. The planned `inference`, `optimize`, `deployment`, and
`close` modes remain unavailable and exit nonzero. The existing
`xsht runtime-stats` diagnostic supplies the phase attribution below.

Run policy controls without Docker:

```sh
python3 -B dev/performance/check.py --config CONFIG self-test
```

Add `--adapter` to qualify existing release products and execute one exact-output
control in the pinned offline Linux image. This also checks rejection of wrong
output and a real malformed cleanup cidfile. It does not measure performance.
Readiness controls exercise before and after failures without a real clock.

## Configuration

The JSON configuration requires exactly these fields. All paths are explicit,
absolute, and free of symlinks. Unknown and duplicate fields are rejected.

| Field | Value |
|---|---|
| `schema` | Integer `1` |
| `source_root` | Clean XSH Git checkout root |
| `commit` | Exact 40-character lowercase source commit |
| `tree` | Exact 40-character lowercase source tree |
| `laputa_root` | Explicit isolated Laputa checkout containing `pm/` |
| `product_receipt` | Actual `canonical_gate_batch_replay` receipt, or completed `profile-cost.py` receipt |
| `xsh`, `xsht` | Exact product paths bound by the selected existing owner |
| `scratch` | Existing directory under the adapter checkout's `.work/tmp/` |
| `functional` | `null`, or the functional input object below |
| `baseline` | Optional `null`, or the baseline receipt paths below |
| `measurement` | Optional `null`, or explicit absolute `fixtures` and `output` paths |

Product qualification checks the clean commit and tree, executable paths and
hashes, successful owner result, and resolved cleanup. The profile provider
requires opt3. The canonical provider verifies bootstrap, resource and log
bindings before qualifying verification products. It issues no new product receipt. An exact-source checkout can differ from the receipt's original
build location. There is no normal-checkout fallback.

The functional object requires exactly `canonical`, `supplemental`, `skip_review`,
`structural`, `review_evidence`, and `output`. Each value is an absolute path.
The output must be under the source checkout's `.work/` and must not exist.
Functional validation requires `product_receipt` to equal `canonical`; it does
not require a prior release profile or functional-close record.

The optional baseline object requires exactly `functional_close`, `profile`,
`observer`, `forced`, `comparative`, and `scheduler`, each an absolute receipt
path. Omitting the object preserves existing schema-1 configurations, but cannot
pass baseline acceptance. `functional_close` must equal `functional.output`.

The measurement fixtures directory contains the frozen `2048`, `4096`, and
`8192` fixture roots. The output must be a fresh JSON file below `scratch`.
Measurement regenerates references through the existing fixture maker and
compares all bytes, modes, entries, links, and goldens before launching work.
The default 2048 manifest retains its existing pinned validation.

## Functional validation

```sh
python3 -B dev/performance/check.py --config CONFIG functional --check
```

The command validates the actual `final_canonical_replay.py` receipt, its pinned
command and image, resource record, bootstrap build, product files, logs, source
checks, and cleanup. It validates required product builds, harness builds, warning-deny commands,
all five current Linux contexts, product stability, harness stability, and log bindings.

The command runs `metrics.xsh check-current --review-evidence` in the pinned image.
This full scan of both Rust source roots has a 900-second bound, matching the
historical full structural gate budget. The focused adapter self-test retains
its 60-second bound. Each private Docker call selects its bound explicitly;
timeout still requires owned-process/container cleanup and never supplies credit.
The structural scan container uses the invoking UID and GID to preserve Git
ownership checks on the mounted checkout and writable scratch.
The existing metrics owner measures current source, validates the structural
report, and checks all nine functional semantic review rows. The report retains
all ten canonical rows. Functional evidence excludes exactly `timings`; final
review still requires `timings`. Neither a bare boolean nor launcher success can
close functional acceptance.

The adapter's `skip_review` input records a human review against exact receipts.
It requires exactly these fields:

| Field | Value |
|---|---|
| `source_commit`, `source_tree` | Exact configured source identities |
| `canonical_sha256`, `supplemental_sha256` | Hashes of the reviewed receipts |
| `reviewer` | Nonempty reviewer identity |
| `contexts` | Object containing `canonical` and every supplemental `linux-special-*` step |

Each context requires exactly `result: "review_verified"` and a nonempty `reason`
that states the skip disposition. Review every mandatory skip in the bound logs;
an unresolved mandatory skip prevents a verified context. This input records a
review, not an automatically established privilege result.

After all checks pass, the command publishes the existing functional-close shape
with the exact commit, tree, reviewer, products, scope, and evidence bindings.
Publication refuses overwrites and exposes only a complete JSON record. It
rechecks source and products before publication. No full successful close has
been established by the adapter foundation tests.

## Canonical bootstrap ownership

`final_canonical_replay.py::validate_bootstrap_receipt` owns the bootstrap schema.
Its `BOOTSTRAP_CARGO_ARGV` requires a verification `xsh` build with
`--message-format=json-render-diagnostics`. The host target directory
`.work/target-final-canonical` mounts at
`/work/.work/target-final-canonical`, so the target mount point already exists in
the read-only source mount. Its Cargo artifact record must name
`/work/.work/target-final-canonical/x86_64-unknown-linux-musl/verification/xsh`.
The receipt binds the clean source, pinned image, exact Rust flags, Cargo command,
successful Cargo JSON stream, and executable bytes. The JSON stream and receipt
live under the selected source checkout's `.work/tmp/`.

`final_canonical_replay.py bootstrap` is the producer for this exact command and
receipt. The supplemental launcher's broader product command and target mount do
not satisfy the canonical bootstrap contract, and its product receipt does not
substitute for the canonical bootstrap receipt. The producer retains the exact
executable bytes as `<receipt directory>/xsh`; the existing artifact path, hash,
and size fields identify that executable copy. This keeps the build receipt
valid after later Cargo commands replace the mutable target executable. Before
launching the canonical batch, the owner verifies that the mutable target still
matches the retained copy. The reader validates the retained copy independently
of later target changes.

## Inherited baseline close

```sh
python3 -B dev/performance/check.py --config CONFIG baseline --check
```

This command validates existing completed receipts for canonical functional close,
the normal release profile, timed observers, forced frames, comparative parity and
samples, and three scheduler pairs. It uses the frozen checkout's owner readers,
checks exact source and product bindings, and replays the existing scheduler
manifest, log, build, and resource validators. Missing receipts, artifact-only
profiles, discarded timings, changed products, and unresolved cleanup fail.
It creates no baseline receipt and runs no replacement benchmark suite.

Profile and forced-frame receipts must bind their required `--laputa-corpus`
selection in `laputa_corpus`: the explicit absolute `path`, clean Git
`source_identity`, and equal `source_identity_after`. Both receipts must match
the current clean `config.laputa_root` commit and tree before and after baseline
validation. The forced-frame executed Docker command must mount that same root
read-only at its own path and select it through `XSH_LAPUTA_CORPUS`.
Historical receipts missing these owner bindings cannot close baseline acceptance.

The scheduler receipt bridge runs the unchanged reference-build launcher from
its own reviewed tool checkout. Candidate copies remain under the frozen source.
Reference targets and Docker argv captures use the shared controller's ignored
gate directory. The bridge validates the passed profile's clean Laputa binding
and the captured inner Docker build command before publishing a reference receipt.
Use the same run ID for bridge and scheduler comparison outputs.

The forced container keeps `--network none` and maps its `xsh-test` hostname to
`127.0.0.1`. It mounts `/usr/local/bin/mold` read-only using the executable qualified
by the passed profile's `toolchain.mold` record. The existing schema-1
`inputs_before` and `inputs_after` require the same `mold` executable snapshot
alongside `source` and `binaries`. Missing, unsafe or changed tools and incomplete
older receipts fail validation. Preserve failed output directories and select a
fresh `forced-frames-COMMIT-attempt-LABEL` directory for a new attempt.

The supplemental receipt must carry the same `laputa_corpus` binding. Each
build, warning-deny, and `linux-special-*` container step must select that path
with one read-only bind mount and one `XSH_LAPUTA_CORPUS` value. The
`stage-core-compat-extra-xsht` step copies the qualified `xsht` product into the
harness target. The stage validator checks its source and destination hashes,
target paths, pinned image, mount modes, and exact Docker argv, and requires the
product, harness, and source-scratch mount roots to remain disjoint from the
corpus. Helper steps bind their
NUL-terminated `docker_argv_capture` bytes by path and SHA-256 and must match the
decoded vector to `docker_argv`; direct Docker steps must match `docker_argv` to
the recorded step command. Missing, stale, dirty, duplicate, or mismatched
bindings fail functional close.

Util-linux close requires its existing in-memory independent review. The standalone
CLI fails before expensive effects when that context is absent. The parent can
load this module, obtain `runtime = baseline_util_linux(config)`, and invoke
`baseline(config, runtime)` inside `runtime.independent_review(actual_gate_result)`.
The result must come from actual independent review; this adapter provides no
serialized review import or fabricated result. Executable delivery of that review
remains required before standalone acceptance can close.

After receipt and review validation, `baseline` calls the existing util-linux
acceptance close under its native/Cargo leases and release-product scope. This is
an expensive stage: its full native suite uses private mount, IPC, and PID
namespaces with `SYS_ADMIN`, followed by host/API, repository, documentation,
resource, review, and cleanup checks. These private boundaries require separate
coverage from canonical correctness. Close owns resource validation; the adapter
does not invoke a second resource pass. Existing owner checkout/output constraints
remain in force. Source, receipt bindings, and products are checked again after
close. No accepted current-source baseline exists yet.

`baseline` binds `config.laputa_root` for the entire close. Release builds,
resource gates, documentation gates, and the paired build driver use that path
for their read-only mount and `XSH_LAPUTA_CORPUS` value. The receipt reader checks
the configured path and its clean commit, tree, and status before and after close.
Direct `dev/util-linux/check.py resources --check` and `close --check` commands
require `--laputa-corpus PATH`. Harness and parity commands do not use a corpus.
Each corpus scope verifies a clean Git identity before work and again when it
ends, including direct CLI runs.

## Representative measurement

Measurement requires the actual same-source util-linux `CloseReport`. The
existing `acceptance.close(runtime)` now returns that report after validation;
`acceptance.validate_close_report(runtime, report)` rechecks clean current
source, selectors, resources, readiness, and cleanup without rerunning suites.
`baseline()` returns the same report. The reviewed coordinator can run both
operations in one process:

```python
runtime = check.baseline_util_linux(config)
with runtime.independent_review(actual_gate_result):
    closed = check.baseline(config, runtime)
    report = check.measure(config, closed)
```

`measure --check` without this live prerequisite fails before timing. The
standalone CLI cannot deserialize or synthesize acceptance. The same-process
flow avoids a second private full suite. Measurement validates inherited
receipts and the normal completed release opt3 profile, including the existing
producer's exact build command, default features, pinned image, and build input
files. It selects the release products from that explicit profile receipt;
the config's canonical verification products continue to qualify functional
close. There is no product fallback or build inside measurement.

The existing hostpipe sampler runs all 15 frozen cases. Each case retains
exact stdout, empty stderr, successful status, effects, deadline, and owned
process-group checks. Samples retain the existing three rounds, with 60 fresh
startup processes or 30 other processes per round, after parity and warmup.
First useful output means the first complete expected line observed on the
pipe; completion requires leader exit and drained stdout/stderr. Accepted
samples bind complete consumed output through the existing SHA-256 field;
failed output remains in the report. Reports flush at round boundaries and on
failure, rather than serializing growing samples after every invocation.
Attribution sidecars live under each unique diagnostic run directory, so reports
sharing one output parent cannot overwrite each other's diagnostic evidence.
A baseline has one selected product;
baseline/candidate comparisons still require the campaign's alternating pairs.

The host runs `perf-gate-ready` immediately around the entire timed Docker
batch, with `-1` immediately after success or failure. This shared window
accepts or discards every sample together. Attribution runs precede the window
and supply no timing credit. Each Docker batch has a 900-second deadline;
each workload invocation retains the existing 60-second bound. Failures retain
their report, and publication errors never replace the primary failure.

The sampler adds `cpu_ns`, `max_rss_bytes`, and `resource_scope` to existing
observations. Linux `wait4` accounts for the owned leader only after group
cleanup while that leader remains unreaped. CPU is user plus system CPU and
includes descendant usage only when the leader actually waited for it. RSS is
the kernel's returned `ru_maxrss`, converted from KiB; it is neither summed
process-tree RSS nor cgroup memory. Missing accounting is `null` and cannot
pass measurement. Existing resource readers retain separate container-lifetime
cgroup peak snapshots for the timed and diagnostic batches, explicitly including
their controller overhead. Binary sizes come from the qualified release files.

Each case separately executes existing ordinary `runtime-stats` and direct
`xsht trace --syscalls` diagnostics with the same output/effect checks.
Runtime reports supply ordinary runner phase CPU/wall and allocation traffic.
Trace reports count syscalls of the instrumented indexed XSH process; detached
external helper descendants are excluded. Process events independently enforce
the frozen direct helper count. The stronger uninstrumented ordinary CLI or
whole-process-tree syscall metric remains `production_syscalls: null`.
Instrumented syscall and phase durations are never startup timing samples.
Raw reports and diagnostic files provide measurement evidence; they do not
assert a performance improvement or complete campaign acceptance.

## Limits

Policy tests cover config rejection, product identity, output comparison, failed
builds, cleanup, readiness ordering, canonical rejection, and required supplemental
contexts. They do not replace full acceptance, resource qualification, workload
measurement, phase-observer validation, or final timing gates.

Unresolved owned-container cleanup retains its cidfile under `scratch` and fails.
The adapter uses the existing owned-container cleanup implementation. It does not
create a new container supervisor or benchmark runner.

## Runtime phase diagnostic

The Rust allocation API keeps `MeasuredScriptRun { output, allocation }`,
`run_script`, and `MeasuredScriptRun::to_json` unchanged. The additive tooling
entrypoint `run_script_with_phase_report` runs once and returns that allocation
result plus the phase-bearing JSON string used by `xsht`. Phase types remain
crate-private.

`xsht runtime-stats --json REPORT SCRIPT` preserves its existing allocation,
status, and output-size fields. The additive `wall_time_ns` object uses monotonic
wall-clock nanoseconds. Instrumented durations include observer overhead and
supply diagnostic attribution, not performance acceptance samples.

| Field | Observed callable boundary |
|---|---|
| `preparation_inclusive` | Entire ordinary runner preparation, including entry-file reads, source decoding, setup, and cleanup |
| `project_parse_load_exclusive` | Project module-root discovery and `parse_load_entry_source_arena_only` |
| `check_solve_fact_build_exclusive` | `Checker::check_arena_with_sources_options_and_type_program` |
| `declaration_publication_exclusive` | `Checker::compact_declarations` transforms checked output into compact declarations |
| `lower_verify_install_exclusive` | `Evaluator::prepare_checked_compact_indexed_only` |
| `preparation_other_exclusive` | Inclusive preparation minus the four disjoint observed preparation calls |
| `execution_finish_exclusive` | Prepared evaluation through `finish_run_attempt`, including execution-thread scheduling and final output processing |
| `diagnostic_finish_exclusive` | Finishing a preparation failure, including any diagnostic re-preparation |

The four observed preparation calls do not overlap. Their durations plus the
residual equal `preparation_inclusive`. Execution and diagnostic finishing sit
outside preparation. Only one finishing field is populated for a run.
A `null` phase was not reached; zero means an observed zero-length duration.

The tooling-only report also adds `cpu_time_ns`, with `clock: "process_cpu"` and
the same fields, callable boundaries, null semantics, and separate reconciliation.
The existing rustix process CPU clock counts all threads in this XSH process.
Preparation CPU distinguishes frontend work from blocking and scheduling waits;
execution CPU includes the evaluator and runtime worker threads. Descendant
process CPU is excluded, so the host sampler still owns whole-workload CPU.
Parallel process CPU can exceed wall time; subtracting CPU from wall does not
measure wait time. Both clocks include diagnostic observer overhead and supply
attribution, not accepted performance samples. The existing Rust allocation
result and its `to_json()` output remain unchanged.

The checker call combines constraint generation, solving, and fact construction.
The evaluator preparation call combines lowering, semantic verification,
installation, and dispatch preparation. These diagnostics do not pretend to
separate work inside either call. Imported modules participate in the same
ordinary preparation path.

Ordinary execution supplies no observer, reads no phase clock, and allocates no
phase records. The diagnostic uses fixed fields and reads the clock at coarse
call boundaries. It does not instrument statements, instructions, or output.
Buffered output becomes available after evaluation returns; that boundary is
completion, not first useful output. M1 must observe the host pipe for first
output latency.

## Frozen workload selection

`workloads.py` freezes fifteen cases before candidate measurements. It supplies
scripts, arguments, exact stdout, and expected helper-process counts to existing
samplers. It does not time workloads or issue acceptance records.

| Family | Common input | Demanding input | Scaling series |
|---|---|---|---|
| Startup | Existing `cold_trivial.xsh`, `hello\n` | `cold_template.xsh`, `hello xsh\n` | Not applicable: each is one fresh invocation |
| Streaming | Existing `stream-stage-cost.xsh`, count and map/where, 65,536 items | Same modes, 262,144 items | 65,536, 131,072, 262,144 items |
| Algorithms | Existing comparative JSON replace/append/name-sort, 2,049 input records | Same program, 8,193 input records | 2,048, 4,096, 8,192 base records plus the retained duplicate |
| Filesystem and process orchestration | Existing comparative directory traversal and 128 sequential external printf children | Same traversal with four times the directory count | 32, 64, 128 directories, each with 32 ordinary files; hidden entries and links retained |

For scaled cases, `n` is common and `4n` is demanding. The fixed spawn case has
128 children and no artificial scaling claim. The JSON and directory scripts
are unchanged. The existing comparative fixture generator owns all their input
bytes, modes, links, and independently computed golden outputs. Its new explicit
`package_rows` choices are 2,048, 4,096, and 8,192. The default remains 2,048 and
keeps the existing pinned manifest unchanged.

Generate each selected fixture tree with that owner under an ignored scratch
root. `frozen_workloads` reads the owner's expected bytes from trees named
`2048`, `4096`, and `8192`. The existing `invoke` and `equivalent` functions own
process execution and observable checks. Every case expects status zero, empty
stderr, exact stdout, and no changes to its working directory. Expected helper
counts are workload contracts, not traced observations.

For these text workloads, useful output means the first complete expected stdout
line observed on the host pipe. It is not first-byte latency or the return of a
buffered `ScriptOutput`. Full stdout and stderr remain captured through process
completion. Wrong output, early failure, timeout, or unresolved cleanup receives
no timing credit. Hostpipe timestamps are populated only in measured mode;
fixture checks do not establish a timing baseline.

Common and demanding names do not assert a measured cost. The later accepted
baseline must establish which substantial cases exceed 50 ms of CPU and qualify
resource, allocation, syscall, and completed-work counts. Post-inference source
and products remain the final measurement baseline.

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
