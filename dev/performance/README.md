# Performance adapter reference

`check.py` dispatches to existing measurement and acceptance owners. Python 3.11
or newer is required. It adds no production dependency. The planned `inference`, `measure`, `optimize`, `deployment`, and
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

## Functional validation

```sh
python3 -B dev/performance/check.py --config CONFIG functional --check
```

The command validates the actual `final_canonical_replay.py` receipt, its pinned
command and image, resource record, bootstrap build, product files, logs, source
checks, and cleanup. It validates required product builds, harness builds, warning-deny commands,
all five current Linux contexts, product stability, harness stability, and log bindings.

The command runs `metrics.xsh check-current --review-evidence` in the pinned image.
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
substitute for the canonical bootstrap receipt.

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
