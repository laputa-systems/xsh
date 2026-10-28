# Testing

Use `cargo dev gate PATH...` for the narrowest check owned by changed paths.
Run `cargo dev gate --full` for the full local integration gate and
`cargo dev test linux` for Linux verification in `Dockerfile.test`. Gate
selection lives in `dev/gates.xsh`; profile and platform rules live in
`dev/test_workflows.xsh` and `dev/targets.xsh`.

## Native tests first

Use native XSH tests for language and command behavior. Use Rust tests for host
boundaries the native harness cannot express, such as PTYs, privileges, and
small-stack behavior. A Rust test that needs XSH behavior should invoke a
disk-backed native test unless the Rust API must inspect or construct source.

| Suite | Location |
|---|---|
| Native XSH | `test_roots` in `xsht-config.ini`: `tests/xsh`, `core/tests`, `dev/tests`, and `showcase/tests` |
| Root Rust tests | `tests/`, including the `integration` aggregate and separate host-boundary targets |
| Tooling and syntax | `crates/xsht/tests/` |
| Interactive | `crates/xshi/src/interactive/` and `crates/xshi/tests/` |
| Fixtures | `tests/fixtures/` |

## Finding tests

`xsht test --list FILTER` lists tests matching a path prefix or test-name
substring. Use `--exact PATH::TEST_NAME` to select one native test. Listing or
running an unmatched exact selector exits 2. Repeat `--exact-id ID` to freeze a
set across modules and add `--events-jsonl PATH` for the schema-1 result stream;
both are specified in [XSHT](XSHT.md#native-tests). The `xsht test` options are
in `xsht help test`.

Use `xsht grep` or text search to find native coverage for a symbol. Rust test
names are Cargo filter strings. For example:

```sh
cargo test --profile verification --target "$TARGET" --test integration \
  runtime::modules::native_xsh_core_dns_clients -- --exact --nocapture
```

On Linux, `dev/cargo_test_runner.sh` uses libtest `--list` to check a focused
test filter. It exits 125 if no test matches or the runner cannot verify the
selection. Unfiltered runs, metadata requests, and benchmarks pass directly to
libtest.

## Gates

`cargo dev gate PATH...` selects Change verification from the component catalogue.
Use `--tier edit` for an Edit request, and repeat `--case ID` for explicit cases.
Use `--plan` to inspect cases, ownership reasons and required products without
building products or starting Docker. Add `--json` for schema 1 inspection output.
`selection_complete` describes ownership and closure resolution, not execution.
Saved plans carry no execution authority and cannot be passed back for execution.
Unknown paths produce incomplete scope and exit 2. They never select a broad fallback.

A focused case that passes stores a record under the checkout's `.work/gate/`,
bound to the source digest and the product digests and modes the run used. The
record is disposable local evidence and is never committed. A case starts by
discarding its previous record, so a failed or interrupted run leaves it not run.
`cargo dev gate [PATH...] [--case ID] --status` lists the selected cases and
reports each as current, stale or not run. Stale names the first mismatch: a
changed source, a changed or missing product, or an unreadable record. Status
only observes source and products; it starts no Docker, Cargo or tests.
Scenario and Laputa cases store their record when the external run passes. A Laputa record also
binds the Laputa root and source digest and, for reuse-count, `xshi`; `--status` with `--laputa-root`
observes the checkout again and reports a changed root or digest as stale, and without the root it
reports the record as stale. A `--full` run stores a
source-bound record only after every step passes; until then status reports
`Full: not run`, and a source edit afterwards reports it stale.

Only the sole `cargo dev gate --full` request enters comprehensive verification.
The former `--batch` flag and `internal gate-batch` operation are removed.
The canonical replay caller uses `internal gate-validate-full` before any Docker
operation. This internal operation accepts no arguments and validates the live
Full catalogue, dependencies, aliases and product closure without preparing or
running them. It grants no Full execution authority. Replay rechecks source and
bootstrap identity afterward; `internal gate-full` repeats closure validation
inside the container before product preparation. Replay receipt names and their
schema remain unchanged.
Aliases and prerequisites are resolved before commands start. A focused closure
containing a manual case, a missing ID or a cycle fails with exit 2.
Native-file IDs emitted by path plans replay with `--case ID`. Planning rejects
duplicate IDs, missing files and native files that resolve outside the owned root.
The catalogue uses `RuntimeSeconds` for bounded cases and `ManualOwner` for
manual cases that retain the existing owner lifecycle.
Scenario cases require explicit `--case ID` selection. Path ownership cannot
select a Scenario through a prerequisite.
The explicit `laputa/pm.make/plan` scenario requires `--laputa-root` with an
absolute canonical Git root. It selects two exact PM planning tests in a
network-disabled Linux container limited to two CPUs, 4 GiB and 300 seconds.
The container mounts Laputa, its linked-worktree Git metadata, and prepared XSH
and XSHT products read-only, with fresh owned scratch as its only writable bind
mount. The host observes both repositories and product bytes and modes before
and after execution, and reconciles XSHT events against the selected IDs. These
observations are not atomic snapshots. The container uses the shared native
lease and retains its cleanup quarantine until container removal is verified.
The `laputa/kbuild/discovery` scenario uses the same boundary for one synthetic
Kbuild discovery test. Select only one Laputa scenario per request.
The `laputa/kbuild/graph-cache` scenario checks synthetic graph equivalence,
cache invalidation, EFI stub task graphs and relocation rejection, plan output,
worker configuration binding, and archive planning snapshots.
The `laputa/kbuild/make-semantics` scenario checks Make assignment, nested
function expansion, and compile-flag regressions.
The `laputa/pm.execute/reuse-count` scenario checks builds and reuse in an
isolated temporary store. It also prepares and binds XSHI for the executor.
The `laputa/pm.root/compose` scenario checks isolated root inventories, file
modes, links, canonical ownership, and collision rejection.
The `laputa/pm.plan/inventory` scenario checks synthetic package graph order,
reuse, dependency kinds, and executor provenance.
The `laputa/world/host-plan` scenario checks isolated host-plan selection and
mirror environment handling; it does not build a world.

The explicit `performance/json-n/correctness` scenario runs alone. It generates
its fixture in owned scratch and checks the output and side effects in a fresh,
network-disabled Linux container. It binds the selected source and prepared XSH
product before and after execution. Its resource output is diagnostic only:
unavailable counters are reported as such, and the case does not measure or
qualify performance.

The initial focused cases include `showcase/tokei/correctness` and
`core/lsmem/correctness`. Each selects its existing native behavior module.
At Edit tier, `src/sema/check/local_inference.rs` selects the exact
`compiler/local-inference/correctness` tests; Change also retains its broader
checker coverage. `src/sema/schemes.rs` similarly selects the exact
`compiler/imported-inference/correctness` tests at Edit tier. Edits to
`src/sema/inference_work.rs` select the exact
`compiler/inference-work/correctness` Rust test.
A Change request for `core/lsmem.xsh` also selects `core/lsmem/consumers`, an
`--exact-id` set of the tests in other modules that run the applet. The
unprivileged gate lane skips the two namespace-dependent lsmem consumers, which need a
private namespace context only a Rust test provides; an exact set cannot include a test
that will skip.
An Edit request for `core/cat.xsh` selects bounded byte-stream tests. Change
selects the cat behavior module and its exact external consumers.
Edit rejects Change-only cases before preparing products. Edit runtime is limited
to 30 seconds and Change runtime to 300 seconds across selected automatic cases.
Each automatic command also has its declared case limit. Explicit Scenario cases use their
declared per-case limit and do not increase any automatic case's budget.

Corpus-wide checks are Scenario cases, not part of the component Change contract.
`native-core/tests` runs the whole core suite, about 9,800 native tests, within 3600
seconds. `test-desugar-corpus` checks and runs every sugared native test file twice, once as
written and once desugared, within 1800 seconds. `test-lint-format-corpus` formats and lints
the repository corpus within 1800 seconds. Change selects `test-desugar` and
`test-tooling` without them, and a Change request for a shared core path reports that no
Change case owns it and lists `native-core/tests` as an available scenario. A Change request
for `core/NAME.xsh` selects the `core/tests/test-NAME.xsh` module when it exists.
The runner admits at most eight tests of one file at a time, so a module that needs more
than 2,400 CPU-seconds cannot finish in 300 seconds. `core/tests/test-awk.xsh` (3,300
CPU-seconds, 429 s wall on a host at load 14 to 27) and the single sort-buffer test in
`core/tests/test-gnu-sort.xsh` (366 s) are therefore 1800 s Scenario cases; Change for
`core/awk.xsh` selects its Edit case and lists the module as an available scenario.

Native cases run as the host user inside the gate container, with a private home and
temporary directory under `.work/tmp`, as the Full native run does. The container itself
keeps Docker's default seccomp profile and capabilities. Required skips still make a case
incomplete, so a native test the unprivileged lane cannot run is not selected there.
`dev/gate_lane.xsh` declares those tests by exact ID. Two groups can run in a container
and have an explicit Scenario case each; each must be selected alone.
`lane/root-native` runs the `root_tests` group as root in the default container, with
Docker's default confinement. `lane/namespace-native` runs the `namespace_tests` group as the
host user in a container whose only change is the seccomp profile
`dev/gate-namespaces.seccomp.json`. That file is Docker's default profile (moby
`profiles/seccomp/default.json`) plus one rule that allows `clone`, `clone3`, `unshare`,
`setns`, `mount`, `umount`, `umount2`, `pivot_root`, `sethostname`, `setdomainname` and the new
mount API (`fsopen`, `fsconfig`, `fsmount`, `fspick`, `open_tree`, `move_mount`, `mount_setattr`)
without `CAP_SYS_ADMIN`; the kernel still checks capabilities in the target user namespace. The
container gets no added capability and no device, and `/proc` stays masked and its system
paths stay read-only. Three tests need an unmasked `/proc` (a `procfs` mount inside the new
namespace, or credential transitions that depend on one) and one needs `CAP_SYS_ADMIN` in the
initial user namespace, so they are declared outside every
lane. A native file whose every test is declared outside the unprivileged lane is a reported gap
that names these scenarios, not a case. A file with only some tests outside the lane stays a
case, but planning reports the counts as unresolved scope, so the request exits 2, and lists the
scenarios that cover the root and namespace tests. A case that ran a reduced selection
records evidence that `--status` shows as reduced, never as current. The remaining groups name a platform, an external
fixture that only a Rust test provides, supplementary groups, a host resource, an unmasked
`/proc`, or a test whose subject is the skip outcome; no gate lane runs them. A new native test
that skips in the gate lane needs an entry in one of those groups, and
`dev/tests/test-gates.xsh` checks that every entry names a test that exists.

Compilation runs outside the runtime budget and is reported as a separate stage.
Before Linux gate execution, Docker resolves the configured test image to an
immutable image ID. An unavailable image or a digest that resolves to a
different ID leaves the scope incomplete; the container uses the resolved ID.
Cargo prepares each needed test harness once with `--no-run`. Selected tests then
run through that executable directly, without another Cargo invocation.
Deadline expiry consumes owned process cleanup and exits 2. Required child failures
exit 1 while retaining the raw status. Gate cancellation consumes cleanup and exits
130. No timeout or failure can request Full verification.

For native filters, the gate lists matching test IDs as JSON and runs that exact
set from a bounded JSON file, preserving newlines and avoiding argument limits.
Discovery counts against the case budget. An empty selection, a required skip,
or an invalid selection exits 2; a failing test exits 1.
An execution plan with no cases also exits 2, including a change whose only
available check is a manually requested scenario. Plan inspection remains available.

After focused product preparation, the gate records digests and modes of selected
XSH, XSHT, and XSHI products. It checks them before and after successful leaf
commands and between native discovery and execution. A changed or unavailable
product makes the scope incomplete. Prepared Cargo test harnesses are also
inspected before and after execution and when reused. Execution inspections run
in owned children within the case and shared runtime limits; the host selection
inspection has a separate tier limit. The focused gate also compares staged Git
entries, persistent index flags, and tracked and untracked source contents,
including checked-out Git submodules, from host selection through preparation
and successful leaf execution. Git's cached index stat data does not affect this
comparison. Source inspection disregards the
configured Git excludes file so host and container see the same inputs;
repository ignore files and Git's per-repository exclude file still apply. A
changed source returns incomplete. A symlink to an existing regular file outside
the observed source set, or a directory containing an unobserved entry, also
returns incomplete. Links to special files are unsupported. The observation
includes each resolved symlink target, plus linked directory names and modes.
These checks are observations, not an atomic
snapshot: edits reverted between observations,
ignored and external inputs, harness preparation, and Full are not yet bound.
Native cases reconcile XSHT events against the selected IDs and child status.
Result and Cargo discovery event files must be regular, unlinked files no larger
than 16 MiB. Cargo diagnostic input must also be a regular, unlinked file; its
output reads at most 2 MiB. Invalid event files leave a successful child incomplete.
Missing or contradictory results make a successful child incomplete; a child
failure retains its failed status even if reconciliation is unavailable. A
failed case also retains failure precedence when the selected-ID file, cleanup,
or event writing fails;
the raw status and available cleanup diagnostics remain visible.
Prepared Cargo test harnesses list their selected tests and reconcile libtest
events. Required ignored tests and missing or contradictory results are incomplete.
Changes to the result reconciler, source observer, focused adapters, resource
observer or native leases select their Python contract tests directly.
Changes to the comparative runner select its harness and core workload tests.
At Edit tier, changes to `uniq`, `sort`, `find`, `xargs`, or `awk` select
focused record, ordering, traversal, process, or program behavior tests.
Their Change tier retains the broader core test selection.
Changes to the JSON runtime facade select structured-data behavior tests at
Edit tier and the existing API and standard-library checks at Change tier.
Changes to effect-inference checking select capability-contract tests at Edit
tier and retain the checker gate at Change tier.
Frontend statistics changes select a retained-column test at Edit tier and
retain the frontend memory checks at Change tier.
The explicit `performance/frontend-edit/correctness` scenario checks annotated
and reduced forms of two generated workloads, including runtime output, frontend
evidence, and phase-report consistency. It does not qualify disposal execution,
scaling, or performance.
The explicit `compiler/frontend-disposal/schemes` and
`compiler/frontend-disposal/compact-entry` scenarios run exact Rust controls for
execution after frontend state is dropped.
Constraint-solver production changes select a reference-oracle test at Edit
tier and retain the checker gate at Change tier.
Changes to inference workload generators and measurement tooling select their
Python contract tests.
Frozen-witness tooling changes select its Python contract tests. Other inference
corpus paths report unresolved until their external inputs and verification
owners are declared.
The explicit `performance/core-wc-{small,medium,large}/correctness` scenarios
check binary inputs of 64 KiB, 1 MiB, and 16 MiB against exact line-count
outputs. They do not measure performance.
The explicit `performance/core-{sort,traversal,awk}-{small,medium,large}/correctness`
scenarios check ordering, filesystem entries, and record aggregation against
exact outputs. They do not measure performance.
The explicit `performance/core-json-{small,medium,large}/correctness` scenarios
decode typed records and check their order, values, labels, and aggregate.
The small trace scenario also reconciles syscall and helper counts.
The explicit `performance/core-{wc,sort,awk,traversal}-small/trace` scenarios
also reconcile syscall summaries and completed helpers through XSHT tracing.
Their syscall counts include the traced XSHT process; they are not production
timing results.
The `core-wc-medium` and `core-traversal-{medium,large}` trace scenarios
use the same oracles at larger input sizes.
The `performance/core-spawn-{small,medium,large}/trace` scenarios check two,
eight, and 32 successful spawn-and-wait lifecycles and deferred no-op cancellation.
The `performance/core-pipeline-{small,medium,large}/correctness` scenarios
check two, eight, and 32 pipeline outputs and their completed helpers through
XSHT's syscall trace. An observed child failure retains status 1 if process
cleanup is unresolved. A completed child with a nonzero exit is a failure; an
unfinished timeout is incomplete and controller cancellation returns 130.
They do not measure performance.
Focused execution requires Python 3.11 or newer for source inspection and result
reconciliation.

Workload result reconciliation, atomic source binding, per-case product profiles
and explicit workload inputs remain open campaign work. Full derives its dependency
and product closure from the retained batch owners. It validates the complete
closure before Docker, product preparation or child execution. Dispatch controls
use bounded substitute commands and do not perform actual Full verification.
Automatic Git path collection preserves newline characters and includes staged,
unstaged, and untracked paths independently. It ignores inherited `GIT_*`
overrides and replacement refs, and rejects indexes with `assume-unchanged` or
`skip-worktree` entries, which can hide edits. Explicit cases remain available.
A discovered non-UTF-8
path makes discovery incomplete with status 2. These
checks do not qualify P0 or campaign acceptance.

Process tests and native suites use the optimized `verification` profile by
default. It disables LTO and uses 64 codegen units. Release tests are for final
verification and performance measurements; `dist` is for packaging. Unit tests
that spawn no binaries may use debug builds. Build only the packages needed.

Rust files such as `tests/grammar.rs` are modules of the `integration` target,
not standalone Cargo test targets. For a pure grammar check, select
`-p xsh --test integration grammar::bare_minus_command_arguments_preserve_source_adjacency -- --exact`
with the default debug profile. XSHT uses the same aggregate target under
`-p xsht`. Confirm target and module names before composing a filtered command.
After editing grammar productions, run the grammar module tests, including both
generator depths, before starting full qualification. Fixed seeds select from the
current production graph, so a local grammar edit can expose another parser case.

Cargo process tests resolve `xsh`, `xsht`, `xshi`, `xsh-fuzz`, and
`xsh-test-helper` through `test_bin!`, which rejects unsupported profiles.
Build products from the same target and profile before running filtered tests.
The canonical gate builds the required products for you.

The ordinary XSHT integration target includes the example lint corpus once.
The corpus test removes the lint timing summary before comparing diagnostics.

On a musl build host, `cargo dev` may preload jemalloc into compiler-only
steps. A `cargo test` step builds test executables with the preload, then runs
them without it so child processes do not inherit the library. Manual builds
do not use this wrapper. See `dev/cargo_steps.xsh` for the policy.

Changed fixtures select their compiler owner rather than running as native
test modules. `cargo dev gate PATH...` applies that mapping. The Full gate runs
one full native suite at a time and includes repository and documentation
checks.

Changes to the current metrics scanner, its attestation, or either current metric
test module select both current metric modules. Canonical bootstrap and execution
reject stale scanner or ratchet hashes before starting a build. Reproduce the
frozen baseline report and item inventory before updating an active observer pin.
The current scanner's `SourceScan.modules` records both `directory` and
`path_directory` so external `#[path]` owners resolve children using Rust's
physical-directory rules. Missing, ambiguous, and conflicting owner-sensitive
module targets fail the scan.
The existing native checks still validate the historical proof.

## Building and release smoke

Build products with `cargo dev build`. Linux tests use the pinned
`Dockerfile.test` image and target settings; a host build is not Linux
verification. `dev/targets.xsh` defines both x86-64 and AArch64 Linux product
targets. The util-linux acceptance adapter currently qualifies its campaign in
the pinned x86-64 image; that scope does not narrow the product target matrix.

`cargo dev dist` creates packaging products under `target/$TARGET/dist`.
`tools/copied-product-smoke.py` checks copied distribution binaries against a
core archive. Use its `--help` for archive and output paths. Keep test runs in
`verification` or `release`, not the `dist` profile.

## Remove a Docker-backed worktree

Before removing a Docker-backed worktree, check every directory as the user
who will remove it. Git needs write and search permission on each directory.
A failed removal can leave the worktree unregistered, so run this check first.

```sh
repo=/absolute/path/to/main/checkout
worktree=/absolute/path/to/linked/worktree
if python3 - "$worktree" <<'PY'
import os
import sys

unwritable = []

def record_error(error):
    unwritable.append(error.filename or sys.argv[1])

for directory, _, _ in os.walk(sys.argv[1], followlinks=False, onerror=record_error):
    if not os.access(directory, os.W_OK | os.X_OK):
        unwritable.append(directory)

if unwritable:
    print("\n".join(unwritable), file=sys.stderr)
    raise SystemExit(1)
PY
then
  git -C "$repo" worktree remove "$worktree"
else
  exit 1
fi
```

Run the check as the user who will remove the worktree. If it prints a path,
repair its ownership or permissions and rerun the check before removal.

## Grammar proofs

`tests/grammar.rs` checks that the parser accepts the language generated by
`src/syntax/grammar.rs` and that repository programs accepted by the parser are
also in that grammar. The fixed-seed soundness target checks generated programs
against the reference evaluator:

```sh
cargo test --profile verification --target "$TARGET" -p xsh-fuzz \
  --test soundness generated_programs_are_grammar_sentences
```

Use `cargo dev gate src/syntax/grammar.rs` for the current grammar owner. Run
`make docs-check` when grammar output or its generated reference changes.

## Soundness fuzzing

`cargo test --profile verification --target "$TARGET" -p xsh-fuzz --test
soundness` runs the fixed seed set. `make fuzz` explores fresh seeds and saves
reproducers under `target/fuzz/`. A failure prints a minimized program.

The fixed-seed run is the Scenario case `test-soundness`. No path selects it
automatically. `cargo dev gate --plan PATH...` lists it as an available scenario
for checker, lowering, runtime and syntax changes, and
`cargo dev gate --case test-soundness` runs it.

## Linux

Run Linux work in the pinned `xsh-test` image with the target flags in
`dev/targets.xsh`. `cargo dev test linux` runs the Linux suite;
`cargo dev test linux --ci` selects the CI route. Rust host-boundary tests
may require root or Linux capabilities. A missing required capability fails
the focused run. The privileged core tests use a `nodev` scratch filesystem for
block-device safety.

The `cargo dev test linux` containers use the Linux musl target for the
selected architecture: x86-64 selects `x86_64-unknown-linux-musl`, and AArch64
selects `aarch64-unknown-linux-musl`, including on macOS. Their Cargo target
root is `target/linux-test` by default. When `CARGO_TARGET_DIR` is set, they
use its `linux-test` subdirectory, separate from host build products. The
`xsh-cargo-registry` volume remains shared.

`date --set` and the ignored BTRFS reflink test change host-like kernel state.
Run them only in a disposable QEMU TCG guest with no network or host devices.
Use an emulated 2020 CMOS clock and pass a fresh 256-bit nonce in both the boot
arguments and environment. The tests also check QEMU identity and guest root.
Stage GNU parity tools inside that guest.

The traceroute source-route cases require `CAP_NET_RAW` and loopback-only
networking. The two infinite `/dev/zero` split cases run in a separate
container with a 0.01 CPU quota and a 1 GiB memory limit. Their driver waits for
the child to open the device, then kills and reaps it. The ordinary gate runs
this bounded case separately.

## Syslog receiver

`core/tests/test-syslog-*.xsh` cover `syslogd` and its libraries. The parser,
renderer and limiter files are pure native tests. `test-syslog-service.xsh`
starts the real service the way a supervisor does (a readiness descriptor, one
append-only log for stdout and stderr, signals to stop it) with the socket under
a test directory, so it needs no namespace. Its senders are XSH and
`core/tests/support/syslog_sender.py` (`python3`), which passes descriptors and
floods. `test-syslog-libc.xsh` builds `core/tests/support/syslog_client.c` with
the image's `clang-23` (`CC` overrides) and runs it against the service inside
new user and mount namespaces whose `/dev` is a private tmpfs, because
`syslog(3)` hard-codes `/dev/log` and the host already has one. The test
creates no device nodes: it bind-mounts `null`, `zero`, `full`, `random`,
`urandom` and `tty` into the tmpfs first. A kernel that refuses unprivileged
user namespaces skips those cases with a named reason, as in the unprivileged
`gate` container; the privileged lane runs them all. The case that runs clients
as three uids (1001 to 1003) needs real root and skips otherwise. Run the full
set the privileged way, with `PRODUCTS` the directory of the built `xsht` and `xsh`:

```sh
docker run --rm --privileged -v "$PWD":/work -w /work xsh-test sh -c \
  'for f in core/tests/test-syslog-*.xsh; do PRODUCTS/xsht test "$f"; done'
```

Result on x86_64 (this work): kmsg 7, kmsg-device 1 (needs a readable
`/dev/kmsg`), libc 7, limit 14, parse 14, render 13, service 23, 79 passed,
0 skipped. On the unprivileged host (`unshare -Urm` works) the libc file passes 6
and skips the uid case, and the kmsg-device file skips.

Facts the tests established about the platform:

- musl `syslog(3)` opens a blocking `SOCK_DGRAM` socket (`strace` shows
  `socket(AF_UNIX, SOCK_DGRAM|SOCK_CLOEXEC)`), and glibc 2.41 does too. A
  stalled receiver therefore blocks every logging program instead of losing
  messages; `test_a_stalled_receiver_blocks_a_musl_client_which_then_loses_nothing`
  holds the service with `SIGSTOP` and shows the client blocked and then
  delivering all 150 messages. `syslogd` drains its socket before it ever waits
  on output for that reason, and drops a record when stdout is not writable
  instead of waiting.
- The queue of an `AF_UNIX` datagram socket holds `net.unix.max_dgram_qlen`
  datagrams, 10 on the test host, and `SO_RCVBUF` does not raise it. A
  nonblocking sender that outruns the receiver sees `EAGAIN` and the receiver
  sees nothing, so kernel loss is reported as `kernel_loss=unobservable`, never
  as a count.
- `SCM_RIGHTS` carries at most 253 descriptors per message. With 4096 bytes of
  control buffer all of them are received and closed; under `RLIMIT_NOFILE`
  exhaustion the kernel installs the ones that fit and sets `MSG_CTRUNC`, and
  the descriptor count of the service is identical before and after 10 000
  messages carrying descriptors.
- Output to a pipe: every line is under 4096 bytes (`PIPE_BUF`), the service
  writes and flushes one line at a time and checks `POLLOUT` before each, so a
  full pipe drops and counts records rather than blocking, and `SIGTERM` still
  stops the service.

### Kernel messages

`syslogd --kmsg /dev/kmsg` also reads kernel records. The reader opens the device
nonblocking and seeks to its end, so only messages logged after the start are
read (the ring buffer is not replayed). `EPIPE` means the reader fell behind the
ring: it is reported as `event=kmsg-overrun` and the next read continues at the
oldest record held, so the `kseq` numbers jump visibly. The record parser, the
overrun and empty-queue classification and the one-record-per-read drain are
native tests over a datagram socket, which also returns one whole record per
read. The tests never write to `/dev/kmsg`. One manual probe against the real
device, in a privileged container, appended two lines to the host ring buffer
(`printf '<5>syslogd kmsg probe one\n' > /dev/kmsg`, then a second) and showed
them as `source=kmsg` records with the kernel's own `kseq` and `ktime_us`,
alongside two unrelated kernel lines that arrived meanwhile.

### glibc client (one-off, not a gate case)

The image has no glibc userland, so a real glibc client runs in a disposable
container that shares only the socket. With the service running on the host or
in `xsh-test` at `$G/dev/log` (directory `0755`):

```sh
docker run --rm -v "$G/dev/log":/dev/log \
  -v "$PWD/core/tests/support/syslog_client.c":/client.c:ro debian:stable-slim sh -c \
  'apt-get update -qq && apt-get install -y -qq gcc libc6-dev &&
   gcc -O1 -o /tmp/c /client.c && /tmp/c basic && /tmp/c multiline &&
   /tmp/c count 50 && /tmp/c long'
```

Result with Debian glibc 2.41 on the x86_64 host: all 54 records arrived
(`facility=daemon severity=warning tag="cclient"`), the `long` message arrived
uncut by glibc and was cut by the service at 2048 bytes (`cut=3951`), and the
kernel pid in each record (6665) differed from the container's own pid in
`tag_pid` (513), which is the kernel credential crossing a pid namespace.

### Measurements

Verification build, one core of a loaded 32-core host (load 20 to 70), x86_64 musl:

| Measure | Result |
|---|---|
| Idle for 61 s | 0 voluntary context switches on either thread, 0 CPU ticks |
| Idle syscalls under `strace -f -c` | 3 `ppoll` calls in the whole run (startup) |
| `time.sleep(10s)` for comparison | 990 `clock_nanosleep` calls |
| Sustained intake | 3000 records accepted in about 3 s wall, 2.18 s CPU: about 0.73 ms CPU per record (parse, limit, render, write, flush) |
| Peak resident set | 38 MB (`VmHWM`) at idle and after the 3000 records |
| Open descriptors | 8 at idle and after the 3000 records; the flood test (4500 blocking sends and a 3000-datagram nonblocking burst) asserts fewer than 20 |
| Stop | `SIGTERM` during a flood: exit 0, socket removed, final counters printed within seconds |

Time-based figures are not gates and were taken on a busy host; the perf-gate
rule was not applied to them.

## Interactive parity

Interactive tests require `-p xshi --features laputa-asterinas`; builds
without the feature compile the disabled stub. Use `--lib` for units and
`--test cli --test stdlib_preparation` for CLI and repeated-input preparation.
Linux runs use the pinned image.

Files under `tests/fixtures/interactive-parity/<os>/` are historical PTY
transcripts from `ish`. Their replay driver is absent, so they do not establish
end-to-end terminal parity.

## What not to run

- Do not run formatters or autofixers in verification. Check-only forms are
  safe. The repository does not accept formatter-generated churn from tests.
- Avoid unfiltered `cargo test --profile verification --target "$TARGET"`.
  The root `runtime::coverage` test runs the full native suite.
- Do not run process tests in debug. They require verification or release
  products, and debug unit tests are the place for debug assertions.
- Do not run multiple full native suites at once. Bound ad-hoc probes and stop
  their child processes before the worktree is handed off.

## Flakes and skips

A flaky test is a bug. Fix its race or unstable assertion instead of retrying
it. Assert on rendered error text, not bare status numbers, because nested
process output can include PIDs and timestamps. Native skips must name the
missing platform, tool, privilege, or fixture condition. Rust tests that return
early for a missing capability count as passed, so use `--nocapture` to confirm
whether a privileged case ran.

The default native timeout is 120 seconds. Use `xsht test --timeout DURATION`
for a different limit; `0` or `none` disables it. Tests that need a longer
limit must request one with `test.timeout`.

## Coverage

`cargo dev coverage [--backend native|docker]` runs the combined Rust LLVM and
XSH API coverage workflow. Use the Docker backend outside the pinned test
image. `xsht test --cov` reports source coverage; add `--api` for API hits or
`--cov-json FILE` for JSON output. Coverage counts parsed executable
statements in every configured source file, including files no test loads.
`[coverage] exclude` removes files from that denominator.

## Benchmarking

Run benchmarks serially with paired settings. A single timing is inconclusive.
`cargo dev bench` runs the `xshi` benchmark suite; `--fast` reports allocation
counts, not latency. Use `cargo dev bench --syscalls` or `xsht trace --syscalls`
to inspect subprocess and kernel activity. The
[standard-library port](../bench/stdlib-port/README.md) and
[performance adapter](../dev/performance/README.md) own their measurement
procedures.

### Profile btop

`dev/perf/btop-profile.xsh` is a Linux release workload. Set `XSH_BIN` to the
absolute release binary built from the revision under test. The profile checks
host readiness before and after the run and requires `perf-gate-ready` to pass.

## Generated docs

`make docs` regenerates `docs/SPEC.md`, `docs/user-tour.md`, and
`docs/reference/*.md` from `docs/templates/` and the command, diagnostic, API,
and grammar sources. Edit the inputs, then run `make docs`. `make docs-check`
renders into memory and checks the XSH snippets and tour project. Do not edit
generated output directly.

`docs/user-tour.html` is rendered from `docs/user-tour.md` by
`dev/tour_html.xsh`; its renderer accepts only the Markdown constructs used by
the tour. Update the renderer in the same change when the tour needs a new
construct.
