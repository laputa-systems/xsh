Prefix: XSH
Next: 121

Apply the [design principles](docs/DESIGN.md) to each slice: prefer reusable
host boundaries and ordinary XSH policy over command-specific runtime growth.
Existing distribution follow-ups remain in
[Laputa's queue](https://github.com/laputa-systems/laputa/blob/master/TODO.md);
its [roadmap](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md)
owns VM milestones. Implement each item in its owning repository.

The joint review on 2026-11-13 inspected XSH `d9d1d7bcb78c` and Laputa
`bfe60a9ac40f`, with priorities assumed from impact and likelihood. It excluded
work already owned by the queues, roadmaps, and
[campaign handoff](dev/campaign/RESUME-2026-11-14.md). Runtime proof limits are recorded in each new item.

The systems-foundation items were reviewed against `9d893e0075d2`; findings
marked unproven need a reproducer, not an assumed defect. Existing acceptance
items remain open. The performance/deployment and Lean campaigns retain their
own pause/launch rules; this queue update resumes neither. The Deser
codec cutover is adopted unconditionally. Native macOS qualification
is deferred by the owner, with no macOS host available. The [G0 release candidate](docs/ROADMAP.md#g0-release-candidate) decides
which items block a release. Do not make an unrelated full baseline an entry fee
for a bounded fix; use the narrow applicable gate. Real-hardware enablement is
out of scope.

### XSH-35 · P2 · Qualify clock domains and deadline conversions

- **Area:** Runtime contracts
- **Where:** `src/modules/time.rs`, `src/runtime/eval/`, `crates/xsh-registry/`, `docs/templates/SPEC.md`
- **Problem:** Wall time is epoch milliseconds, `time.monotonic()` is nanoseconds from a process-local origin, and `time.boottime()` uses nanoseconds from a host clock. Linux boottime includes suspend; macOS uses its monotonic clock. All three use integers. Laputa xinit now converts monotonic nanoseconds to milliseconds for elapsed deadlines; other consumers and conversion boundaries remain unqualified.
- **Recommendation:** Qualify the remaining consumer and conversion boundaries against the acceptance criteria below, preserving the recorded platform deferrals. - **Progress:** Pinned Linux host tests bracket wall, process-monotonic and boot-clock readings against their owning clocks. DHCP one-shot deadlines saturate without overflowing and reject unrepresentable CLI seconds before network setup; focused tests cover cancellation, zero budgets and existing wall/boot-clock jumps. The Linux inventory of `core/`, `core/lib/` and the Rust wait paths is complete. Rust waits (`Instant`, `poll`, process and netlink deadlines) are monotonic and take Durations of at most `u64` milliseconds, which cannot overflow `Instant` on 64-bit Linux; a `time.sleep` of the largest millisecond Duration is cancelled by SIGTERM rather than panicking. `syslogd` and DHCP use the boot clock deliberately. `lib.monotonic` gives `Reading` and `Deadline` nominal types so a wall value cannot meet a monotonic deadline, with tests for zero, negative, expired and saturating budgets; `udevadm`, `nc`, `traceroute`, `ping`, `curl`, `wget`, `dd` and `mdev` now use it. `iw` read wall milliseconds as nanoseconds and is fixed. `ping` keeps every send, drain and run deadline as a `Deadline` and no longer adds raw `Reading.ms` integers. `ping` and `traceroute` keep the kernel `SO_TIMESTAMPNS` stamps for microsecond round trips but bound each with the gap between two `lib.monotonic` `Stamp`s taken around the same send and receive (`round_trip_ns`): a wall difference that is negative or longer than that gap proves a step and the monotonic gap answers instead, so a step can no longer distort an RTT or the summary time. Pure monotonic RTTs were rejected: script latency made loopback RTTs 0.5 to 0.9 ms against 0.12 ms, and `SO_TIMESTAMPING` has no monotonic source. A step smaller than the script latency, or a backward step that leaves the difference positive, still passes through, bounded by the monotonic gap. `ping -D` still prints the kernel wall stamp. The remaining hand conversions now go through unit-safe helpers: `lib.monotonic` gains `reading_of`, `stamp_after` and `wait_ms`, and `lib.boot_clock` gives the boot clock its own `Boot` type with `millis`, `micros` and `elapsed_us`. `btop` and `fdu` poll deadlines, the btop replay deadline, `iw -r`, `syslogd` report intervals and the DHCP boot-clock milliseconds use them, and `tui_input.wait_ms` replaces two copies of the Escape-deadline clamp. Defects found: the Escape deadline overflowed at the largest millisecond reading and now saturates; a curl cookie `Max-Age` past the integer range overflowed, and one that cancelled the wall second became `0`, a session cookie, so `http_transfer.cookie_expiry` saturates and maps a non-positive age to second 1. Rust host tests cover wall milliseconds before and after the epoch and at saturation, and the monotonic and boot-clock overflow refusals. Tests inject readings for forward and backward steps; none changes the host clock. The `btop` scheduler and `time.monotonic()` readings it feeds stay process-monotonic integers its own validators bound. - **Remainder:** Laputa's recorded Linux x86-64/KVM self-hosting guest changes its wall clock, checks xinit readiness, restart and stop budgets, and exercises acknowledged QMP pause/resume. This is the bounded saved-input proof in Laputa's `docs/SELF-HOSTING.md`, not a qualification of the latest seed or every migrated command. Physical suspend remains unqualified. Native tests cannot step the wall clock inside each migrated command, so their wall-jump immunity still rests on the types, not an injected clock. macOS is owner-deferred. Keep wall timestamps for calendar and security decisions; never persist a process-local instant as a restart-stable timestamp. - **Acceptance:** Native tests separate wall and monotonic inputs, cover backward/forward wall jumps, zero/expired budgets, overflow and cancellation; host tests establish the units/origin. Qualify VM pause separately from physical suspend. Qualify xinit's conversion and any further consumer migration with unit-safe conversions; update authored docs before regeneration; no host clock changes. - **Coordination:** Laputa LAP-27 owns xinit migration and disposable-guest clock tests.
- **Fix with:** make-invalid-states-unrepresentable
- **Effort:** M

### XSH-36 · P2 · Qualify descriptor and process ownership under failure

- **Area:** Resources
- **Where:** `src/modules/`, `src/runtime/process.rs`, `tests/xsh/process-failure-ownership.xsh`, `tests/xsh/unix-spawn-ownership.xsh`
- **Problem:** Linux qualification found no defect: repeated failed spawns, early consumer exit, partial input and output, and descriptor inheritance leave no owned children or leaked descriptors, and a released or stale handle cannot signal a recycled pid. macOS has no qualification, and no backend limitation is recorded for it. Resolved observation: The full Linux verification at `d62ee5746` failed `core/tests/test-uu-factor.xsh::test_uu_factor_parallel` once with `io: No child process (os error 10)`. That commit added `test_unix_spawn_process_group_new_session_leads_its_own_session`, the only `unix.shutdown_process_groups` test that ran inside the native runner process. The runner runs tests concurrently on threads, and that shutdown drains every exited child with `wait(-1)`, as its PID 1 contract requires. A controlled reproduction showed it reaping another test's exited child, whose own wait then failed with ECHILD. The test now runs its scenario in a child script like its siblings, so its drain can reap only that script's children. Any new caller of `unix.shutdown_process_groups` or `unix.reap_child_events` in a native test must also run in a child script.
- **Recommendation:** Run the same cases on macOS, where `/proc` is absent, so the child and descriptor inventories need another source. Record any backend that cannot enforce a contract instead of treating a no-op as enforcement. - **Acceptance:** The cases in `tests/xsh/process-failure-ownership.xsh` pass on native macOS. Native macOS qualification is deferred by the owner. - **Coordination:** Laputa LAP-34 exercises the service lifecycle; retain the host-process/bounded-stream concurrency model, not a new async runtime.
- **Fix with:** lifecycle-concurrency
- **Effort:** S

### XSH-37 · P2 · Qualify confined filesystem operations for privileged consumers

- **Area:** Host authority
- **Where:** `crates/xsh-root/`, `crates/xsh-archive/`, `src/modules/fs.rs`, `src/modules/archive/`, `tests/`
- **Problem:** `xsh-root` confines pathname resolution, but lexical Path validation and an `fs` effect are not filesystem authorization. Archive extraction (`archive.extract` and `FsRoot.extract`, one engine), `patch.apply`, `FsRoot.rename`/`fsync` and `fs.root_install_file` acquire one anchor and resolve beneath it; Path-taking extraction and publication acquire that anchor from the path. Rooted file reads in archive creation, patch, `FsRoot.fsync` and `FsRoot.read_regular` open non-blocking and refuse a non-regular file; the convenience `FsRoot.read` and `read_result` still block on a FIFO by design, and a privileged consumer takes `read_regular` instead.
- **Recommendation:** Add `FsRoot` hard-link and special-file creation only if Laputa LAP-33 shows a need. - **Status:** The Linux acceptance is met. `crates/xsh-archive/tests/interrupted.rs` (`test-archive-interrupted`) SIGKILLs a child mid-extraction (path and retained-root destinations, and over an existing destination), mid-creation, and during repeated publication. `tests/runtime/root_install.rs` SIGKILLs a script mid-`fs.root_install_file`, fresh and replacing. In every case staging is left behind and no partial destination or replaced file becomes visible. `rooted.rs` and `publication.rs` cover ancestor and symlink replacement of a retained root. LAP-33 migration gaps landed: `FsRoot.mkdir(parents: true)` accepts an existing directory; `fs.root_install_file_sha256` returns the SHA-256 of the bytes it copied; `FsRoot.tempdir(prefix)` creates an owned private directory removed through the parent root's descriptor, so it survives the parent tree being moved; `FsRoot.syncfs()` flushes the containing filesystem (Linux only); `FsRoot.read_regular(path, max_bytes)` opens one regular file beneath the root with symlinks refused and without waiting for a peer, decides on the opened descriptor (refusing a non-regular entry or an oversized file with stable kinds), and returns the bytes with that descriptor's identity, so a group-writable control inbox can be drained without a prior `lstat`. `FsRoot.children` keeps returning root-relative paths, which its contract and the system-report callers rely on; only the documentation was sharpened. Only the owner-deferred macOS work and the items conditional on Laputa LAP-33 (`FsRoot` hard-link and special-file creation) remain; the ticket stays open for those. - **Acceptance:** Adversarial fixtures cover extraction into a retained root under ancestor and symlink replacement, and interrupted publication by process kill. Power-loss durability stays unclaimed; atomic visibility and durability stay tested as separate contracts. macOS enforcement is owner-deferred and must fail explicitly until qualified. - **Coordination:** Laputa LAP-33 owns package adoption; LAP-28 owns build-process isolation; `linux.run_confined` (SPEC §15) is the one process confinement claim.
- **Fix with:** isolate-effects
- **Effort:** S

### XSH-43 · P2 · Stress process and stream lifetimes with signal storms

- **Area:** Runtime verification
- **Where:** `tests/runtime/signal_storm.rs`, `crates/xshi/tests/cli.rs`, `crates/xshi/src/interactive/term.rs`, `docs/TESTING.md`
- **Problem:** The Linux seed-replayable storm (`runtime::signal_storm`) found no orphan, double cleanup or unbounded cancellation, including signals that interrupt a loop of spawns and pipelines mid-setup. SIGKILL of the runner runs no cleanup and leaves its owned process groups running, as the SPEC now states and a companion case pins. macOS is not covered. A second signal that lands after cleanup but before exit kills the runner with the default action, which the oracle accepts.
- **Recommendation:** Add the macOS run when a host exists; the terminal-restoration PTY session is covered on Linux. - **Acceptance:** The storm passes on native macOS and checks terminal restoration. Native macOS qualification is deferred by the owner. - **Progress:** The interactive PTY storm (`crates/xshi/tests/cli.rs`, module `terminal_storm`) starts the real `xshi` on its controlling PTY, waits for raw mode, and delivers a seeded SIGINT (escalated to SIGTERM), SIGTERM, or SIGHUP. It checks bounded exit, ICANON/ECHO/ISIG/IXON/ICRNL/OPOST and VMIN/VTIME restoration, and no tagged stragglers. It found that `xshi` had no SIGTERM/SIGHUP handler, so a signal during raw mode left the terminal raw; `crates/xshi/src/interactive/term.rs` now installs a one-shot handler for each of the two signals that is still at its default action, restores the saved attributes and re-raises the signal; an inherited ignored SIGHUP stays ignored, which `an_inherited_ignored_hangup_stays_ignored_in_raw_mode` pins. Linux x86-64 musl. Closing the master to raise SIGHUP is not observable afterwards (the slave answers `tcgetattr` with `EIO`), so SIGHUP is delivered directly. SIGKILL cannot restore a terminal and is out of scope; macOS stays owner-deferred. - **Coordination:** XSH-36 owns the resource contracts; Laputa LAP-36 supplies the xinit/PID 1 consumer, and LAP-34 owns its first hermetic lifecycle replay.
- **Fix with:** lifecycle-concurrency
- **Effort:** S

### XSH-55 · P2 · Qualify the native SSH client

- **Area:** Remote execution
- **Where:** `crates/oxish-client` (vendored from ssh-build `client/`, `scripts/vendor-ssh-client.sh`), `src/modules/ssh.rs`, `docs/rfc/XSH-55-native-ssh-client.md`
- **Problem:** The Linux x86-64 musl client remains experimental. Native macOS qualification is owner-deferred, and the build-time gate remains deferred. Known limits remain: the vendored Graviola SHA-256 contexts are unwiped, and the client's single implementation is ssh-build's `client/` (XSH vendors it and carries no divergent copy).
- **Recommendation:** Keep mandatory host verification, typed execution certainty, and the experimental label. Preserve the RFC's limits and qualify the remaining platform and build-time gates when resumed. - **Progress:** `7a3809eaa` bounds agent socket establishment and wipes growing private-key buffers. An independent review verified both fixes with 35 focused tests and the original backlog and allocator probes, with zero remaining findings in that scope. All 144 disposable OpenSSH fixtures and package clippy pass on Linux x86-64 musl. This closes the independent review of those fixes, not every protocol or cryptographic risk. - **Acceptance:** The RFC's required tests pass against disposable OpenSSH servers, with ssh-build's OpenSSH client as oracle, on the executed Linux musl and macOS arm64 lanes; the client is labeled experimental. Native macOS qualification is deferred by the owner. - **Coordination:** Laputa R6 and XSH-56 consume the client.
- **Fix with:** security
- **Effort:** L

### XSH-56 · P2 · Complete xsh-infra qualification

- **Area:** Remote execution consumer
- **Where:** `laputa-systems/xsh-infra`, `docs/rfc/XSH-56-xsh-infra.md`
- **Problem:** xsh-infra implements remote reconciliation over native SSH. Its Alpine container scenarios and the pyinfra 3.5 differential now cover every resource kind, packages, services, commands and failures, and agree with pyinfra apart from two recorded deviations. Two-process cancellation is done (xsh-infra `7cd069a`): a SIGTERM or SIGINT exits 3 and reports the resource in flight as unknown and every resource not yet run as blocked, and 11 Alpine scenarios hold the before, after and in-flight boundaries with target-side gates and prove the rerun converges. Single-request kinds have no holdable interior. The file, system, command, failure, process and cancellation suites also pass on a pinned Alpine 3.22.8 KVM guest with OpenRC as PID 1 (xsh-infra `df05b26`). xsh-infra `e293bfd` (vendored as Laputa `be2fbe3`) adds an `xsh` inventory dialect for Laputa, whose login shell compiles commands as XSH, and an xinit service adapter: against a qemu-maintained KVM guest over native SSH, plan, apply (a managed file and a `logger` restart, pid 84 to 277, ready), an unchanged rerun and `check` all converge, and declaring `enabled` is refused because xinit has no enable. The 180-case suite passes against Alpine containers. Laputa package, account and OpenRC resources are refused under the `xsh` dialect; macOS is owner-deferred. Laputa is replacing its older in-tree xinfra with this implementation.
- **Recommendation:** Retain the recorded Alpine/OpenRC and Laputa/xinit proof identities and the explicit Laputa resource refusals. Native macOS qualification remains owner-deferred. - **Acceptance:** The RFC's per-resource and integration scenarios pass against disposable Alpine SSH hosts, with pyinfra as a differential reference on equivalent hosts; XSH defects found get minimal reproducers in XSH. - **Coordination:** Remote execution uses the experimental XSH-55 client.
- **Fix with:** isolate-effects
- **Effort:** L

### XSH-39 · P3 · Qualify bounded Unix-socket I/O for VM control

- **Area:** Host I/O
- **Where:** `src/modules/`, `crates/xsh-registry/`, `core/lib/`, `tests/`
- **Problem:** `unix.connect_stream`, descriptor I/O and polling support Laputa's XSH QMP client, which owns and closes its socket and uses a monotonic deadline. Linux host tests cover short-write backpressure and retries, scoped closure while the interpreter remains alive, shutdown during a bounded poll, and a native line-protocol fixture on `lib.monotonic` deadlines: byte-fragmented and coalesced frames, EOF, a stalled peer, oversized unterminated and terminated frames, abrupt close mid-frame with a stable descriptor count, and SIGTERM mid-frame with cleanup and peer-visible closure. `unix.poll_fd` and `unix.poll_fds` both end at a handled signal, so Laputa's QMP client cancels promptly. Laputa's recorded Linux x86-64/KVM self-hosting proof corroborates a real QEMU monitor stop/resume cycle, with three seconds between acknowledgments and rejection of a resume marker before the stop acknowledgment. This retains that record's declared saved inputs; macOS remains unqualified.
- **Recommendation:** Keep framing and QMP policy in the XSH consumer. - **Acceptance:** The same properties hold on the macOS host path, which remains owner-deferred. Real-monitor corroboration is bounded to the recorded acknowledged stop/resume cycle, not every host framing-fault case. No futures runtime or callbacks are required.
- **Fix with:** isolate-effects
- **Effort:** S

### XSH-111 · P3 · Lift the SSH client's experimental label

- **Area:** Remote execution
- **Where:** `crates/oxish-client`, ssh-build `client/`, `crates/oxish-client/vendor/graviola`, `docs/rfc/XSH-55-native-ssh-client.md`
- **Problem:** The independent review ([report](docs/reviews/XSH-111-ssh-client-review.md)) found six defects, all fixed in ssh-build `6201a5a`, but it excluded Graviola's primitives and ran on Linux only, so the client keeps its experimental label (decided 2026-11-19).
- **Recommendation:** Commission a review of the Graviola primitives XSH uses, and record a native macOS arm64 interop run against OpenSSH.
- **Acceptance:** Both are recorded in the [RFC](docs/rfc/XSH-55-native-ssh-client.md#open), and the label is removed from `docs/ssh.md` and the RFC.
- **Fix with:** security
- **Effort:** L

### XSH-112 · P4 · Port buildlib's source locks to XSH

- **Area:** Tooling
- **Where:** `bench/consolidation/prepare.py`, [laputa-buildlib](https://github.com/laputa-systems/buildlib) `buildlib.locks`
- **Problem:** `bench/consolidation/prepare.py` fetches its inputs without buildlib's verified source locks, and buildlib is Python-only, so XSH build scripts cannot use the contract natively.
- **Recommendation:** Port `buildlib.locks` (source-input validation and verified downloads) and `buildlib.hashing` to an XSH library that follows the [build repository contract](https://github.com/laputa-systems/stuff/blob/main/docs/build-repos.md), then move the consolidation bench preparation onto it.
- **Acceptance:** The XSH library reads `sources.lock.json`, rejects inputs whose SHA-256 or size differs, and the consolidation preparation fetches through it.
- **Fix with:** reference-parity
- **Effort:** M

### XSH-113 · P4 · Stop locking unused Deser bridge crates

- **Area:** Dependencies
- **Where:** [laputa-systems/deser](https://github.com/laputa-systems/deser) `deser-core/Cargo.toml`, XSH `Cargo.lock`
- **Problem:** `laputa-deser-core`'s `std` feature names its optional bridges through weak features (`rust_decimal?/std`, `bigdecimal?/std` and others). Cargo locks dependencies named that way even when nothing enables them ([rust-lang/cargo#10801](https://github.com/rust-lang/cargo/issues/10801)), so XSH's lockfile carries `rust_decimal`, `borsh`, `rand`, `wasm-bindgen` and `toml_edit`, none of which is built. `cargo fetch` and Laputa's vendoring still download them.
- **Recommendation:** In the fork, give each bridge a feature that enables `dep:X` and `X/std` together so `std` no longer names the optional crates; publish and update XSH.
- **Acceptance:** XSH's `Cargo.lock` contains no Deser bridge crate that no enabled feature builds.
- **Fix with:** none
- **Effort:** S

### XSH-116 · P3 · Connected reader decodes record labels from the wrong table

- **Area:** Formal verification
- **Where:** `formal/Xsh/Conformance/Connected/RawExpression.lean` (`readRawValueFields`), `src/runtime/eval/indexed/full.rs` (`FullCodec for Name`)
- **Problem:** Rust writes an `ExprRecord` entry's label as a process-interner symbol word, but the connected reader resolves that word as a string-pool ID through `readRawStringBytes`; the connected assignment-path decoder keeps labels as raw symbols. No connected document carries an `ExprRecord` row yet, so the mismatch is unexercised, and interner symbol words are not stable bytes in any case.
- **Recommendation:** When the connected path first exchanges a record row, carry labels through the canonical label table in [the record-local design](docs/rfc/lean-vertical-record-locals.md) and read them from it in both connected decoders.
- **Acceptance:** A connected document with an `ExprRecord` row decodes its labels to the spellings Rust wrote, and a test fails if the reader uses the string pool.
- **Fix with:** reference-parity
- **Effort:** S

### XSH-125 · P3 · atomically publishes after a SIGKILL race on macOS

- **Area:** Runtime
- **Where:** `tests/xsh/atomically.xsh` (line 59 scenario), `src/runtime/eval/modules/unix.rs` (self-signal path near line 355)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19) the atomic-publication test fails intermittently: the child exits 137 and `out` is published, although a SIGKILL before commit must leave only the temporary file. A direct self-`SIGKILL` probe behaved correctly, so the cause is signal-delivery timing, not yet located.
- **Recommendation:** Loop the exact test under `xsht test --exact-id` on macOS with a sample of the child at kill time, and check whether Darwin delivers the self-signal after the rename returns.
- **Acceptance:** The test passes 200 consecutive runs on native macOS and still passes on Linux.
- **Fix with:** root-cause
- **Effort:** S


### XSH-129 · P2 · master CI is red on Lint and Test

- **Area:** Tooling
- **Where:** `dev/test_workflows.xsh:259`, `crates/xsht/tests/lint_format_invariance.rs`; corpus files `core/lib/ipc_report.xsh`, `core/lib/locks.xsh`, `core/lib/memory_report.xsh`, `core/lib/scols_escape.xsh`, `core/tests/test-lsipc.xsh`, `core/tests/test-lslocks.xsh`, `core/tests/test-tail.xsh`, `dev/tests/test-targets.xsh`
- **Problem:** Since at least `c4f2f0411` (2026-11-19), the CI Test job stops before any test because `HOST_UID` is unset when the job runs as root in the container. The Lint job, and locally `lint_format_invariance` in `cargo dev test linux`, fail on findings in the listed files: `lint.shadowing`, `check.ambiguous-grouping` in `ipc_report.xsh:158` after formatting, `check.redundant-parens`, and warnings. The linter's rules are newer than those files.
- **Recommendation:** Pass `HOST_UID`/`HOST_GID` from the workflow, or skip the ownership repair when they are absent. Fix the listed findings at their sources.
- **Acceptance:** Both CI jobs pass on master, and `cargo dev test linux` passes `lint_format_invariance`.
- **Fix with:** root-cause
- **Effort:** S
