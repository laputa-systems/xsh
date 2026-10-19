# Performance adapter reference

`check.py` dispatches to existing measurement and acceptance owners. It
requires Python 3.11 or newer and adds no production dependency. `inference`,
`optimize`, `deployment`, and `close` remain unavailable and exit nonzero.
`xsht runtime-stats` supplies phase diagnostics.

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

## Frozen workload selection

`inference_workloads.py` generates annotated and reduced source maps for the
eight fixed inference families at 64, 128, 256, and 512 units. It is fixture
preparation only; it does not compile, execute, measure, or qualify them.
Expected units, logical stdout, and runtime-output byte counts are declared
separately from source generation. Frontend behavior remains unprobed except
for the deep-record controls: 64, 128, and 256 await a boundary probe, while
512 remains an expected type-depth rejection under the existing limit. These
sources retain future RED inputs without claiming section 12 coverage.

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
