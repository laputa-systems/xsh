# Testing

Use `cargo dev gate PATH...` for the narrowest check owned by changed paths.
Run `cargo dev gate --full` for the full local integration gate and
`cargo dev test linux` for Linux verification in `Dockerfile.test`. Gate
selection lives in `dev/gates.xsh`; profile and platform rules live in
`dev/test_workflows.xsh` and `dev/targets.xsh`.

Cargo resolves crates.io packages, including h12tiny, through the mycrates archive
and the owned async runtime through the native mycrates registry, as configured
in `.cargo/config.toml` and the workspace patches.
The archive contains pinned versions, so publish newly locked dependencies to
the archive before relying on an updated `Cargo.lock`.

## Performance baseline

On 2026-11-16 the owner approved the recommendation to count the passing
canonical replay as the correctness gate and replace the timed consolidation
stages with ordinary readiness-checked interleaved measurements. XSH-19 then
retired the pinned consolidation tools. No everyday gate reads campaign
receipts, baselines or source pins: the [structural ratchets](#structural-ratchets)
and the [util-linux runtime products](../dev/util-linux/README.md#parity) use
current inputs, and the records under `dev/consolidation` are historical.

The canonical replay at `9ed42ed4af9a9367ff5be65699cb05302dd39cc0` passed
with 15,503 passed, 0 failed and 174 skipped. The diff from that commit to the
measured base `12827bb0be864a42747ec82f60560fda1b8578cd` changes tests and
documentation only, with no `src/` or `core/` changes.

On the 32-core x86_64 host, the pinned Linux image built the base source with
`release` O2, thin LTO and static musl. Each timing window passed
`perf-gate-ready` before and `perf-gate-ready -1` afterward. `/usr/bin/time`
measured user plus system CPU. The script interleaved startup batches and core
checks. Ranges are the smallest and largest samples.

| Workload | Samples | Median CPU | Range |
|---|---:|---:|---:|
| Trivial script `print 1`, per process | 15 batches of 50 | 1.0 ms | 0.8–1.0 ms |
| `xsht check core` | 5 | 396.45 s | 361.77–427.43 s |

The startup batch includes the shell loop. CPU values are rounded to 0.01 s per
batch by `/usr/bin/time`. Startup runs a one-line `print 1` script file.

The comparative suite passed all 35 parity cases and collected 3,780 samples
in the pinned overlay. It reports wall time, including child processes, on one
allowed CPU with warm filesystem caches. Each cell is pooled median (median
absolute deviation), in milliseconds. Startup has 180 samples per interpreter;
other workloads have 90. This is a workload record, not a language ranking.

| Interpreter | Startup | Spawn | Pipeline | Directory | JSON |
|---|---:|---:|---:|---:|---:|
| `xsh` | 1.215 (0.026) | 17.314 (0.162) | 329.968 (3.407) | 10.708 (0.148) | 8.847 (0.044) |
| `bash` | 0.582 (0.015) | 20.195 (0.253) | 18.894 (0.381) | 18.295 (0.612) | 14.902 (0.226) |
| `dash` | 0.250 (0.005) | 12.368 (0.177) | 18.405 (0.127) | 13.079 (0.277) | 14.361 (0.086) |
| `python` | 6.122 (0.037) | 29.829 (0.170) | 53.534 (0.519) | 21.405 (0.127) | 14.488 (0.263) |
| `ysh` | 0.677 (0.010) | 19.314 (0.200) | 1017.329 (4.557) | 12.038 (0.381) | 12.299 (0.109) |
| `elvish` | 1.312 (0.028) | 15.155 (0.156) | 1676.025 (5.252) | 45.317 (0.476) | 54.631 (0.482) |
| `nushell` | 4.891 (0.061) | 40.528 (0.231) | 622.554 (4.135) | 55.324 (0.211) | 16.472 (0.087) |

See [comparative workloads](../bench/consolidation/README.md) for fixtures,
image preparation and the runner. The outer readiness checks
bracketed the entire parity and timing run.

To repeat the CPU measurements, build release binaries with the same feature
set and target, then run the following from the repository root. Save each
accepted line; discard a window when either readiness command fails. The loop
alternates startup batches with five core checks.

```sh
export XSH_RELEASE="$PWD/.work/tmp/performance-target/x86_64-unknown-linux-musl/release"
printf 'print 1\n' > .work/tmp/trivial.xsh
for n in $(seq 0 14); do
  perf-gate-ready || continue
  /usr/bin/time -f 'startup %U %S' sh -c 'i=0; while [ "$i" -lt 50 ]; do "$1" "$2" >/dev/null || exit; i=$((i+1)); done' sh "$XSH_RELEASE/xsh" "$PWD/.work/tmp/trivial.xsh"
  status=$?; perf-gate-ready -1; ready=$?
  if [ "$status" -ne 0 ] || [ "$ready" -ne 0 ]; then echo 'discard startup sample'; fi
  if [ $((n % 3)) -eq 0 ]; then
    perf-gate-ready || continue
    /usr/bin/time -f 'check %U %S' "$XSH_RELEASE/xsht" check core
    status=$?; perf-gate-ready -1; ready=$?
    if [ "$status" -ne 0 ] || [ "$ready" -ne 0 ]; then echo 'discard check sample'; fi
  fi
done
```

For later comparisons, interleave the baseline and candidate, use the same
profile, target and workloads, and compare medians. A later change fails only
when its regression exceeds the recorded spread. Repeat rejected windows until
there are at least 15 startup batches and five core checks. Binary sizes are in
[Release engineering](RELENG.md#size-record).

Util-linux `resources --check` passed on `a51ad81f3` against a release
baseline prepared from the base commit. Existing commands stayed within
1.4 MiB of the baseline product's peak RSS. New cohort commands peaked at
33–100 MiB against 11–16 MiB for the C reference under the same fixtures;
these are reported, not limited. See
[util-linux acceptance](../dev/util-linux/README.md) for the commands.

## Historical P0 loop measurements

On `fcdae517e`, the scoped-verification P0 measured verification products in
its pinned Linux image on a 32-core x86_64 host. These products used O3; current
verification products use O2. Establish an O2 baseline before comparing current
work. The short-loop batches include shell spawn cost. Peak RSS is the largest
process RSS, not a cgroup peak. Gate times include container startup and product
freshness checks; their CPU and memory were not measured.

| Loop | Samples | Wall median (p90) | CPU median | Peak RSS |
|---|---|---|---|---|
| `xsh --startup` | 20 batches of 50 | 0.8 ms (0.8) | 0.8 ms | 15 MB |
| `xsht test --exact` one native test | 20 batches of 10 | 28 ms (29) | 127 ms | 75 MB |
| `xsht check` one file | 20 batches of 20 | 10 ms (10) | 9.5 ms | 43 MB |
| `xsht check core`, 928 files | 5 | 11.2 s (11.5) | 271 s | 2.8 GB |
| `cargo dev gate --case check-docs`, warm | 3, interleaved | 56.6 s (59.8) | unavailable | unavailable |
| `cargo dev gate --case test-sema`, warm | 3, interleaved | 99.7 s (108.4) | unavailable | unavailable |

Each batch held the shared gate lock and passed `perf-gate-ready` before and
`perf-gate-ready -1` afterward. `xsht check core` used about 24 cores. See
[Release engineering](RELENG.md#historical-p0-product-sizes) for product sizes.

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

Every workspace library and test harness that lists tests has a Change case, and
`dev/tests/test-gates.xsh` checks the manifests against the catalogue. A Cargo
case names one package and one harness, so each harness is its own case. The
Compile area owns the remainder of the root library and integration harnesses:
`test-core-unit` and `test-core-integration` skip only the prefixes that a
focused area case runs. Some harnesses cannot be Change cases. The gate's Cargo
adapter needs libtest's JSON listing and a nonempty selection, so the
`harness = false` suites (`papaparse_test_suite`, the INI `corpus`,
`toml_test_suite` and `yaml_test_suite`) and the libraries with no tests have
none; run the suites with `cargo test -p PACKAGE --test NAME`. Three more
stay with manual lanes: the root-only `core_compat_extra` modules and `linux_priv`
are `#[ignore]`d outside the privileged `cargo dev test linux` container, which
runs them with `--ignored` from the ordinary build, and `profile_parity` compares
builds that exist only on aarch64 Linux and macOS. The `oxish-client` connect, exec,
stream and agent harnesses and the native `tests/xsh/ssh-` tests need disposable
OpenSSH servers: run them through `crates/oxish-client/fixture/ssh-fixture.sh exec
COMMAND` (Docker, loopback only). The `authentication`, `descriptors`, `hostile`,
`known_hosts`, `protocol` and `review_probes` harnesses use a scripted peer but need `ssh-keygen`;
the `private_key_wipe` allocator probe also needs it. The gate image lacks
`ssh-keygen`; run these harnesses with `cargo test -p oxish-client --test NAME` on
a host with OpenSSH or through the fixture script. `xsh-archive`'s `interop`
harness needs the GNU tar and bsdtar oracle image built by
`dev/reference/oracle.sh`.
`split::` runs in its resource-limited container, and the `runtime::coverage`
test and the external-corpus tests that the XSHT runs skip by name are manual.
`test-root-unit` is Linux-only because xsh-root's unit tests cover `openat2`;
the macOS lane uses the security and mutation harnesses.
Start `cargo dev test macos --ci` from a session that does not ignore SIGINT
or SIGTERM (not under `nohup … &`): XSH keeps an inherited ignored signal, so
the signal-cancellation tests then cannot cancel and leak their fixtures.

### Native macOS qualification

Run the native corpus on a macOS arm64 host with `cargo dev test xsh`, and the
Rust lanes with `cargo dev test macos --ci`. Keep `TMPDIR` outside the
checkout: `xsht` ignores files under `.work/`, so a temporary root inside the
repository makes fixture files invisible to the lint and test tools. A
detached run must restore default SIGINT, SIGTERM, SIGQUIT and SIGHUP before
starting the suite, for the reason given above, and a launcher written in
Python must also restore SIGPIPE and SIGXFSZ, which Python ignores: XSH passes
an inherited ignored SIGPIPE on to every child, so writers then fail with
EPIPE instead of dying by SIGPIPE.

A test whose fixture needs a facility macOS lacks skips with the facility's
name, through `core/tests/support/requires.xsh` or the
`system.uname()?.sysname != "Linux"` idiom. Linux runs every such test.

The latest complete run on m1-max (2026-11-19, `4704619ba`) ran 15,805
tests: 14,479 passed, 1,320 skipped with named limitations, and 6 failed. The
host was shared, with a load average above 20 throughout. The tty, sparse-copy,
numfmt and sort cases were fixed or pass when rerun alone; rerun alone, the dhcp-client renewal
and gates selection cases pass, since they timed out only under that load. The
earlier run (`1278debe8`) had 105 failures. Their causes, and the fixes that
removed them, were:

- Darwin FIFOs live on socket buffers. `poll` and `kqueue` miss EOF after the
  last writer closes, and a writer whose readers closed still polls writable.
  `unix.poll_fd` and `unix.poll_fds` confirm both with `select`, `FIONREAD` and
  a zero-length write (`src/modules/unix/poll.rs`).
- Darwin keeps a signal that every thread blocks pending on the main thread,
  so `xsh` evaluates scripts on the main thread there. It links that thread
  with a 16 MiB stack.
- `kill` on a process group that holds only a zombie returns `EPERM`, so
  `timeout` sends `SIGCONT` first on Darwin. Child exit waits use kqueue
  `EVFILT_PROC`.
- Darwin's libc regex lacks the GNU BRE extensions musl provides. Non-musl
  builds compile the vendored musl regex (`third_party/musl-regex`).
- Without `/proc`, `unix.fd_info` and `unix.set_fd_flags` report and set
  descriptor status flags and identity.
- Darwin termios has its own control characters, speeds and disabled value
  (`0xff`).
- `F_PUNCHHOLE` makes sparse copies. APFS keeps a file's final partial block
  as data.
- A directory without search permission is listed from an independent open,
  because `openat(fd, ".")` needs search permission.
- APFS folds case and rejects names that are not UTF-8, and `PATH_MAX` is
  1024. Tests name outputs and size deep trees accordingly.

A focused case that passes stores a record under the checkout's `.work/gate/`,
bound to the source digest and the product digests and modes the run used. The
record is disposable local evidence and is never committed. A case starts by
discarding its previous record, so a failed or interrupted run leaves it not run.
`cargo dev gate [PATH...] [--case ID] --status` lists the selected cases and
reports each as current, stale or not run. Stale names the first mismatch: a
changed source, another target or profile, a changed or missing product, or an unreadable record. Status
only observes source and products; it starts no Docker, Cargo or tests.
Scenario and Laputa cases store their record when the external run passes. A Laputa record also
binds the Laputa root and source digest and, for reuse-count, `xshi`; `--status` with `--laputa-root`
observes the checkout again and reports a changed root or digest as stale. The CLI rejects a status
request for a Laputa case without `--laputa-root`. A `--full` run stores a
source-bound record only after every step passes; until then status reports
`Full: not run`, and a source edit afterwards reports it stale.

Only the sole `cargo dev gate --full` request enters comprehensive verification.
The former `--batch` flag and `internal gate-batch` operation are removed.
`internal gate-full` validates the live Full catalogue, dependencies, aliases and
product closure inside the container before product preparation.
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
Historical PM diagnostics used an older XSH seed. They do not qualify the
current seed, the full world inventory, artifact reuse, or a bootable root.
The historical real-kernel plan lacked an independent inventory oracle. The
synthetic Kbuild scenarios do not qualify the real-kernel scenario.

The explicit `performance/json-n/correctness` scenario runs alone. It generates
its fixture in owned scratch and checks the output and side effects in a fresh,
network-disabled Linux container. It binds the selected source and prepared XSH
product before and after execution. Its resource output is diagnostic only:
unavailable counters are reported as such, and the case does not measure or
qualify performance.

The initial focused cases include `showcase/tokei/correctness` and
`core/lsmem/correctness`. The first selects its native behavior module. The second selects the
lsmem module without its 16 GiB layout test, which needs about 20 CPU-seconds and does not fit the
Edit budget; a Change request adds that test as `core/lsmem/heavy-inventory`.
At Edit tier, `src/sema/check/local_inference.rs` selects the exact
`compiler/local-inference/correctness` tests; Change also retains its broader
checker coverage. `src/sema/schemes.rs` similarly selects the exact
`compiler/imported-inference/correctness` tests at Edit tier. Edits to
`src/sema/inference_work.rs` select the exact
`compiler/inference-work/correctness` Rust test.
A Change request for `core/lsmem.xsh` also selects `core/lsmem/heavy-inventory` and
`core/lsmem/consumers`, an
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
seconds. `test-lint-format-corpus` formats and lints the repository corpus within 1800
seconds. Change selects `test-tooling` without it, and a Change request for a shared core
path reports that no Change case owns it and lists `native-core/tests` as an available scenario. A Change request
for `core/NAME.xsh` selects the `core/tests/test-NAME.xsh` module when it exists.
The runner admits at most eight tests of one file at a time, so a module that needs more
than 2,400 CPU-seconds cannot finish in 300 seconds. `core/tests/test-awk.xsh` (3,300
CPU-seconds, 429 s wall on a host at load 14 to 27) and the single sort-buffer test in
`core/tests/test-gnu-sort.xsh` (366 s) are therefore 1800 s Scenario cases; Change for
`core/awk.xsh` selects its Edit case and lists the module as an available scenario.

Native cases run as the host user inside the gate container, with a private home and
temporary directory under `.work/tmp`, as the Full native run does. That scratch holds an empty
`xsht-config.ini`, so tools a test runs on its fixtures do not read the repository's lint exclusions. The container itself
keeps Docker's default seccomp profile and capabilities. Required skips still make a case
incomplete, so a native test the unprivileged lane cannot run is not selected there.
`dev/gate_lane.xsh` declares those tests by exact ID. Two groups can run in a container
and have an explicit Scenario case each; each must be selected alone.
`lane/root-native` runs the `root_tests` group as root in the default container, with
Docker's default confinement and no network. `lane/namespace-native` runs the `namespace_tests` group as the
host user in a container whose only change is the seccomp profile
`dev/gate-namespaces.seccomp.json`. That file is Docker's default profile (moby
`profiles/seccomp/default.json`) plus one rule that allows `clone`, `clone3`, `unshare`,
`setns`, `mount`, `umount`, `umount2`, `pivot_root`, `sethostname`, `setdomainname` and the new
mount API (`fsopen`, `fsconfig`, `fsmount`, `fspick`, `open_tree`, `move_mount`, `mount_setattr`)
without `CAP_SYS_ADMIN`; the kernel still checks capabilities in the target user namespace. The
container gets no added capability and no device, and `/proc` stays masked and its system
paths stay read-only. Three tests need an unmasked `/proc` (a `procfs` mount inside the new
namespace, or credential transitions that depend on one), so they are declared outside every
lane. A native file whose every test is declared outside the unprivileged lane is a reported gap
that names these scenarios, not a case. A single native file with only some tests outside the
lane is incomplete, exit 2, whether it is requested by path or by `--case native-FILE`: planning
reports the counts as unresolved scope and lists the scenarios that cover its root and namespace
tests. A suite case that spans many files, such as `native-tests/xsh/stdlib`, runs the tests inside
the lane and exits 0 with `Selected scope: passed, reduced (N tests outside the lane; see
lane/root-native, lane/namespace-native)` as its last verdict line. Its report says
`Coverage: reduced`, not complete, and its evidence shows as reduced in `--status`, never as
current. The remaining groups name a platform, an external
fixture that only a Rust test provides, supplementary groups, a host resource, an unmasked
`/proc`, or a test whose subject is the skip outcome; no gate lane runs them. A new native test
that skips in the gate lane needs an entry in one of those groups, and
`dev/tests/test-gates.xsh` checks that every entry names a test that exists.

The gate containers give reproducible environments and keep a test's effects away from the
host session. They are not a security boundary against hostile test code. Test code already runs
as a docker-capable user and can write the checkout and `.git/hooks`. In `lane/root-native` the
container's root is host uid 0, because the daemon does not use user-namespace remapping, so a
test that escapes the container is root on the host. The seccomp profile, the dropped network and
the host-user wrapper reduce accidents; they do not contain an adversary. The native wrapper does
not add `no_new_privs`, because declared tests assert that the flag starts clear
(`test_set_no_new_privs_is_visible_and_inherited` and the `nnp=false` checks in `test-setpriv.xsh`).

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
Frozen-witness tooling changes select its Python contract tests, and annotation-erasure
tooling changes select `verification/erase-sites`. Other inference
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

The Linux signal-storm test (`runtime::signal_storm` in the `integration`
target) derives each run from a fixed seed: a scenario over process scopes,
pipelines, `defer`/`errdefer`, live streams, a PTY redirection, a signal
hook, or a loop of spawns and pipelines that signals interrupt mid-setup;
SIGINT/SIGTERM injection (optionally escalated by a second signal); and
an early-closed stdout. Seeds replay the plan, not the OS scheduling. The
oracle checks bounded exit, no live process carrying the run's environment tag,
one-time ordered cleanup, pipeline output, and an untouched bystander process.
It treats death by an uncaught signal before the script is ready as
non-cooperative. A companion case SIGKILLs the runner after its children start
and requires that no cleanup ran and that the owned process groups are still
running, as the SPEC states. Terminal-mode restoration is covered separately, on Linux
only, by the interactive storm in `crates/xshi/tests/cli.rs`
(`terminal_storm::`): it starts the real `xshi` on a PTY that is the child's
controlling terminal, waits for the shell's raw mode, and delivers a seeded
SIGINT (escalated to SIGTERM, because the shell ignores SIGINT at the prompt),
SIGTERM, or SIGHUP. After a bounded exit it
compares the PTY's termios flags and VMIN/VTIME with the pre-start values, and
it requires no live process carrying the run's tag and an untouched bystander.
A companion case starts the shell with SIGHUP ignored and requires it to survive
a SIGHUP in raw mode.
That test needs `cargo test -p xshi --features laputa-asterinas`. A SIGHUP
raised by closing the master is not observable this way, because the slave then
answers `tcgetattr` with `EIO`, so the seeded SIGHUP is delivered directly.
SIGKILL of the runner cannot restore a terminal and stays out of scope, and
macOS is owner-deferred. `tests/xsh/process-failure-ownership.xsh` holds the
focused failed-spawn, early-exit, partial I/O, descriptor-inheritance and
recycled-pid cases; the recycled-pid case needs unprivileged user and pid
namespaces and skips without them.

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

### Structural ratchets

`dev/source_metrics.xsh` measures structural metrics of the production roots
`src` and `crates/xsht/src`: blanket `dead_code` scopes, instruction, pattern and
stage schema variants, manual payload and optional raw reads, wide arena
wildcards, and the type-mapping, type-predicate, lowering-resolver and nested
machine-factory definitions. `dev/ratchets.json` records the current value of
each. The Full gate's `batch-ratchets` step runs the check:

```sh
xsh dev/ratchets.xsh check --root "$PWD" --report .work/gate/ratchets.json
```

The check fails when a metric rises above its recorded value, when a metric
falls below it, when the metric inventory differs from the baseline, or when
the source breaks the stage-schema contract: exactly 46 generated stage variants
in `src/runtime/eval/indexed/full.rs` and no lowering type resolver. Source line
totals are reported and never enforced. `--report` keeps the measured report
with the evidence location of every counted occurrence.

A change that lowers a metric records the lower value in the same change:

```sh
xsh dev/ratchets.xsh update --root "$PWD"
```

`update` rewrites the baseline with the measured values and refuses a rise.
Raising a value means editing `dev/ratchets.json` by hand and explaining the rise
in review. Changes to the scanner, the command, the baseline or their native
tests select `dev/tests/test-source-metrics.xsh` and `dev/tests/test-ratchets.xsh`.
The scanner records both `directory` and `path_directory` for each module, so
external `#[path]` owners resolve children by Rust's physical-directory rules.
Missing, ambiguous and conflicting owner-sensitive module targets fail the scan.

## Building and release smoke

Build products with `cargo dev build`. Linux tests use the pinned
`Dockerfile.test` image and target settings; a host build is not Linux
verification. `dev/targets.xsh` defines both x86-64 and AArch64 Linux product
targets. The util-linux acceptance adapter currently qualifies its campaign in
the pinned x86-64 image; that scope does not narrow the product target matrix.
The [util-linux README](../dev/util-linux/README.md) lists its checks and parity rules.

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
Linux verification from a linked worktree mounts the shared Git directory
read-only so repository checks can resolve the worktree's Git metadata.

`date --set` and the ignored BTRFS reflink test change host-like kernel state.
Run them only in a disposable QEMU TCG guest with no network or host devices.
Use an emulated 2020 CMOS clock and pass a fresh 256-bit nonce in both the boot
arguments and environment. The tests also check QEMU identity and guest root.
Stage GNU parity tools inside that guest.

The traceroute source-route cases require `CAP_NET_RAW` and loopback-only
networking. The two infinite `/dev/zero` split cases run in a separate
container with a 0.01 CPU quota and a 1 GiB memory limit. The preceding Linux
stage builds the selected optimized XSH product. The limited container invokes
that product directly. Its driver waits for the child to open the device, then
kills and reaps it. The ordinary gate runs this bounded case separately.

### Block-device dd regression

After preparing the verification products, run the two block-device cases in a
separate privileged container. The fixture creates a 4 KiB loop device and
refuses to replace an existing `/dev/sda1`. It checks failed and valid skips for
stdin and named inputs, with and without conversion. Cleanup removes its node,
detaches its loop device and deletes its image, including after a test failure.
No physical disk is used.

```sh
mkdir -p .work/tmp/dd-block
docker run --rm --init --privileged --network none --cpus=2 --memory=2g \
  -v "$PWD:/work:ro" -v "$PWD/target/linux-test:/work/target:ro" \
  -v "$PWD/.work/tmp/dd-block:/scratch" -w /work -e TMPDIR=/scratch xsh-test \
  /bin/bash dev/campaign/dd-block-fixture.sh \
  target/x86_64-unknown-linux-musl/verification/xsht \
  target/x86_64-unknown-linux-musl/verification/xsh
```

## Archive and compression

The XSH-57 implementation was tested on Linux through `56f5fa9f8` and a later
list-budget follow-up. The `test-archive-*` and `test-root-mutation` cases
covered malformed and truncated inputs, limits, path confinement, publication,
cancellation, non-UTF-8 paths and streaming. Native coverage is in
`tests/xsh/stdlib/archive.xsh`. The interoperability fixtures read BusyBox tar,
Python tar and zip, Info-ZIP and cpio archives; BusyBox tar, Python tar and
cpio read archives made by XSH. These historical results do not qualify later
source.

Rooted extraction was cross-compiled for macOS but not run there. On Linux,
root extraction applies non-zero uid and gid and creates character and block
devices and FIFOs with their archived modes, including set-id bits; the device
numbers, kinds, ownership and modes match GNU tar and bsdtar. Unprivileged
extraction ignores ownership by default and reports refused ownership changes.
CPIO hard-link sets are unsupported. TAR members with the GNU
sparse type flag or `GNU.sparse.*` pax keys list as unsupported entries and
extraction refuses them; XSH never extracts a sparse member as its compacted
bytes.

For the root reference fixture, create an empty writable directory and run
`dev/reference/archive-root.sh DIRECTORY`. Run the `rooted` Rust tests as root
in the pinned Linux image with `XSH_ARCHIVE_ROOT_FIXTURE` set to the mounted
fixture directory. Run the unprivileged test as a non-root user as well. The
fixture contains device nodes; do not open them.

`crates/xsh-archive/tests/interop.rs` runs GNU tar 1.35 and bsdtar 3.8 (the
`tar` and `libarchive-tools` packages of the `xsh-oracle` image, started through
`dev/reference/oracle.sh` with no network) in both directions. The reference
tools write ustar, GNU, old GNU, pax and v7 archives (bsdtar: ustar, pax, GNU
and v7) with hard links, symbolic links, a FIFO, set-id and private modes,
non-UTF-8 and non-ASCII names, and names and link targets over 100 bytes; XSH
must report mode, mtime, uid, gid, link target and kind exactly, and its
extracted tree must match the source tree. XSH writes plain, gzip, bzip2, xz,
zstd and lzma tars that both tools extract to the same tree (same files, modes,
links, contents) and list with owner 0/0 and mtime 0. XSH archives hard-linked
files as independent regular files. bsdtar drops the set-id bit when it extracts
as an unprivileged user, so the extracted-tree comparison normalizes that bit and
the listings check it. The tests skip with a stderr line when the image is
absent or lacks the two tools; a stale `xsh-oracle` image needs deleting so
`oracle.sh` rebuilds it with them. Run them with
`cargo test -p xsh-archive --test interop` (`XSH_ORACLE_IMAGE` selects another
image name). On x86_64 Linux: 3 passed, 0 skipped.

Publication is atomic and not durable. `publication.rs` observes the
destination from a second thread while `create` and `extract` run: a created
archive or an extracted tree is visible only complete, and only staging names
beginning `.xsh-archive-` appear beside it. Neither operation calls `fsync`;
a caller that needs the result to survive a crash calls `fs.fsync` on the
output and on its parent directory. Merging into an existing directory can
publish some entries before a conflict.

The XZ reader passes the workspace limit to the decoder (`liblzma` memlimit),
which checks it for the first block, every later block and every concatenated
member, including filter state; integrity checks the decoder cannot verify are
rejected. The `files.rs` cases cover later members, later blocks, filter chains
with trailing data and an unknown check, and a source I/O error is not reported
as a limit.

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
## DHCP client

The DHCP client tests run a pure suite on any host (`test-dhcp-wire`,
`-arp`, `-machine`, `-net`, `-store`, `-dns`, `-frame`, `-client`) and four
namespace suites. `test-dhcp-route-identity`, `test-dhcp-world` and
`test-dhcp-integration` create user, mount and network namespaces joined by a
veth pair and never touch the host's links, resolvers or clocks; they are
declared in the namespace group of `dev/gate_lane.xsh` and run by
`lane/namespace-native`, and a kernel that refuses the namespaces skips them
with the reason. `test-dhcp-reference` compares the client's wire library with
dnsmasq and a tcpdump capture of the same exchange, including dnsmasq's own
encoding of option 121. dnsmasq drops group privileges, which a user namespace
forbids, so that suite needs real root, has no gate lane, and runs in the Linux
test image (`Dockerfile.test` carries `dnsmasq` and `tcpdump`):

```sh
docker run --rm --privileged -v "$PWD":/work -w /work xsh-test \
  PRODUCTS/xsht test test-dhcp-
```

It skips where dnsmasq or real root is absent. The in-process fake world
(`tests/support/dhcp_fake.xsh`) is a test-only `ClientIo`; production
networking cannot be redirected to it.

Idle cost of a bound client (`test_a_bound_client_idles_without_polling_or_growth`,
lease 3600 s, 20 s window, verification build, x86_64): 0 CPU ticks, 0
voluntary and 0 involuntary context switches, 12 descriptors before and after,
2 threads, resident set about 58.6 MB and flat. After a silent server returned,
the first DISCOVER reached it 5.5 s after it started (the client was backing
off) and the ACK followed the REQUEST by 13 ms. The test bounds CPU, wakeups,
descriptors and memory generously and records nothing else.

### DHCP virtual machine qualification

`xinit/tests/fixtures/dhcp_vm_host.xsh` in the Laputa checkout boots a
disposable QEMU guest (KVM, 512 MiB, 2 CPUs) from an initramfs holding a
static musl `xsh` and the matching `core/` tree, with Laputa's Linux 7.2.9
installer kernel, and drives it over QMP. The guest side is
`dhcp_boot_guest.xsh` (init), `dhcp_clock_guest.xsh` (a bound 60 s lease
against the scripted server in a private network namespace, sampling the
monotonic, boot, wall and CMOS clocks every 100 ms) and `dhcp_network_guest.xsh`
(a virtio-net device behind QEMU's user-mode DHCP server). Run from the Laputa
root with `XSH_ROOT` holding `bin/xsh` and `core/`:

```sh
export XSH_MODULE_PATH=XSH_ROOT/core
HOST="xsh xinit/tests/fixtures/dhcp_vm_host.xsh XSH_ROOT KERNEL SCRATCH"
$HOST pause 45                        # QMP stop, 45 s, cont
$HOST pause 45 -cpu host,-kvmclock    # the same without the paravirtual clock
$HOST wall                            # guest wall clock steps +1 d, -2 d, +1 d
$HOST nic                             # carrier flap and device re-enumeration
```

Results, QEMU 11.1.2, KVM, one run each, host load average 24 to 40 on 32 CPUs
(perf gate deferred: host busy; the clock steps below are structural and do
not depend on load, the latencies carry at least the 20 ms polling interval):

| Run | Host stop to cont | Largest step between 100 ms samples: monotonic / boot / wall / RTC | Client |
|---|---|---|---|
| pause 20 s | 20.007 s | 104 / 103 / 104 ms / 20 s | renews at 30 s guest time as before; bound |
| pause 45 s | 45.007 s | 102 / 103 / 102 ms / 45 s | the same; bound |
| pause 80 s | 80.008 s | 103 / 103 / 103 ms / 81 s | the same, though the lease is only 60 s; bound |
| pause 45 s, `-cpu host,-kvmclock` | 45.007 s | 45061 / 45062 / 45061 ms / 46 s | resumes at the lease's expiry, withdraws it, discovers and binds again |
| wall steps | none | 105 / 105 / 172799899 ms / 1 s | T1 renewal 30.019 s after the REQUEST; never left bound |

What this shows, and what it does not:

- With QEMU's default paravirtual clock, a host `stop`/`cont` is invisible to
  the guest's `CLOCK_MONOTONIC`, `CLOCK_BOOTTIME` and `CLOCK_REALTIME`: none
  advanced by more than the sampling period. Only the CMOS clock, read with
  `linux.hwclock`, saw the pause, at one-second resolution. `CLOCK_BOOTTIME`
  counts time the guest kernel was suspended; it does not count time the
  hypervisor withheld the CPU while hiding it from the guest clock, so it
  cannot detect this pause. A lease that expired on a server outside the guest
  during such a pause is therefore noticed only when the next renewal or
  rebinding exchange fails or is refused.
- Without the paravirtual clock the guest keeps time from the TSC and all three
  clocks jump by the pause. `CLOCK_BOOTTIME` and `CLOCK_MONOTONIC` jumped alike
  (45062 and 45061 ms), so the boot clock the client uses for lease age
  treats such a jump as elapsed time, which is the right reading of a
  suspend, and cannot tell it from a pause. The client acted on it: an
  expired lease is withdrawn before any event is handled, and discovery began.
- Which of these a deployment gets depends on its hypervisor and clock
  configuration. The figures are for one QEMU, one kernel and one CPU model;
  nothing here shows that every backend excludes or includes a pause, and the
  client assumes neither.
- Wall-clock steps of one day forward and two days back moved no deadline: the
  largest monotonic and boot steps stayed at the sampling period and the T1
  renewal came at 30.019 s on the boot clock.

Network cases, one run (`nic`), guest boot clock, 20 ms polling:

| Event | Guest ms |
|---|---|
| link present, carrier up | 3133 |
| bound by the user-mode DHCP server (includes three ARP probes) | 10254 |
| carrier lost (QMP `set_link`) / client reports `no_link` | 12272 / 12293 |
| carrier back / `init_reboot` / bound | 15297 / 15318 / 15339 |
| device removed from the PCI bus (interface gone) | 17413 |
| device re-enumerated as a new interface (index 2 to 4) / bound | 18951 / 19017 |

The kernel has no PCI hotplug driver, so QMP `device_del` cannot reach the
guest; the guest removes and rescans the virtio-net device itself, which
destroys and recreates the interface and changes its index. Not covered here:
the scanner as PID 1, `logger`, log rotation and glibc clients belong to the
syslog and `xinit` work, and aarch64 guests are not claimed.

## Interactive parity

Interactive tests require `-p xshi --features laputa-asterinas`; builds
without the feature compile the disabled stub. Use `--lib` for units and
`--test cli --test stdlib_preparation` for CLI and repeated-input preparation.
Linux runs use the pinned image.
History storage unit tests in `crates/xshi/src/interactive/history/tests.rs`
cover lock contention, failed writes and retries, reset, and torn log tails.
They do not establish crash or power-loss durability.

`cargo test -p xshi --features laputa-asterinas --test parity` drives the real
`xshi` binary through a kernel PTY and compares each scenario with
`tests/fixtures/interactive-parity/<os>/`; ordinary runs need no `ish` install.
Set `XSHI_PARITY_ISH_BIN` to an `ish` binary to replay the same scripts against
`ish` and fail on drift, and add `XSHI_PARITY_RECORD=1` to rewrite the goldens
from that binary. Recording and drift runs require a six-character short host
name; run them in a `--hostname sentry` container or under
`unshare --user --map-root-user --uts` with `hostname sentry`. Set
`XSHI_PARITY_FULL=1` to print the full transcript instead of the failure window.
The driver is `crates/xshi/tests/parity.rs`; scenarios are in
`crates/xshi/tests/parity/`.
`tests/fixtures/interactive-parity/known-gaps.txt` lists the scenarios where
`xshi` does not yet match `ish`; a listed scenario must still differ, so the
entry is removed in the change that closes the gap.

## What not to run

- Do not run formatters or autofixers in verification. Check-only forms are
  safe. The repository does not accept formatter-generated churn from tests.
- Avoid unfiltered `cargo test --profile verification --target "$TARGET"`.
  The root `runtime::coverage` test runs the full native suite.
- Do not run process tests in debug. They require verification or release
  products. Debug assertions run in the focused debug unit gates of
  `cargo dev gate`; the full contract runs library unit tests in its test
  profile.
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

### Profiling

Build `xsh` and `xsht` with the verification profile's optimization settings,
DWARF line tables and Rust frame pointers. The build has its own target
directory, so its Rust flags cannot invalidate verification or release
artifacts. On Linux x86_64, run:

```sh
CARGO_BUILD_JOBS=8 cargo dev profile
PROFILING_BIN="$PWD/.work/tmp/profiling-target/x86_64-unknown-linux-musl/profiling"
perf record --call-graph fp -o .work/tmp/xsht-perf.data -- "$PROFILING_BIN/xsht" check core/lib/awk.xsh
perf report --stdio -i .work/tmp/xsht-perf.data
```

The profile command also accepts the other supported Linux target through
`cargo dev profile --target aarch64-unknown-linux-musl`; use that target's
triple in `PROFILING_BIN` when running on a matching host. To render an SVG
flamegraph from the recorded call chains:

```sh
perf script -i .work/tmp/xsht-perf.data > .work/tmp/xsht-perf.script
xsh showcase/perf-collapse.xsh -- .work/tmp/xsht-perf.script > .work/tmp/xsht-perf.folded
xsh showcase/flamegraph.xsh -- .work/tmp/xsht-perf.folded > .work/tmp/xsht-perf.svg
```

### Profile btop

`dev/perf/btop-profile.xsh` is a Linux release workload. Set `XSH_BIN` to the
absolute release binary built from the revision under test. The profile checks
host readiness before and after the run and requires `perf-gate-ready` to pass.

On 2026-11-14, release `1598ef041` in pinned Linux image `3564b3d70fac`
passed five runs of twenty preloaded 1,000-process frames at 120 by 40, seed
424242. Per-run p95 ranged from 117.17 to 126.18 ms (median 123.75 ms);
the profile now enforces 250 ms. Every pre- and post-readiness check passed.
The measured region covers model, render and diff, with collection and fixture
setup outside it. Whole-profile peak RSS was 182,596–183,788 KiB per process;
the 1k-to-2k process-join median ratios were 1.97–2.01. Evidence is under
`.work/tmp/resume-btop/`. These are absolute qualification results, not a
comparison with the earlier unqualified measurements.

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

### Test timing records

Every `cargo dev test linux` run records its timings under
`.work/test-records/latest`: libtest output with per-test `--report-time`
durations for each Cargo test binary in `cargo/`, and the native suite's
`--events-jsonl` stream in `native-events.jsonl`. A new full run replaces the
directory. Rank the latest run with:

```sh
cargo dev test-health            # print rankings
cargo dev test-health --record   # also write bench/test-suite-health-YYYY-MM-DD.json
```

`dev/performance/rank.py` ranks any recorded native event stream or Cargo
timing file directly, for example a bounded native run:

```sh
xsht test --jobs 8 --events-jsonl .work/tmp/native.jsonl core/tests/test-uu-sort-
python3 dev/performance/rank.py .work/tmp/native.jsonl
```

The native run start records its worker count, and each case start records a
zero-based worker and nanoseconds since the run started. File time is the sum
of case durations; file span runs from its first case start to its last case
end. Worker idle includes setup, scheduling and teardown outside case
durations. The tail lists the last case starts and the last case to finish,
which is the suite's critical path. Unstarted cases have no schedule and are
omitted. Cargo binaries record through `dev/cargo_test_runner.sh` when
`XSH_CARGO_TIMINGS_DIR` names a writable directory: the harness output stays
readable and also goes to one file per binary, with `<N.NNNs>` after each test.
Ignored tests have no duration. Gate Cargo runs request the same timings in
their scratch event streams.

### Suite health

The suite is part of the product: its wall time decides how often the full
contract runs, and a test that costs far more than the defect it can catch
crowds out tests that would catch more. Keep it measured and audited.

**Records.** After a full contract run that is meant to stand as evidence, run
`cargo dev test-health --record` and commit the dated
`bench/test-suite-health-*.json` with the change it measures. Its label is the
commit the full run measured, which the run records beside its timings; each record keeps the native wall time, case-time sum, worker
idle, the slowest files and cases, the tail, and each Cargo binary's slowest
tests. Compare against the previous record before claiming a speedup or
accepting a regression, and note the host load: rankings stay useful under
load, absolute times do not.

**Current record.** `bench/test-suite-health-2026-11-17.json` measured
`c716265d1` on the 32-thread x86_64 host with warm build caches. The full
contract passed in 1,374 s, against 3,158 s for the serial contract at
`4f9874c4f` with a cold build. The `xsht` integration stage fell from 1,875 s to
354 s, mostly from deleting the desugared-corpus test and bounding the
lint-format corpus work; the privileged stage from 183 s to 3 s by running its
root-only tests from the ordinary build; the native suite ran 15,480 passed and
174 skipped in 831 s of wall time with 12,616 s of case time (17,137 s before
the audit). Its 16 workers were 94 to 96% busy, so the suite is now bound by
total work rather than by its tail; its single longest case is
`test_gnu_sort_sort_buffer_size_log` at 341 s (XSH-97), and per-process
checking of large applet scripts dominates the remaining case time; a
cross-process program cache was measured and declined
([RFC](rfc/XSH-96-checked-program-cache.md)).
Library unit tests in the verification profile took about as long to compile
as the debug build they replaced, so that change removed a second dependency
build but no wall time.

**Cost rules.** These make a test a suspect, to be judged, not deleted:

- a native case of 30 s or more, or a native file whose case time exceeds 2% of
  the suite's case-time sum;
- a Rust test that libtest reports running for over 60 s, or a Cargo binary
  whose case time exceeds 10% of the full contract's wall time;
- any test in the ordinary contract that re-runs the native corpus, re-checks
  or reformats the whole repository, or copies the workspace; such checks
  belong in a Scenario gate case unless their unique coverage is named;
- a test that waits out a real timeout or sleep instead of a short limit or an
  injected clock, or that spawns one process per row where one invocation would
  cover the rows.

A new test that crosses a rule names, in its commit, the defect it catches that
no cheaper test catches.

**Audit procedure.** Rank the latest record. For each suspect in the top of the
case-time and tail rankings, read the test, its fixtures and the code under
test, and decide:

1. What it asserts and which plausible defect it detects.
2. Whether a cheaper test at the same boundary already detects it; name it.
3. Where the time goes: real work in the code under test, per-process startup
   and checking, oversized inputs, repetitions, or waiting.
4. A disposition: **Retain**; **Fix** execution drag without changing what is
   checked (batch invocations, smaller inputs that still cross the boundary
   being tested, short limits, in-process calls); **Consolidate** duplicated
   coverage into the outermost test that pins it; or **Delete** only with a
   named cheaper test covering the same contract. Missing evidence means not
   ready to delete; a slow test is not deletable for being slow.
5. Cost that comes from slow product code is a product finding: file it in
   `TODO.md` instead of shrinking the test around it.

Reducing a recorded upstream conformance log to a sample, or moving a check out
of the ordinary contract, changes what the suite proves and needs the owner's
decision.

**When to audit.** When a full contract run regresses by more than 10% against
the last record, when a new file enters the top ten by case time, and at each
roadmap milestone.
