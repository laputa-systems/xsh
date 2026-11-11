# TODO

Prefix: XSH
Next: 88

Apply the [design principles](docs/DESIGN.md) to each slice: prefer reusable
host boundaries and ordinary XSH policy over command-specific runtime growth.
[Laputa's queue](https://github.com/laputa-systems/laputa/blob/master/TODO.md)
owns distribution consumers; its [roadmap](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md)
owns VM milestones. Cross-repository IDs below name prerequisites, not permission
to edit another checkout.

The systems-foundation items were reviewed against `9d893e0075d2`; findings
marked unproven need a reproducer, not an assumed defect. Existing acceptance
items remain open. The performance/deployment and Lean campaigns retain their
own pause/launch rules; this queue update resumes neither. The Deser
codec cutover was functionally qualified on an earlier Linux source; current-source
qualification is active. Native macOS qualification
is deferred by the owner, with no macOS host available. The [G0 release candidate](docs/ROADMAP.md#g0-release-candidate) decides
which items block a release; performance and resource items such as XSH-1 and
XSH-3 are deferred for G0. Do not make an unrelated full baseline an entry fee
for a bounded fix; use the narrow applicable gate. Real-hardware enablement is
out of scope.

### XSH-1 · P2 · Complete private and resource baseline acceptance

- **Deferred:** Runs with XSH-3 after the rest of the backlog; see XSH-3.
- **Area:** Performance
- **Where:** `dev/util-linux/resources.py:1632`, `dev/util-linux/acceptance.py:926`
- **Problem:** Final private/resource qualification remains open on current source. The historical scheduler comparison failed, so composed baseline close cannot proceed.
- **Recommendation:** After scheduler qualification, run the private and resource gates on the same source. Complete baseline close with its one-use independent review and readiness checks.
- **Fix with:** benchmark-checklist
- **Effort:** S

### XSH-35 · P2 · Qualify clock domains and deadline conversions

- **Area:** Runtime contracts
- **Where:** `src/modules/time.rs`, `src/runtime/eval/`, `crates/xsh-registry/`, `docs/templates/SPEC.md`
- **Problem:** Wall time is epoch milliseconds, `time.monotonic()` is nanoseconds from a process-local origin, and `time.boottime()` uses nanoseconds from a host clock. Linux boottime includes suspend; macOS uses its monotonic clock. All three use integers. Laputa xinit now converts monotonic nanoseconds to milliseconds for elapsed deadlines; other consumers and conversion boundaries remain unqualified.
- **Progress:** Pinned Linux host tests bracket wall, process-monotonic and boot-clock readings against their owning clocks. DHCP one-shot deadlines saturate without overflowing and reject unrepresentable CLI seconds before network setup; focused tests cover cancellation, zero budgets and existing wall/boot-clock jumps. The Linux inventory of `core/`, `core/lib/` and the Rust wait paths is complete. Rust waits (`Instant`, `poll`, process and netlink deadlines) are monotonic and take Durations of at most `u64` milliseconds, which cannot overflow `Instant` on 64-bit Linux; a `time.sleep` of the largest millisecond Duration is cancelled by SIGTERM rather than panicking. `syslogd` and DHCP use the boot clock deliberately. `lib.monotonic` gives `Reading` and `Deadline` nominal types so a wall value cannot meet a monotonic deadline, with tests for zero, negative, expired and saturating budgets; `udevadm`, `nc`, `traceroute`, `ping`, `curl`, `wget`, `dd` and `mdev` now use it. `iw` read wall milliseconds as nanoseconds and is fixed. `ping` keeps every send, drain and run deadline as a `Deadline` and no longer adds raw `Reading.ms` integers. `ping` and `traceroute` keep the kernel `SO_TIMESTAMPNS` stamps for microsecond round trips but bound each with the gap between two `lib.monotonic` `Stamp`s taken around the same send and receive (`round_trip_ns`): a wall difference that is negative or longer than that gap proves a step and the monotonic gap answers instead, so a step can no longer distort an RTT or the summary time. Pure monotonic RTTs were rejected: script latency made loopback RTTs 0.5 to 0.9 ms against 0.12 ms, and `SO_TIMESTAMPING` has no monotonic source. A step smaller than the script latency, or a backward step that leaves the difference positive, still passes through, bounded by the monotonic gap. `ping -D` still prints the kernel wall stamp.
- **Remainder:** VM pause and physical suspend need a disposable guest and host (LAP-27) and stay unqualified. Native tests cannot step the wall clock inside a consumer, so wall-jump immunity of the migrated commands rests on the types, not an injected clock. macOS is owner-deferred. Keep wall timestamps for calendar and security decisions; never persist a process-local instant as a restart-stable timestamp.
- **Acceptance:** Native tests separate wall and monotonic inputs, cover backward/forward wall jumps, zero/expired budgets, overflow and cancellation; host tests establish the units/origin. Qualify VM pause separately from physical suspend. Qualify xinit's conversion and any further consumer migration with unit-safe conversions; update authored docs before regeneration; no host clock changes.
- **Coordination:** Laputa LAP-27 owns xinit migration and disposable-guest clock tests.
- **Fix with:** make-invalid-states-unrepresentable
- **Effort:** M

### XSH-36 · P2 · Qualify descriptor and process ownership under failure

- **Area:** Resources
- **Where:** `src/modules/`, `src/runtime/process.rs`, `tests/xsh/process-failure-ownership.xsh`, `tests/xsh/unix-spawn-ownership.xsh`
- **Problem:** Linux qualification found no defect: repeated failed spawns, early consumer exit, partial input and output, and descriptor inheritance leave no owned children or leaked descriptors, and a released or stale handle cannot signal a recycled pid. macOS has no qualification, and no backend limitation is recorded for it.
- **Recommendation:** Run the same cases on macOS, where `/proc` is absent, so the child and descriptor inventories need another source. Record any backend that cannot enforce a contract instead of treating a no-op as enforcement.
- **Acceptance:** The cases in `tests/xsh/process-failure-ownership.xsh` pass on native macOS. Native macOS qualification is deferred by the owner.
- **Coordination:** Laputa LAP-34 exercises the service lifecycle; retain the host-process/bounded-stream concurrency model, not a new async runtime.
- **Fix with:** lifecycle-concurrency
- **Effort:** S

### XSH-37 · P2 · Qualify confined filesystem operations for privileged consumers

- **Area:** Host authority
- **Where:** `crates/xsh-root/`, `crates/xsh-archive/`, `src/modules/fs.rs`, `src/modules/archive/`, `tests/`
- **Problem:** `xsh-root` confines pathname resolution, but lexical Path validation and an `fs` effect are not filesystem authorization. Archive extraction (`archive.extract` and `FsRoot.extract`, one engine), `patch.apply`, `FsRoot.rename`/`fsync` and `fs.root_install_file` acquire one anchor and resolve beneath it; Path-taking extraction and publication acquire that anchor from the path. Rooted file reads in archive creation, patch and `FsRoot.fsync` open non-blocking and refuse a non-regular file, but `FsRoot.read` of a FIFO still blocks by design.
- **Recommendation:** Add `FsRoot` hard-link and special-file creation only if Laputa LAP-33 shows a need.
- **Status:** The Linux acceptance is met. `crates/xsh-archive/tests/interrupted.rs` (`test-archive-interrupted`) SIGKILLs a child mid-extraction (path and retained-root destinations, and over an existing destination), mid-creation, and during repeated publication. `tests/runtime/root_install.rs` SIGKILLs a script mid-`fs.root_install_file`, fresh and replacing. In every case staging is left behind and no partial destination or replaced file becomes visible. `rooted.rs` and `publication.rs` cover ancestor and symlink replacement of a retained root. LAP-33 migration gaps landed: `FsRoot.mkdir(parents: true)` accepts an existing directory; `fs.root_install_file_sha256` returns the SHA-256 of the bytes it copied; `FsRoot.tempdir(prefix)` creates an owned private directory removed through the parent root's descriptor, so it survives the parent tree being moved; `FsRoot.syncfs()` flushes the containing filesystem (Linux only). `FsRoot.children` keeps returning root-relative paths, which its contract and the system-report callers rely on; only the documentation was sharpened. Only the owner-deferred macOS work and the items conditional on Laputa LAP-33 (`FsRoot` hard-link and special-file creation) remain; the ticket stays open for those.
- **Acceptance:** Adversarial fixtures cover extraction into a retained root under ancestor and symlink replacement, and interrupted publication by process kill. Power-loss durability stays unclaimed; atomic visibility and durability stay tested as separate contracts. macOS enforcement is owner-deferred and must fail explicitly until qualified.
- **Coordination:** Laputa LAP-33 owns package adoption; LAP-28 owns build-process isolation; XSH-47 owns any process sandbox claim.
- **Fix with:** isolate-effects
- **Effort:** S

### XSH-43 · P2 · Stress process and stream lifetimes with signal storms

- **Area:** Runtime verification
- **Where:** `tests/runtime/signal_storm.rs`, `crates/xshi/tests/cli.rs`, `crates/xshi/src/interactive/term.rs`, `docs/TESTING.md`
- **Problem:** The Linux seed-replayable storm (`runtime::signal_storm`) found no orphan, double cleanup or unbounded cancellation. Its script scenarios still do not cover SIGKILL of the runner, interruption inside spawn setup, or macOS. A second signal that lands after cleanup but before exit kills the runner with the default action, which the oracle accepts.
- **Recommendation:** Add the macOS run when a host exists; the terminal-restoration PTY session is covered on Linux.
- **Acceptance:** The storm passes on native macOS and checks terminal restoration. Native macOS qualification is deferred by the owner.
- **Progress:** The interactive PTY storm (`crates/xshi/tests/cli.rs`, module `terminal_storm`) starts the real `xshi` on its controlling PTY, waits for raw mode, and delivers a seeded SIGINT (escalated to SIGTERM), SIGTERM, or SIGHUP. It checks bounded exit, ICANON/ECHO/ISIG/IXON/ICRNL/OPOST and VMIN/VTIME restoration, and no tagged stragglers. It found that `xshi` had no SIGTERM/SIGHUP handler, so a signal during raw mode left the terminal raw; `crates/xshi/src/interactive/term.rs` now installs a one-shot handler for each of the two signals that is still at its default action, restores the saved attributes and re-raises the signal; an inherited ignored SIGHUP stays ignored, which `an_inherited_ignored_hangup_stays_ignored_in_raw_mode` pins. Linux x86-64 musl. Closing the master to raise SIGHUP is not observable afterwards (the slave answers `tcgetattr` with `EIO`), so SIGHUP is delivered directly. SIGKILL cannot restore a terminal and is out of scope; macOS stays owner-deferred.
- **Coordination:** XSH-36 owns the resource contracts; Laputa LAP-36 supplies the xinit/PID 1 consumer, and LAP-34 owns its first hermetic lifecycle replay.
- **Fix with:** lifecycle-concurrency
- **Effort:** S

### XSH-55 · P2 · Qualify the native SSH client

- **Area:** Remote execution
- **Where:** `crates/xsh-ssh`, `src/modules/ssh.rs`, `docs/rfc/XSH-55-native-ssh-client.md`
- **Problem:** The native client is experimental on Linux x86-64. Four internal protocol and security passes (2026-11-05 to 2026-11-11) fixed eighteen defects, most recently channel traffic before the `exec` reply and connection service while a caller's writer stalls; the `xsh-ssh` suite and the native `tests/xsh/ssh-` suite, including a SIGTERM cancellation test, pass on x86-64 musl against OpenSSH 10.0. Open: native macOS qualification, an independent protocol and security review, the build-time gate (host busy), and the RFC review section's remaining risks (unwiped SHA-256 contexts, fixable only in the vendored Graviola, and a writer that never returns still keeping `exec_stream` from returning).
- **Recommendation:** Complete the remaining qualification in the RFC and obtain the independent review. Preserve mandatory host verification and typed execution certainty.
- **Acceptance:** The RFC's required tests pass against disposable OpenSSH servers, with ssh-build's OpenSSH client as oracle, on the executed Linux musl and macOS arm64 lanes; the client is labeled experimental. Native macOS qualification is deferred by the owner.
- **Coordination:** Laputa R6 and XSH-56 consume the client.
- **Fix with:** security
- **Effort:** L

### XSH-56 · P2 · Complete xsh-infra qualification

- **Area:** Remote execution consumer
- **Where:** `laputa-systems/xsh-infra`, `docs/rfc/XSH-56-xsh-infra.md`
- **Problem:** xsh-infra implements remote reconciliation over native SSH. Its Alpine container scenarios and the pyinfra 3.5 differential now cover every resource kind, packages, services, commands and failures, and agree with pyinfra apart from two recorded deviations. Two-process cancellation is done (xsh-infra `7cd069a`): a SIGTERM or SIGINT exits 3 and reports the resource in flight as unknown and every resource not yet run as blocked, and 11 Alpine scenarios hold the before, after and in-flight boundaries with target-side gates and prove the rerun converges. Single-request kinds have no holdable interior. The file, system, command, failure, process and cancellation suites also pass on a pinned Alpine 3.22.8 KVM guest with OpenRC as PID 1 (xsh-infra `df05b26`). xsh-infra `e293bfd` (vendored as Laputa `be2fbe3`) adds an `xsh` inventory dialect for Laputa, whose login shell compiles commands as XSH, and an xinit service adapter: against a qemu-maintained KVM guest over native SSH, plan, apply (a managed file and a `logger` restart, pid 84 to 277, ready), an unchanged rerun and `check` all converge, and declaring `enabled` is refused because xinit has no enable. The 180-case suite passes against Alpine containers. Laputa package, account and OpenRC resources are refused under the `xsh` dialect; macOS is owner-deferred. Laputa is replacing its older in-tree xinfra with this implementation.
- **Recommendation:** Add the Laputa/xinit service adapter once Laputa ships xsh-infra.
- **Acceptance:** The RFC's per-resource and integration scenarios pass against disposable Alpine SSH hosts, with pyinfra as a differential reference on equivalent hosts; XSH defects found get minimal reproducers in XSH.
- **Coordination:** Remote execution uses the experimental XSH-55 client.
- **Fix with:** isolate-effects
- **Effort:** L

### XSH-3 · P3 · Finish the deferred consolidation acceptance matrix

- **Deferred:** The owner put product work first and defers timed performance gates. Exclude this ticket and XSH-1 from the active backlog run until the rest of the backlog is done.
- **Area:** Tests
- **Where:** `dev/consolidation/final_canonical_replay.py`, `dev/performance/check.py`, `dev/consolidation/winddown/gate-tools/`
- **Problem:** On `e8c526960`, every stage then in the matrix passed: supplemental, bootstrap, canonical replay, route replay (422 of 422 steps), privileged and SSH skips, functional close, profile cost, observers, forced frames and the comparative profile. The three scheduler pairs, added afterwards, failed on every later commit, never for a timing regression: a fork stall from inherited ignored SIGQUIT (`a59cc658f`, product follow-up XSH-79), a read-only gate record directory (`45dc38d69`), a split memory probe with one page of headroom (`77cdb05a1`), a pr overflow check that walked every byte (`21fd840bc`), and a pr merge header that read the clock per page (`29995f89d`). Round 1 reference passed 15,268 of 15,268 on both `d956f9f8d` and `45dc38d69`. A chain takes about nine hours and runs the native suite about nine times, and any one failure voids it.
- **Recommendation:** When resumed, gate correctness once, on the canonical replay, and treat the scheduler pairs, observers and forced frames as timing measurements that report and ticket test failures. Reuse a stage's receipt across commits whose diff touches none of its inputs. Branch `fix/gate-correctness-once` holds that change, unverified against a full chain. Before any timed run, sweep the native suite four times concurrently to surface load-sensitive failures at once. Bootstrap and plan as before: `final_canonical_replay.py bootstrap --repo SOURCE --head COMMIT --tree TREE --output .work/tmp/bootstrap-RUN --run-name bootstrap-RUN`, then `execute --bootstrap-receipt .work/tmp/bootstrap-RUN/bootstrap-receipt.json`, validating each tool's `--plan` first.
- **Unmet:** (1) The full matrix, including the three scheduler pairs, on one current commit. (2) util-linux `resources --check` and `close --check` (XSH-1). (3) Publishing the accepted baseline and retiring the infrastructure under XSH-19.
- **Fix with:** orchestrate
- **Effort:** M

### XSH-80 · P3 · Make a btop frame for 1,000 processes interactive

- **Area:** Performance
- **Where:** `core/lib/btop_model.xsh`, the btop render and diff path, `dev/perf/btop-profile.xsh`
- **Problem:** The release `btop-profile` run on `32aad8b7a` (2026-11-07) put the p95 time for one frame (model, render and diff) at 2.08 s with 1,000 processes. btop refreshes about once a second, so every frame at that size overruns its interval. The profile asserts only that the process join scales linearly (2.02 times from 1k to 2k processes) and sets no frame budget. No earlier figure exists to compare against.
- **Recommendation:** Profile one 1k-process frame, find which of model, render or diff dominates, and fix that stage. Then add a frame-time budget to `btop-profile.xsh`.
- **Progress:** `df06e7ce6`..`99912ff48` compute per-frame inputs once, derive both process rates in one call and sort and filter once per frame. Model wire JSON and every sorted screen stay byte-identical. Unqualified p95 for 1,000 processes fell from 4.3 to 10.6 s to 0.43 to 0.84 s on a loaded host; the remaining cost is evaluator records, field reads and calls (XSH-77). `btop-profile.xsh` now enforces p95 at most 1.25 s, to tighten to 250 ms after a qualified release run.
- **Acceptance:** p95 frame time for 1,000 processes under 250 ms in a readiness-qualified release run, with the profile enforcing that budget; btop tests green.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-77 · P3 · Make module constants and record fields cheap to read

- **Area:** Performance
- **Where:** `src/runtime/eval/`, core commands written in XSH (`core/factor.xsh` and others)
- **Problem:** Profiling `factor` on 2026-11-07 measured a function call at about 3 µs, and a record field read, list index or module `const` read at about 0.7 to 1 µs, against about 0.05 µs for arithmetic on locals. `perf` put 36% of the time in `memcpy` in the evaluator. Replacing a module `const` with a literal cut one `factor` kernel from 29 µs to 10 µs. The `factor` fix (`64788888b`) works around this by keeping limbs in `Int` locals; every other core command written in XSH pays the same cost.
- **Recommendation:** Profile const, field and index reads on a small kernel, find where the copy happens, and make those reads borrow instead of clone. Re-measure `factor`, `test_uu_factor_parallel` and the `test_gnu_factor_*` cases (about 55 s each at `-j 8` against a 120 s budget).
- **Progress:** Field and index reads already borrowed their base; the copies were frame-machine moves. `FrameContinuation` (336 bytes) and `FrameWork` (400) moved by value on every pushed expression, and `eval_indexed_expr` copied two 544-byte `Result<_, RuntimeError>` slots into a tuple. `b79683f37` boxes the large continuation payloads (152 and 216 bytes now) and `58eb404cd` drops the tuple; memcpy in a field-read loop fell from 35% to 18% of samples. `8efacac78` lowers Int and Bool module constants onto the typed scalar path, so `acc + K` costs what `acc + 7` costs. Unqualified user cycles per `acc = acc + X` statement on a shared host (minimum of four interleaved runs, `e10770744` to `58eb404cd`): local 3,174 to 2,326, const 4,982 to 2,314, field 5,230 to 3,394, list index 5,760 to 3,890. `factor` on 6,250 six-digit numbers fell from 13.6 to 10.3 G user cycles and on 12 numbers in [2^63, 2^64) from 2.15 to 1.73 G, with identical output.
- **Unmet:** `a3359bcb1` adds `IntFieldSlot`, which puts an Int field of a record slot on the typed integer path when the record type has no dynamic member; in the same unqualified measurement a field-read statement costs 1.08 to 1.11 times a local-read statement (local 2,271, field 2,461 minimum user cycles), and `acc + info.m` runs 7,365 instructions against 6,910 for `acc + m`. `3ca95352c` adds `IntListIndexSlot` for a checked Int index into a `List[Int]` local; in an unqualified interleaved run on a shared host the `acc = acc + xs[i]` statement fell from 3,778 to 2,401 minimum user cycles against 2,274 for a local read, and `factor` output stayed byte-identical. The qualified CPU-time comparison waits for the deferred performance gates.
- **Acceptance:** A module `const` read and a record field read cost within 2x of a local read on the same kernel, in readiness-qualified CPU time; native suite green.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-14 · P3 · Reuse import-graph checks across lint candidates

- **Area:** Performance
- **Where:** `crates/xsht/src/lint.rs`, `crates/xsht/src/cli`
- **Problem:** Each lint candidate probe parses, loads, and checks the unchanged import graph again. Large fix runs repeat the same work for every candidate.
- **Recommendation:** Measure shared checking, grouped candidates, and clean-corpus probe skipping. Keep the smallest design that preserves diagnostics and fix safety.
- **Unmet:** The batched-probe candidate (`97e361171`, merge `6300eb05b`) was reverted on 2026-11-01: guarded CPU timing put lint at 0.98 to 1.00 of the baseline on three workloads, inside the spread, for about 700 lines and 86,080 bytes of release `xsht`.
- **Progress:** 2026-11-09 attribution on the current tree (release `xsht lint core`, 933 files, thread CPU around every `LintProbeTransport` call): 829 probes cost 20.6 s of about 75 s user. 797 are `lint_try_capture`'s unconditional re-check of each unchanged file (16.2 s, 21 percent), run even when the file has no helper candidate; 32 are `lint_identical_match_arms` standalone candidate checks (4.7 s, 6 percent, about 150 ms each). `xsht lint --fix core` on a clean copy adds only 4 loaded probes (milliseconds), so import-graph reuse across candidates has no measured workload to pay for it: core is fix-clean, and the checker (module check rounds) dominates the remainder. The try-capture baseline check is now lazy (lint stage thread time 32.0 s to 17.1 s, user CPU 0.77 of baseline over three interleaved runs on a loaded host, diagnostics byte-identical). Remaining: only a corpus that actually triggers many loaded fix probes would justify shared import checking; none is known.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-19 · P3 · Retire consolidation-only gate infrastructure after acceptance

- **Area:** Build
- **Where:** `dev/consolidation/winddown/gate-tools/`, `dev/performance/check.py`, `dev/util-linux/acceptance.py`
- **Problem:** Baseline qualification currently depends on campaign-specific controllers, receipt adapters and historical pins. Leaving those dependencies in the everyday gate path makes subsequent changes require campaign history. The accepted baseline is still pending, so deletion must follow G1.
- **Recommendation:** Execute G5 immediately after G1. Trace current callers, move reusable checks behind ordinary developer commands, and delete obsolete adapters, duplicate entry points and historical-pin dependencies. Preserve required gates and their rejection controls. Update testing and baseline docs. Keep historical evidence outside the everyday command path.
- **Fix with:** dead-code-archaeology
- **Effort:** M

### XSH-38 · P3 · Establish one typed virtual-network library cohort

- **Area:** Systems libraries
- **Where:** `core/ip.xsh`, `core/ifup.xsh`, `core/lib/`, `src/modules/`, `crates/xsh-registry/`
- **Problem:** Interface/route observations and low-level network operations exist, but a shared typed observe/change surface for stateful VM administration is not qualified. More command spellings alone would not close that boundary.
- **Recommendation:** Start with links, IPv4/IPv6 addresses and routes on virtual Ethernet. Put policy and command rendering in shared XSH libraries; Rust owns narrow host operations and decoding. Make malformed/truncated replies, interrupted dumps and lost notifications visible so callers can re-observe. Add bridge/veth/namespace operations only when the next concrete fixture requires them; no Wi-Fi or replacement DHCP stack.
- **Acceptance:** A command and Laputa's controller use the same records/operations without rendering and reparsing text. Fixtures exercise malformed input and event loss; disposable Linux guests exercise actual mutations and convergence. Report unsupported families/backends separately; do not invent empty successful observations.
- **Coordination:** Laputa LAP-32 owns managed-resource identity, planning and reconciliation.
- **Progress:** `core/lib/net_state.xsh` holds the typed cohort: `Link`, `Address` (with `peer` for point-to-point), `Hop` and `Route` (with `other_attributes`, the unmodeled attribute kinds) under `Family` `inet`/`inet6`; `project(dump)` and `observe()` return `State` or a per-dump `ObserveFailure` (`Unsupported`, `Denied`, `Malformed`, `Truncated`, `Interrupted`, `Limited`, `Failed`), never an empty state, and count unmodeled families in `State.unmodeled`. `Change` (`LinkUp`, `LinkDown`, `AddAddress`, `RemoveAddress`, `AddRoute`, `RemoveRoute`) encodes through the pure `request` and applies through `linux.netlink_route_request` as `apply`, failing `Refused(errno)`, `Unencodable` (multipath, unmodeled route attributes, bad text) or `Transport`. `subscribe` and `drain` report `Quiet`, `Changed` or `Lost` on ENOBUFS or an undecodable datagram. `ip` renders `addr`, `link` and `route` from `State` (byte-identical to the former `linux.interfaces`/`linux.routes` rendering, which are deleted along with their Rust projections after Laputa's installer and guests moved to `net_state`; the live-host route and link agreement checks moved to `net_state`) and gains `link set`, `addr add/del` and `route add/del`, which select removals from the observation. Pure fixtures cover every dump-failure state, rule/counter issues that do not block, an issue-free incomplete dump, nameless links, unmodeled families and attributes, request bytes (address bytes equal Laputa's encoder) and truncated or overrun notification datagrams. Namespace cases (`lane/namespace-native`) converge IPv4/IPv6 addresses and routes on a veth pair with a foreign address surviving, detect a real ENOBUFS overrun, and run the `ip` change transcript. Polling a netlink socket after an overrun reports readable and error together, which the DHCP client's event drain treated as idle; it now drains through `net_state.drain`. Remaining: Laputa's `network_kernel.xsh` still projects and encodes itself (adapter: plan over `net_state.State` and emit `net_state.Change`), a disposable-guest run of that controller. Bridge, namespace and multipath changes stay out until a fixture needs them.
- **Fix with:** single-source-of-truth
- **Effort:** L

### XSH-39 · P3 · Qualify bounded Unix-socket I/O for VM control

- **Area:** Host I/O
- **Where:** `src/modules/`, `crates/xsh-registry/`, `core/lib/`, `tests/`
- **Problem:** `unix.connect_stream`, descriptor I/O and polling support Laputa's XSH QMP client, which owns and closes its socket and uses a monotonic deadline. Linux host tests cover short-write backpressure and retries, scoped closure while the interpreter remains alive, shutdown during a bounded poll, and a native line-protocol fixture on `lib.monotonic` deadlines: byte-fragmented and coalesced frames, EOF, a stalled peer, oversized unterminated and terminated frames, abrupt close mid-frame with a stable descriptor count, and SIGTERM mid-frame with cleanup and peer-visible closure. `unix.poll_fd` and `unix.poll_fds` both end at a handled signal, so Laputa's QMP client cancels promptly. macOS and Laputa LAP-35 corroboration remain unqualified.
- **Recommendation:** Keep framing and QMP policy in the XSH consumer.
- **Acceptance:** The same properties hold on the macOS host path, and Laputa LAP-35 demonstrates the same primitives against a real QEMU monitor; no futures runtime or callbacks are required.
- **Fix with:** isolate-effects
- **Effort:** S

### XSH-40 · P3 · Reduce one system failure to a hermetic native replay

- **Area:** Tests
- **Where:** `crates/xsht/`, `src/trace/`, `tests/`
- **Problem:** Laputa xinit has a native fake replay of a crashed leader with an owned descendant, corroborated by a disposable Linux x86-64 guest and a deliberately broken cleanup. `test.unix_fake` now takes a `replay` observation script (SPEC section 17): ordered lines, per-call required results with no defaults, field checks, a redacted `[A-Za-z0-9_.,-]` 32-byte value alphabet, rejection of a reordered, mismatched, missing-result, past-the-end or never-consumed call, and a `unix-fake-replay` error instead of the host for the uncovered `unix` entries. Native tests in `tests/xsh/stdlib/unix.xsh` reproduce the crashed-leader sequence offline and fail when the cleanup skips the leader group. Laputa `d5aa064` moved its xinit crashed-leader test onto the replay (`xinit/tests/fixtures/crash-replay.txt`), including the mutation that skips the leader group. The KVM self-hosting guest fixture `lifecycle_replay_guest.xsh` exposes the same invariant, but it has not been rerun against current xinit.
- **Recommendation:** Qualify the existing LAP-34 counterexample against an explicit ordered, complete and redacted input contract. Add a minimal reusable test helper only if existing fixtures cannot express the case; no generic replay platform or campaign controller.
- **Acceptance:** The reduced case reproduces offline, fails when the repaired invariant is deliberately broken, and rejects missing/mismatched observations rather than consulting the host. Test fixtures contain no secrets. Corroborate with the real guest; mock agreement does not establish OS conformance or power-loss durability.
- **Fix with:** root-cause
- **Effort:** M

### XSH-44 · P3 · Connect one Lean semantic scenario to actual XSH execution

- **Area:** Formal conformance
- **Where:** `xsh-lean.md` (G0b/G1b/G9 and the connected milestone), `formal/README.md`, `formal/Xsh/Conformance/`, `src/runtime/eval/indexed/full/`, native tests
- **Problem:** Lean models and finite bridge fixtures exist, but `formal/coverage.json` still has no source-identified Rust conformance cases. A theorem about the Lean model is not correspondence with the compiler or host.
- **Recommendation:** Only when the owner explicitly resumes the paused Lean campaign and its source-freeze/G1 prerequisites hold, implement its prescribed test-only source/IR bridge and the first stage of the connected milestone, with an independently specified actual-source witness (prefer ordered argv or Result propagation). Shape the case so a second implementation can submit its candidate facts and IR to the same checks. Replay the same bounded observation through the verified XSH execution path, record precise source/IR identity and expected observations, and minimize a mismatching case. Reuse G1b/G9; no new Lean campaign or product-visible exporter.
- **Acceptance:** One traceable ledger entry compares independent model expectations to actual Rust/XSH outcomes; altered source bindings, forged IR, omitted events and intentionally mutated behavior are rejected. The entry states its starting boundary (source bytes or supplied syntax). Record the limits of the finite witness, and make no full-language or Rust/kernel soundness claim. This TODO does not resume Lean work.
- **Fix with:** reference-parity
- **Effort:** M

### XSH-46 · P3 · Pilot opt-in static signatures for external command flags

- **Area:** Tooling
- **Where:** `crates/xsht/`, `crates/xsh-registry/`, `docs/XSHT.md`, native CLI contracts and tooling tests
- **Problem:** XSH checks native and XSH-defined CLI contracts but does not statically validate arbitrary `run` targets' flags. An optional trusted signature could catch misspellings without constraining ordinary Unix interoperability.
- **Recommendation:** Start with one statically named XSH core applet whose CLI signature already comes from the canonical registry; optionally test one pinned external tool. Have `xsht` diagnose known incompatible flags/argument shapes, warn on incomplete or stale signatures, and skip dynamic command heads. Never execute tools during checking or add a mandatory external-signature database. A tool's CLI schema cannot prove its effects or authority.
- **Acceptance:** Valid invocations and unchecked dynamic commands retain their behavior; reviewed complete contracts reject known-invalid flags, partial contracts never cause false hard failures, signatures stay version-associated, and no filesystem/network/process execution is triggered by checking. Benchmark checking overhead and drop the experiment if maintenance outweighs benefit.
- **Coordination:** Laputa LAP-40 pilots a real recipe only if the opt-in tool contract proves useful.
- **Status:** Parked. A pilot on branch `feat/xsh-46-command-signatures` (`5d1dd5e32`) lints literal `run NAME` flags against `[command-signature.NAME]` sections in `xsht-config.ini`, piloted on GNU `sha256sum`, at about 15 µs per checked call. No Laputa recipe runs a statically named external tool with literal flags, so it has no consumer; the tools LAP-40 would need (`git`, `tar`) also need subcommand and positional shapes the pilot does not model. Resume when LAP-40 names a real recipe.
- **Fix with:** make-invalid-states-unrepresentable
- **Effort:** M

### XSH-47 · P3 · Qualify one OS-enforced subprocess confinement boundary

- **Area:** Host authority
- **Where:** `src/runtime/process/`, `crates/xsh-root/`, `src/modules/`, `tests/`, `docs/SPEC.md`
- **Problem:** Effects and `without` statically constrain XSH calls; `FsRoot` confines selected pathname operations. Neither enforces authority over an external child: `without net` is not a kernel network sandbox, and no general confinement guarantee follows from its type.
- **Recommendation:** Use Laputa LAP-28's concrete closed-build cohort to first audit available kernel primitives and prove one optional constrained child execution path, reusing namespaces, Landlock, seccomp or existing container isolation only where each actually enforces the requested property. Keep static effects and OS authority distinct; prefer existing execution APIs and the smallest needed host primitive. No universal capability algebra, mandatory sandbox, or cross-platform pretend-success.
- **Acceptance:** Declared inputs/writes remain usable; forbidden file access, process/network operations and inherited-descriptor escapes fail under the actually supported profile. Tests prove enforcement inside a disposable Linux environment and reject unavailable kernels/features before execution. Document remaining privilege/host/child escapes and measure startup and dependency cost.
- **Status:** One profile landed: `linux.run_confined(command, read:, write:, tcp: false)` (Landlock ABI 6 or newer plus `no_new_privs`, built in the parent and entered in the forked child before exec; descriptors above standard error are marked close-on-exec). An older kernel, disabled Landlock or an unopenable declared path fails with `ProcessError.Spawn` before any spawn. `tests/xsh/stdlib/linux_confined.xsh` runs on the host kernel in throwaway temp directories and shows declared reads and writes work while undeclared reads, writes, execution, signals to outside processes, TCP connect and an inherited descriptor to an outside file fail, each against an unconfined or `tcp: true` control; the ABI decision is unit-tested as a pure function. Remaining privileges, documented in the SPEC: UDP and other non-TCP sockets, connecting to existing pathname unix sockets, file metadata and path resolution, System V IPC and resource use (ABI 10 adds UDP and unix-socket rights; adopting them is a second profile). Startup and dependency cost: no new dependency; the confined spawn takes the fork path plus three child system calls, not measured. Laputa `995bc30` adopts it for the closed-build cohort: each package build child starts through `linux.run_confined` with declared read roots (`/bin`, `/usr`, `/lib`, the seed, PM modules, `/proc`, `/dev/null`, `/dev/urandom`, its work tree and seeded build root) and its scratch tree as the only write root, with TCP denied. Hostile read, write and TCP cohorts fail with Landlock denials while the honest cohort publishes identical payloads twice, and a kernel without Landlock fails before any child starts. Confinement adds about 122 µs per spawn (unqualified, one host). Open: whether `pm repo build` should confine by default, and an empirical minimum of the read roots (the passing build proves sufficiency, not necessity).
- **Coordination:** Laputa LAP-28 already owns the consumer and OS-enforced closed build; LAP-33 and XSH-37 separately own confined pathname operations.
- **Fix with:** isolate-effects
- **Effort:** M

### XSH-66 · P3 · Cut the checker cost of jointly erased private callables

- **Area:** Inference performance
- **Where:** `src/sema/constraints/`, `core/sort.xsh`
- **Problem:** Erasing all 298 private parameter annotations of `core/sort.xsh` (all pass alone and jointly) produces byte-identical output on 100,000-line text and numeric inputs, but about 2.5 times the annotated source's user CPU (unqualified, shared host: 3.3 s against 8.3 s on `nums`; 0.9 s against 2.7 s on one line). The extra CPU is checker work on every launch, not sort runtime: `xsht check core/sort.xsh` goes from about 0.65 s to about 2.7 s, and a sampled run moves `sema::constraints` self time from 14% to 33% (`TypeConstraints::input_bound_variables`, `unresolved_input_variables`, `CallableApplicationScope` lookups, `Type::clone`). Each erased private callable becomes a `forall` signature with row-polymorphic records and field, method-family and iterable constraints, and every call instantiates and solves them. No single site accounts for it; grouped erasures add up (176 scalar sites about +0.8 s, 69 `SortOptions` sites about +0.5 s). `wc.xsh` (57 sites, 0.74 s to 1.06 s) and `awk.xsh` (9 sites) show the same shape at lower cost, and Laputa `pm/generation.xsh` (47 sites) is unchanged at 0.96 s against 0.98 s. Interface qualification (`xsht check --compare-with`) reports 66 callable and 90 checked-body contract mismatches for erased `sort` (every one a private callable generalized from its concrete signature; the `main` CLI contract does not mismatch), two body mismatches for `awk` (`find_program`, `read_program`), many for `pm/generation.xsh`, and for `wc` the candidate cannot capture a checked body fact at `wc.xsh:390` (`byte_count(0)`: no canonical body site). The self-compare of `wc` passes.
- **Progress:** `da9c2e883` builds a declaration's anchored input support once instead of per call site; in an unqualified interleaved run the erased `sort` check stage fell 71.5%, from 8.8 to 2.5 times the annotated check-stage thread time, with byte-identical output on erased sort, awk and wc and on annotated `core/`. The remaining gap is per-call generic scheme instantiation and constraint solving for the 298 erased parameters, method-family selection, the root file's second effect-probe pass, and lowering of the generalized schemes (lower 0.28 s against 0.08 s, outside `src/sema/`). `--compare-with` now ends each mismatch with `[expected-private-generalization]`, `[interface-drift]` or `[checked-body-drift]`: erased sort reports 67 expected callable generalizations and no callable drift, plus 67 expected and 23 drifting checked bodies. The `wc` body fact at `core/wc.xsh:390:51` is captured; erased `wc` reports 8 expected, 5 interface-drift (its retained `UInt` schema boundaries) and 10 checked-body-drift diagnostics. Two identical generic schemes in a reduced `wc` self-compare (`byte_text`, `line_for`) are still not `equivalent_to` each other.
- **Recommendation:** Profile the constraint solver on the erased `sort` candidate with frame pointers (the verification build has none) and remove the repeated `input_bound_variables` and `unresolved_input_variables` recomputation, or cache solved instantiations per callable. Explain the 23 checked-body drifts on erased sort and the generic self-compare inequality.
- **Acceptance:** Erased `sort` check CPU is within the observed spread of the annotated source's, or the remaining gap is recorded with its cause; the qualification tool distinguishes expected generalization from interface drift.
- **Fix with:** make-it-fast, benchmark-checklist
- **Effort:** M

### XSH-23 · P4 · Evaluate bbtree as the ordered-map implementation

- **Deferred:** The owner is not interested in this evaluation at present. Exclude it from the active backlog run until the owner reopens it, even after a baseline becomes available.
- **Area:** Performance
- **Where:** `src/runtime/value.rs:172`, `src/runtime/value.rs:958`, `src/sema/schemes.rs`, `src/sema/constraints.rs`
- **Problem:** `laputa-systems/bbtree` claims about 5x faster single-thread lookups than std `BTreeMap` with `unsync::BBTreeMap` as a drop-in. XSH uses `BTreeMap` throughout the checker, verifier and runtime values, but most maps are probably tiny, where the gain may vanish. The measured campaign growth is generalization-map copying, which depends on clone cost. `unproven` that ordered-map operations matter at all.
- **Recommendation:** After the post-inference performance baseline, profile the 512-unit generalization witness and script startup to rank `BTreeMap` time. Only if it is a meaningful share, swap `unsync::BBTreeMap` in behind one alias at the hottest owner and compare interleaved CPU time. Vet the single maintainer, its `unsafe` and allocation code, `InlinkeKey` on our key types, and the build-time, binary-size and startup gates before widening.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-42 · P4 · Evaluate an optional SQLite boundary with one real consumer

- **Area:** Optional stdlib
- **Where:** `src/modules/`, `crates/xsh-registry/`, `examples/`, `Cargo.toml`
- **Problem:** Transactional local state may simplify maintenance history or small catalogs, but the benefit and dependency cost are unproven. This is not a requirement for PID 1, PM receipts or the default system.
- **Recommendation:** Select one concrete consumer and compare against ordinary files before implementation. If justified, prototype an optional established SQLite binding with parameterized statements, explicit transactions, owned connections/statements, bounded row iteration and existing value validation. No ORM, SQL type system, storage-engine rewrite or mandatory service.
- **Acceptance:** The consumer demonstrably loses bespoke persistence machinery; tests cover injection-safe parameters, rollback, busy/error handling, cancellation and resource release. Measure dependency, build, startup, binary-size and memory cost against the no-feature build. Stop at the evaluation if no consumer earns that cost; keep early boot and immutable receipts independent.
- **Fix with:** none
- **Effort:** M

### XSH-49 · P4 · Account for first-party complexity in one capability change

- **Area:** Design economy
- **Where:** `docs/DESIGN.md`, `docs/ARCHITECTURE.md`, one selected codec or host-capability change
- **Problem:** Binary-size/startup gates already exist, but they do not expose ownership proliferation, parallel representations, repeated semantics or Rust/native glue added for one XSH-facing capability. A raw line-count ceiling would punish reuse of trustworthy dependencies and tests.
- **Recommendation:** On one real accepted change (the Deser cutover or a bounded host API), record before/after first-party source ownership, runtime variants and operations, representations/conversions, duplicated invariants, required crates, product size and startup, and one end-to-end XSH consumer. Identify machinery retired and any complexity moved outward; use a small review table in the owning existing document, not a separate metrics framework.
- **Acceptance:** Independent reviewer can trace added, retained and deleted responsibilities to code and tests; no compatibility or verifier boundary is counted as removable merely because it duplicates a check. Record a concrete simplification or reject the abstraction on evidence, without blocking unrelated fixes on a new arbitrary LOC quota.
- **Coordination:** Laputa LAP-39 applies the same method to one userspace consumer; existing performance gates remain authoritative.
- **Fix with:** ultratriage
- **Effort:** S
