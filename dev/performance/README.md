# Performance adapter reference

`check.py` dispatches to existing measurement and acceptance owners. It
requires Python 3.11 or newer and adds no production dependency. The full
`inference` acceptance gate remains unavailable until capability, joint-corpus,
and classification evidence are integrated. The separate P4
`inference_performance` measurement reports section 12 only; completing it does
not close full inference acceptance. `optimize`, `deployment`, and `close`
require their existing owners and fail before work when those owners are
unavailable. `xsht runtime-stats` supplies phase diagnostics.

Run policy controls without Docker:

```sh
python3 -B dev/performance/check.py --config CONFIG self-test
```

Add `--adapter` to qualify existing release products and execute one exact-output
control in the pinned offline Linux image. This also checks rejection of wrong
output and a real malformed cleanup cidfile. It does not measure performance.
Readiness controls exercise before and after failures without a real clock.

## Configuration

[`parse_config`](check.py#L77) owns the schema and exact field sets;
[`source_identity`](check.py#L105) validates source and path bindings. The
configuration binds full lowercase Git IDs, absolute nonsymlink paths, exact
XSH/XSHT products, an explicit Laputa checkout, and scratch under `.work/tmp`.
Functional, baseline, and measurement objects bind their existing evidence
paths. Functional output must live under the source checkout's `.work/`, and
must be a fresh path. Functional checks require `product_receipt` to be the
actual canonical receipt.

Product qualification checks the clean commit and tree, executable paths and
hashes, successful owner result, and resolved cleanup. The profile provider
requires opt3. The canonical provider verifies bootstrap, resource and log
bindings before qualifying verification products. It issues no new product receipt. An exact-source checkout can differ from the receipt's original
build location. There is no normal-checkout fallback.

Functional validation does not require a prior release profile or
functional-close record.

The optional baseline object requires exactly `functional_close`, `profile`,
`observer`, `forced`, `comparative`, and `scheduler`, each an absolute receipt
path. Omitting the object preserves existing schema-1 configurations, but cannot
pass baseline acceptance. `functional_close` must equal `functional.output`.

The measurement fixtures directory contains six roots: `2048`, `4096`,
`8192`, `16384`, `32768`, and `65536`. The output must be a fresh JSON file
below `scratch`. Measurement regenerates each reference through the existing
fixture maker and compares all bytes, modes, entries, links, and goldens before
launching work. The default 2048 manifest retains its existing pinned
validation.

## Functional validation

```sh
python3 -B dev/performance/check.py --config CONFIG functional --check
```

The command validates the final canonical replay receipt, pinned image and
command, resource record, bootstrap build, products, logs, source checks, and
cleanup. It checks required product and harness builds, warning-deny steps,
all five Linux contexts, and product stability. The structural scan runs
`metrics.xsh check-current --review-evidence` in the pinned image. It validates
the report and functional review rows; neither a boolean nor launcher success
can close acceptance. Timeout or unresolved container cleanup fails the run.

[`check_skip_review_policy`](check.py#L567) binds the review to the configured
source, the exact canonical and supplemental receipt hashes, and each required
context. Every context needs `result: "review_verified"` and a nonempty reason
for its skip disposition. Review every mandatory skip in the bound logs; an
unresolved mandatory skip prevents a verified context. The review records a
human judgment; it does not establish that a privilege-dependent case ran.

After all checks pass, the command publishes the existing functional-close shape
with the exact commit, tree, reviewer, products, scope, and evidence bindings.
Publication refuses overwrites and exposes only a complete JSON record. It
rechecks source and products before publication. No full successful close has
been established by the adapter foundation tests.

## Canonical bootstrap ownership

`final_canonical_replay.py` owns the bootstrap command and receipt schema. The
receipt binds the clean source, pinned image, Rust flags, Cargo output, and
executable bytes. The producer retains those bytes because later Cargo builds
can replace target artifacts. A supplemental product receipt does not satisfy
the canonical bootstrap contract.

## Inherited baseline close

```sh
python3 -B dev/performance/check.py --config CONFIG baseline --check
```

This command validates completed receipts for canonical functional close,
release profile, timed observers, forced frames, comparative parity, and
scheduler pairs. Every receipt must match the clean source and qualified
products. Missing receipts, discarded timings, changed products, or unresolved
cleanup fail. It validates existing evidence; it does not run replacement
suites or publish a new baseline.

Profile, forced-frame, and supplemental receipts bind the explicit clean
`laputa_root`. Forced containers mount it read-only and set
`XSH_LAPUTA_CORPUS` to that path. Historical receipts without these bindings
cannot close baseline acceptance.

The scheduler bridge validates the existing reference build and captured Docker
command before publishing a receipt. Use the same run ID for bridge and
scheduler comparison outputs.

Forced frames require `--network none`, a loopback `xsh-test` hostname, and the
`mold` executable qualified by the release profile. Preserve failed output and
use a fresh attempt directory for each run.

Each supplemental build, warning-deny, and `linux-special-*` step uses one
read-only Laputa mount and one matching `XSH_LAPUTA_CORPUS` value. Receipt
validation checks product and harness paths, image, mount modes, and Docker
arguments. Missing, stale, dirty, duplicate, or mismatched bindings fail
functional close.

Util-linux close requires an in-memory independent review. The standalone CLI
fails before expensive work without it. A coordinator must call
`baseline(config, runtime)` inside `runtime.independent_review(actual_gate_result)`.
The result must come from actual review; the adapter cannot import or fabricate
one.

`baseline` then runs the existing util-linux close under native and Cargo
leases. Close checks private mount, IPC, and PID boundaries plus host/API,
repository, documentation, resource, review, and cleanup gates. Close owns
resource validation. The adapter checks source, receipts, and products again
afterward. No accepted current-source baseline exists yet. The
[util-linux adapter](../util-linux/README.md) owns corpus and review requirements.

## Representative measurement

Measurement requires a same-source util-linux `CloseReport`. A coordinator
runs close and measurement in one process:

```python
runtime = check.baseline_util_linux(config)
with runtime.independent_review(actual_gate_result):
    closed = check.baseline(config, runtime)
    report = check.measure(config, closed)
```

`measure --check` without this live prerequisite fails before timing. The
standalone CLI cannot deserialize or synthesize acceptance, so it cannot repeat
the private suite. Measurement uses the completed release `opt3` receipt and
builds no products. Canonical verification products continue to qualify
functional close.

The sampler runs every selected case after parity and warmup. It uses three rounds
of 60 fresh startup processes or 30 processes for other cases. `perf-gate-ready`
and `perf-gate-ready -1` bracket the timed Docker batch; a busy host discards
the whole batch. Workload deadlines, exact output, effects, and process cleanup
must pass before a sample receives credit. First useful output is the first
complete expected line on the host pipe, not first-byte latency.

Samples include CPU, RSS, resource scope, and exact output checks. Linux CPU
and RSS come from the waited process leader; they do not represent an entire
process tree or cgroup. Container cgroup peaks and binary sizes are reported
separately. Missing accounting fails measurement. `runtime-stats` and
`xsht trace --syscalls` provide diagnostic attribution, not accepted samples.
Reports provide evidence but do not by themselves assert a performance
improvement or close acceptance.

## Limits

Policy tests cover config rejection, product identity, output comparison, failed
builds, cleanup, readiness ordering, canonical rejection, and required supplemental
contexts. They do not replace full acceptance, resource qualification, workload
measurement, phase-observer validation, or final timing gates.

Unresolved owned-container cleanup retains its cidfile under `scratch` and fails.
The adapter uses the existing owned-container cleanup implementation. It does not
create a new container supervisor or benchmark runner.

## Runtime phase diagnostic

`xsht runtime-stats --json REPORT SCRIPT` reports allocation, status, output
size, and phase durations. Wall time uses a monotonic clock. CPU time counts
threads in the XSH process and excludes descendants. Both clocks include
observer overhead, so use the fields for attribution only. Ordinary execution
does not collect phase data. A `null` phase was not reached; zero is an observed
zero-length phase. The report schema and callable boundaries live in
`crates/xsht/src/stats.rs` and `src/runtime_stats.rs`.

## Inference measurements

`inference_workloads.py` generates annotated and reduced source maps for the
nine inference families at 64, 128, 256, and 512 units. The original eight
families and 36 cases are unchanged. Named generic constructors add four shapes:
independent binders, shared defaults, ground return carriers and explicitly
anchored phantom contexts, for 52 cases in total. Deep-record
constructors and projections keep each syntax expression shallow while the
named record types retain the requested semantic depth. The 64, 128, and 256
depth outcomes remain pending probes. The adapter records them as ordinary
checks. The 512 deep-record input must reach the semantic type-depth rejection
with documented check exit status 2 for both source variants and both
compilers. It has no runtime distribution.

The `inference_performance` config binds a fresh report path and a frozen
reference source and profile receipt:

```json
{
  "inference_performance": {
    "output": "/checkout/.work/tmp/inference.json",
    "reference": {
      "source_root": "/checkout/.work/wt/frozen-reference",
      "commit": "<full lowercase commit ID>",
      "tree": "<full lowercase tree ID>",
      "profile_receipt": "/checkout/.work/evidence/reference-profile.json"
    }
  }
}
```

The profile reader selects release opt3 `xsh` and `xsht` products for both
source revisions. It checks source, target, pinned image, LTO, codegen units,
binary hashes, and the exact command generated by the existing profile-cost
owner. The report includes the selected profile flags and command. The adapter
does not select a reference from a scheduler receipt or a path in `PATH`.

Run the section 12 performance measurement with:

```sh
python3 -B -m dev.performance.inference_measurement --run-performance --config CONFIG
```

This entry point calls the single `check.inference_performance` owner. The
existing `check.py inference --check` command remains fail-closed until the full
capability, corpus, and classification gate is available. A report's
`performance_complete` and `qualification_complete` fields apply only to the
section 12 workload and performance budgets; they do not represent full
inference acceptance or the separate build-cost and campaign gates.

The adapter checks each generated entry with the candidate `xsht` and, when
the reference accepts the annotated input, with both compilers. It records the
reference's status and diagnostic when a feature is unsupported. The candidate
still runs its annotated and reduced cases. Unsupported reference cases keep
the report incomplete for qualification.

Each candidate check distribution alternates 100 annotated/reduced pairs. Each
reference/candidate annotated check comparison also uses 100 pairs. Runtime
distributions alternate five pairs, including five reference/candidate pairs
for each supported annotated workload. The adapter also compares the existing
`bench/stdlib-port/cold_trivial.xsh` runtime input. It requires identical source
bytes in both source trees and checks its exact `hello\n` output.

Every arm launches a fresh process. `perf-gate-ready` and its `-1` form bracket
each distribution. If the post-check rejects the host or source/product
identity changes, the window is marked discarded and its samples do not
qualify. The adapter holds the existing exclusive performance,
native-suite, Cargo, and parity leases in that order. It verifies both clean
source trees, both receipts, all product hashes, and every generated file
before and after each distribution. The filesystem page cache stays warm; the
adapter does not flush it. Samples include CPU time, wall time, peak process
RSS, status, exact output hashes, effects, and first useful output time.
Summaries use nearest-rank p95 and retain raw samples, ranges, and paired
spread. Check samples recognize only the canonical closing stage-timing line;
actual diagnostics, malformed or extra lines still fail. Raw stderr bytes and
hashes remain recorded. Runtime samples still require empty stderr.

The adapter passes each generated `main.xsh` entry to the canonical
`xsht frontend-stats --json` owner, which resolves its imported sources through
the ordinary loader. Separate untimed `xsht runtime-stats --json` runs record
wall and process CPU phase durations, including inclusive total preparation,
for candidate annotated/reduced inputs and supported reference inputs. These
instrumented diagnostics are reported separately from runtime samples and
never enter the timing budgets. Candidate frontend stats must report these nonnegative raw
`inference_work` fields on that entry: `fresh_variables`,
`generated_constraints`, `representative_hops`, `compression_writes`,
`unifications`, `occurs_visits`, `row_field_visits`, `scheme_instantiations`,
`successful_generic_call_resolutions`, `worklist_wakeups`,
`host_candidate_trials`, `host_obligation_requeues`, `rollbacks`,
`rollback_changes`, `level_adjustments`, `effect_edge_visits`,
`candidate_tests`, `diagnostic_work`, `output_type_nodes`, `evidence_nodes`,
`generalization_type_visits`, `instantiation_type_visits`, `schema_argument_visits`,
`constructor_generalizations`, `constructor_template_binding_visits`, and
`generalization_binding_clones`.
The entry record must also report `function_body_check_entries` and
`semantic_retained_bytes`. The adapter reads allocation traffic from the
existing `tokens`, `cst`, `ast_check`, `lower`, and `after_drop` stages. It
requires allocation tracking to be active for candidate samples.

Candidate `frontend-stats` supplies invocation-local counters. Missing candidate
fields are unavailable and fail before timing; they never default to zero.
Older reference products may lack these counters. The reference reader uses
retained typing and allocation data without inventing solver counts. The adapter
does not derive solver work from source length or output-shape metadata. Output and evidence descriptions in the workload generator remain
structural metadata.

P4 qualification compares candidate annotated checks with the frozen
reference. Median CPU and wall time may increase by at most 5 percent or 1 ms.
The p95 limits are 10 percent or 2 ms. Median peak process RSS and retained
typing bytes each have a separate 10 percent or 2 MiB limit. Candidate reduced
check medians may increase by at most 20 percent or 2 ms over candidate
annotated medians. Reference/candidate runtime medians, including the
monomorphic control, may increase by at most 5 percent or 1 ms. The adapter
reports binary sizes separately. A result passes only when the full observed
uncertainty interval stays within its allowance; it fails only when that
interval is wholly over budget. Overlap is inconclusive and does not pass. A
separate independent window is needed to resolve an inconclusive result.

For each workload doubling, the adapter checks each raw solver counter and
`function_body_check_entries` after normalization by the independently
reported `output_type_nodes + evidence_nodes`. Each normalized value must stay
within `3 * previous + 128`. The report identifies its
`inference-performance` scope and distinguishes `samples_complete`,
`timing_complete`, `counters_complete`, `qualification_complete`, and
`performance_complete`. Samples can finish while qualification remains
incomplete because of unsupported reference inputs, missing counters,
inconclusive budgets, or a failed scaling law.

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
