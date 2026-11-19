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

### XSH-111 · P2 · Independent protocol and security review of the SSH client

- **Area:** Remote execution
- **Where:** `crates/oxish-client`, ssh-build `client/`, `docs/rfc/XSH-55-native-ssh-client.md`
- **Problem:** The client stays experimental until an independent protocol and security review covers key exchange, rekeying, authentication, channel handling and host verification.
- **Recommendation:** Commission a reviewer outside the implementation lanes; record findings and fixes in the RFC. - **Progress:** The review ran on 2026-11-18 against ssh-build `c9733b5` ([report](docs/reviews/XSH-111-ssh-client-review.md)): one medium and five low findings, all fixed in ssh-build `6201a5a`, vendored, and recorded in the [RFC](docs/rfc/XSH-55-native-ssh-client.md#independent-review-xsh-111). - **Remainder:** the owner's decision on the experimental label.
- **Acceptance:** The review report and the fixes for its findings are recorded; the experimental label is revisited with the owner.
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

### XSH-118 · P2 · Daemon startup signals are not observed blocked on macOS

- **Area:** Runtime
- **Where:** `src/modules/unix/daemon.rs` (`daemon_start`, `wait_ready`, `SignalCapture::forward`; test `daemon_boundaries`, worker mode)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19) `modules::unix::daemon::tests::daemon_boundaries` fails in its worker case: the worker sends SIGHUP and SIGTERM to its `daemon_start` parent, which forwards them while the worker blocks both, but within 3 s `sigpending` in the worker never reports them ("startup signals were not forwarded while blocked"), so `daemon_start` returns the worker's exit instead of readiness. Every earlier case in the test passes. Running the worker on libtest's main thread (`--test-threads=1`) does not change it, so per-thread pending sets alone do not explain it. The daemon readiness contract (SPEC `unix.daemon_start`) is unqualified on macOS until this is understood.
- **Recommendation:** Instrument both sides on macOS: whether the parent's capture handler records the worker's signals (`PENDING`), whether `forward` sends them, and the worker's blocked mask after `exec`. Fix the implementation if forwarding is lost; record a platform limit in the SPEC if macOS delivery differs.
- **Acceptance:** `daemon_boundaries` passes on native macOS and Linux, or the SPEC records the macOS limit and the test pins it.
- **Fix with:** root-cause
- **Effort:** M

### XSH-119 · P2 · awk, sed and grep regular expressions follow the host libc

- **Area:** Core commands
- **Where:** `src/modules/regex/posix.rs` (`PosixRegex` over `regcomp`/`regexec`), `core/lib/awk.xsh`, `core/lib/sed.xsh`, `core/tests/test-awk.xsh` (GNU matrix regex shards), `core/tests/test-sed.xsh` (table slices), `core/tests/test-bb-sed-1.xsh` (`sed_OR_not_cfd133ba`), `core/tests/test-grep.xsh`
- **Problem:** awk and sed compile their regular expressions with the host C library, so the same program matches differently on macOS (BSD regex) and on Linux (musl). BSD regex lacks GNU BRE extensions such as `\|` alternation and rejects some repeated quantifiers. On macOS arm64 (m1-max, 2026-11-19) the recorded GNU awk matrix differs in 18 regex and regexp-syntax shards, and sed table slices fail on the same extensions; those cases skip there with this limitation.
- **Recommendation:** Decide whether awk and sed regex are part of XSH's portable contract. If it is, implement POSIX ERE matching in XSH (or a vendored portable engine) behind `PosixRegex` and drop the host dependency; if not, record the host dependency in the CORE awk contract.
- **Acceptance:** Either the awk and sed regex cases pass on macOS and Linux with one engine, or `docs/CORE.md` states that their regex follows the host library and the macOS skips name it.
- **Fix with:** reference-parity
- **Effort:** L

### XSH-120 · P2 · Shared-stream and parallel pipelines stall on macOS

- **Area:** Runtime
- **Where:** `core/lib/awk.xsh` (command `getline` on a shared stream), `core/tests/test-awk.xsh` (getline, streams-and-processes, grammar 9 and lexical-forms 6 matrix shards), `core/tests/test-gnu-factor.xsh` (`factor_parallel_log_1_to_31250`), `core/tests/test-gnu-od.xsh` (`big_w_log`), `core/tests/test-uu-seq-1.xsh` (eight `infinite_sequence` FIFO cases)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19) these tests exceed 160-300 s where Linux finishes in seconds; samples of the stuck processes show them waiting in `poll` with a blocked pipeline, so at least the awk and factor cases look like a lost wakeup or a pipe-capacity deadlock (macOS pipes hold 16 KiB, Linux 64 KiB), not slowness. `od -w` with a large width streams slowly. The tests still fail on macOS.
- **Evidence:** In `test-uu-seq-1.xsh`, a single `unix.poll_fd(fd, ["readable"], timeout_ms: 5000)` on a FIFO with a live writer times out on macOS, while repeated 50 ms polls on the same descriptor see it readable, so the runtime's wait misses a readiness edge on Darwin.
- **Recommendation:** Reproduce the awk shared-stream getline case with a small input, sample it, and check whether a producer blocks on a full pipe the consumer never drains, or a readiness wait misses an edge on macOS. Fix the runtime, then rerun the factor case; measure `od` separately.
- **Acceptance:** The listed tests pass on native macOS within their Linux bounds (or a documented platform budget for `od` only), and still pass on Linux.
- **Fix with:** root-cause
- **Effort:** M

### XSH-121 · P2 · Descriptor identity relies on Linux /proc on macOS

- **Area:** Core
- **Where:** `core/lib/textio_a1.xsh` (`/proc/self/fdinfo` lookup), `core/tests/test-gnu-cat.xsh` (`cat_self_log`), `core/tests/test-gnu-head.xsh` (`head_pos_log`), `core/tests/test-gnu-split.xsh` (`guard_input_log`), `core/tests/test-gnu-wc.xsh` (`wc_files0_log`), `core/tests/test-head.xsh` (elided-stdin rewind), `core/tests/test-uu-cat.xsh` (self-copy)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19) the self-overwrite guards in cat and split, head's repositioning of a shared offset, and wc's redirected `--files0-from` detection all fail because they read descriptor offsets and identity through `/proc/self/fdinfo`, which macOS lacks. Split exits 0 where GNU exits 1, and wc pads nothing.
- **Recommendation:** Derive descriptor identity from `fstat` (`st_dev`, `st_ino`) and the offset from `lseek(fd, 0, SEEK_CUR)` on every host, keeping `/proc` only where Linux needs it for flags `fstat` cannot provide.
- **Acceptance:** The listed tests pass on native macOS and still pass on Linux.
- **Fix with:** os-boundaries
- **Effort:** M

### XSH-122 · P2 · timeout cannot signal its child's group on macOS

- **Area:** Core
- **Where:** `core/timeout.xsh`, `core/tests/test-gnu-timeout.xsh` (`timeout_blocked_log`, `timeout_parameters_log`), `core/tests/test-timeout.xsh` (`timeout_accepts_hex_intervals_and_saturates_large_intervals`), `core/tests/test-uu-timeout.xsh` (`dont_underflow`)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19) these cases print `timeout: failed to send signal: permission denied` and exit 125 where 124 is expected. Instrumenting `core/timeout.xsh` showed the group `TERM` succeeds and the following `CONT` fails with errno 1 (EPERM); foreground mode exits 124. The likely cause, not yet confirmed, is that Darwin returns `EPERM` from `kill(-pgid)` when the group holds a zombie, where Linux still delivers to the live members.
- **Recommendation:** Root-cause which member refuses the signal. If it is a reaped or exiting child, treat `EPERM` on a group whose leader has already exited as delivered, as GNU's Darwin build effectively does; otherwise fix the group setup.
- **Acceptance:** The listed tests pass on native macOS with status 124 and still pass on Linux.
- **Fix with:** root-cause
- **Effort:** S

### XSH-123 · P3 · Terminal and FIFO observation differs on macOS

- **Area:** Core
- **Where:** `core/stty.xsh` (Linux-sized termios decoding), `core/tests/test-stty.xsh` (`all_lists_every_setting_in_gnu_order`, `combination_settings_follow_the_gnu_definitions`, `control_characters_take_every_spelling`, `delay_styles_replace_each_other`, `drain_alone_prints_and_otherwise_chooses_when_to_apply`), `core/tee.xsh` (zero-interest FIFO poll), `core/tests/test-tee.xsh` (`nopipe_mode_does_not_wait_for_idle_input_when_stdout_is_broken`, `nopipe_mode_stops_when_stdout_and_file_fifo_readers_are_gone`)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19) stty decodes termios with Linux field sizes and flag values, so its listings and combinations differ and `cooked` exits 1. tee's `--output-error` modes time out because Darwin keeps reporting a named FIFO writable after its last reader closes, so the zero-interest poll never sees the hangup.
- **Recommendation:** Give stty a per-host termios table (Darwin `tcflag_t` is 64-bit and its flag set differs) and assert host-correct GNU output. For tee, detect reader loss by a zero-length or `EPIPE` probe write instead of poll readiness on Darwin.
- **Acceptance:** The listed tests pass on native macOS with host-correct expectations and still pass on Linux.
- **Fix with:** os-boundaries
- **Effort:** M

### XSH-124 · P3 · Remaining single macOS native failures

- **Area:** Core
- **Where:** `core/tests/test-bb-pidof.xsh` (`pidof_this_48b653be`), `core/tests/test-bb-sed-1.xsh` (`sed_OR_not_cfd133ba`), `core/tests/test-gnu-dd.xsh` (`conv_case_log`), `core/tests/test-grep.xsh` (`differential_basic_selection_and_modes`), `core/tests/test-rmdir.xsh` (`trailing_slash_symlink_diagnostic`), `core/tests/test-sysctl.xsh` (`reads_writes_and_loads_configuration_through_native_keys`), `core/tests/test-uu-cp-5.xsh` (`cp_sparse_always_non_empty`), `dev/tests/test-lifecycle.xsh` (three container lifecycle cases), util-linux `lslocks`/`mountpoint`/`namei`/`syslog-libc` reference comparisons
- **Problem:** On macOS arm64 (m1-max, 2026-11-19), each fails for its own reason. pidof derives `argv0` from `ps` (`src/modules/process.rs`) and misses the child. A sed BRE alternation prints `abca` for `ca`. dd's ISO-8859-1 `conv=lcase` maps only ASCII, so it leaves byte 201 where 233 is expected. grep `-L` lists a matching file. On APFS, `rmdir` of `link/` succeeds where it must fail. sysctl `-p FILE` also reads the absent `/etc/sysctl.conf`. A sparse `always` copy allocates 136 blocks where Linux allocates 16. The lifecycle tests read `/proc/sys/kernel/hostname`. The util-linux references are absent, and the Linux planner rejects those files when they are selected with other cases.
- **Recommendation:** Triage one at a time with root-cause. Split out a ticket for any that turns into a contract question, such as sysctl default loading or the sparse allocation bound.
- **Acceptance:** Each listed case passes on native macOS, or skips with a named platform limitation, and still passes on Linux.
- **Fix with:** root-cause
- **Effort:** M

### XSH-125 · P3 · atomically publishes after a SIGKILL race on macOS

- **Area:** Runtime
- **Where:** `tests/xsh/atomically.xsh` (line 59 scenario), `src/runtime/eval/modules/unix.rs` (self-signal path near line 355)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19) the atomic-publication test fails intermittently: the child exits 137 and `out` is published, although a SIGKILL before commit must leave only the temporary file. A direct self-`SIGKILL` probe behaved correctly, so the cause is signal-delivery timing, not yet located.
- **Recommendation:** Loop the exact test under `xsht test --exact-id` on macOS with a sample of the child at kill time, and check whether Darwin delivers the self-signal after the rename returns.
- **Acceptance:** The test passes 200 consecutive runs on native macOS and still passes on Linux.
- **Fix with:** root-cause
- **Effort:** S

### XSH-126 · P3 · Gate workflow tests assume Linux fixture targets on macOS

- **Area:** Dev tooling
- **Where:** `dev/tests/test-gates.xsh` (`test_gate_cancel_consumes_owned_descendants_before_exit_130`, `test_gate_prepares_each_cargo_harness_once_and_runs_filters_without_cargo`)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19) the fixtures populate `x86_64-unknown-linux-musl` products while the gate under test inspects `aarch64-apple-darwin`, so both tests fail there.
- **Recommendation:** Build the fixture tree for the context's own target, or pin these tests to a Linux context the way `fixtures.linux_context` is used elsewhere.
- **Acceptance:** Both tests pass on native macOS and Linux.
- **Fix with:** root-cause
- **Effort:** S

### XSH-127 · P2 · FsRoot cannot list a directory without search permission on macOS

- **Area:** Runtime
- **Where:** `crates/xsh-root/src/mutate.rs` (`read_dir`), `crates/xsh-root/src/macos.rs` (`open_readable_dir`), `core/du.xsh`; tests `core/tests/test-du.xsh` (`reports_children_blocked_by_directory_search_permissions`), `core/tests/test-gnu-du.xsh` (`inacc_dest_log`, `no_x_log`), `core/tests/test-uu-du-1.xsh` (`du_inaccessible_directory`, `du_no_exec_permission`)
- **Problem:** On macOS arm64 (m1-max, 2026-11-19), `fs.open_root` of a readable but unsearchable directory (mode 0600) succeeds, but `children(".")` reports `enumeration_succeeded: false` with errno 20. `read_dir` hands the descriptor to rustix `Dir::read_from`, which reopens it with `openat(fd, ".")`; that lookup needs search permission, which the directory denies. du therefore prints `cannot read directory 'd/no-x': Not a directory` where GNU lists the names and reports `cannot access 'd/no-x/y': Permission denied`.
- **Recommendation:** Enumerate from an independent open file description without a path lookup inside the directory: on macOS, reopen the directory from its parent (the root already knows it) or use `fdopendir` on a fresh descriptor and `rewinddir` before reading, so the root's own offset is never shared. Keep Linux unchanged.
- **Acceptance:** The listed du tests pass on native macOS, a new xsh-root test lists a mode-0600 directory on both platforms, and Linux results are unchanged.
- **Fix with:** os-boundaries
- **Effort:** M

### XSH-128 · P3 · Second set of single macOS native failures

- **Area:** Core
- **Where:** each item names its test under `core/tests/` unless shown
- **Problem:** After the three qualification lanes (m1-max, 2026-11-19), these still fail on native macOS with causes not yet established:
  - `test-chown.xsh`/`test-chgrp.xsh` verbose permission failures print a different stdout line.
  - `test-rm.xsh` (`recursive_removes_hierarchy_deeper_than_path_max`) and `test-gnu-rm.xsh` (`deep_2_log`) fail on Darwin `ENAMETOOLONG` (63).
  - `test-dd.xsh` (`oflag_append_on_standard_output`: Permission denied) and `test-gnu-dd.xsh` (`sparse_log`) fail with `oflag=append`.
  - `test-curl.xsh` (`plain_http_needs_no_trust_store`) exits 77 loading trust anchors.
  - `tests/xsh/run.xsh` has four failures: inherited run and pipeline return timing (2.8 s versus 0.15 s), `Operation not permitted` in the foundation literals test, and run-target failure kinds.
  - `tests/xsh/stdlib/unix.xsh` has two process-group and session-leader failures.
  - `dev/tests/test-gates.xsh` fails in changed-path discovery and in source changes during execution (see also XSH-126).
  - `test-uu-comm.xsh` (`locale_collation`) and `test-uu-sort-2.xsh` (`merge_batch_size_with_limit`) fail.
  - `test-uu-cksum-1.xsh` (`debug_flag`), `test-uu-stat.xsh` (`printf_atime_ctime_mtime_precision` on `/dev/ptmx`) and `test-uu-ls-2.xsh` (`ls_order_time_breaks_ties_by_name`) fail.
  - `test-uu-du-2.xsh` (`du_time`: `--time=birth`) fails.
  - `test-tail.xsh` (`follows_standard_input_redirected_from_a_regular_file`) and `test-uu-tail-1.xsh` (`follow_stdout_pipe_close`) time out.
  - `test-uu-tee.xsh` (`tee_continues_after_short_read`, `linux_only_pipe_mode_broken_pipe_only`) fails.
  - `test-lslocks.xsh` (`exited_flock_acquirer`, `filter_non_utf8_outside_literal`) fails.
- **Recommendation:** Triage each with root-cause on m1-max. Split any that becomes a contract question into its own ticket, and fold descriptor-identity and FIFO-wakeup causes into XSH-121 and XSH-120 when they match.
- **Acceptance:** Each case passes on native macOS or skips with a named platform limitation, and still passes on Linux.
- **Fix with:** root-cause
- **Effort:** L
