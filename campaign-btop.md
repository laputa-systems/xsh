# Pure-XSH btop: unattended campaign

Two parts. **Part A** is the executable campaign: what to deliver, in what order, how to know it is done, and what to do when stuck. **Part B** is reference material that Part A points to; read the section a task names, not the whole of Part B.

Part A is the contract. Where Part B elaborates, Part A wins.

---

# Part A — Campaign

## A1. Start here

1. If `dev/btop-progress.json` exists, read it and resume at its `next` list. Do not rediscover the project.
2. Otherwise begin at G0 (A5).
3. Read before editing, as the repository requires: `AGENTS.md`, `docs/user-tour.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`, `core/SYSTEM-REPORT.md`, and the nearest code and tests for the slice.
4. This campaign is authored against XSH commit `4b0b4e7c`. The working tree is newer (it adds the `xshi` crate). Record the actual starting commit in the ledger and never reset the tree to match the plan.

## A2. Mandate and authorization

Build a btop v1.4.7-compatible Linux system monitor whose application, collection policy, derived metrics, state, layout, widgets, graphs, and interaction are written in XSH. Use it to harden XSH's reusable TUI capability and surface genuine language/runtime gaps.

Work in this repository. No Rust monitor, no wrapper around upstream btop, no second interpreter. Implement, test, measure, fix, document, commit.

**This run's scope is G0 through G3 and G4a.** The product is one fixed default experience that must just work. It has no configuration surface.

**Out of scope by decision** (not deferred: do not build it and do not leave hooks for it):

- configuration files, option parsing beyond the harness flags in B2, settings/options/help menus, presets, runtime update-interval changes, box toggles, graph-mode switching;
- themes and theme selection;
- mouse input and alternate keymaps (vim keys);
- packaging: aliases, inventory growth beyond what the repository contracts force, installers, Laputa VM integration;
- terminal restoration after signal-driven termination (SIGTERM, SIGHUP, SIGINT delivery). The limitation is recorded in the product contract; no signal handlers, native guard, or tests for it are built.

Terminal capability detection (color depth, UTF-8, size) is not configuration. It is a pure function of the environment, and replay supplies the same inputs through the bundle manifest.

Everything else outside G0–G4a is a deferred follow-on campaign (A8). Do not start follow-on work, and do not shrink in-scope work by relabeling it deferred.

Authorization granted by this document:

- **Commit** coherent increments on the current branch (`btop`). Stage explicit paths; never commit unrelated working-tree changes.
- **Never** `git push`, merge, rebase, amend published commits, or rewrite history.
- **Never** run formatters or autofixers (`cargo fmt`, `xsht fmt`, `cargo dev lint --fix`, `xsht lint --fix`, `clippy --fix`).
- **No new dependency** (Cargo or otherwise). If one looks necessary, record it as a blocker with the alternative you would take.
- **Contract changes** (public API, types, schemas, permissions, invariants, transaction boundaries) cannot be asked about. Make the smallest justified change, update code, tests, and docs together, and append an entry to `contract_changes` in the ledger. The final report lists them first.
- **Comments and docs produced by this campaign must be self-contained.** Never cite this file, gate numbers, assertion IDs, or milestones in code comments, docstrings, or product docs.

## A3. Boundaries

1. **XSH owns domain behavior.** Native code exposes reusable OS, descriptor, terminal, time, numeric, Unicode, and byte operations only. It must not know what a CPU panel, sort mode, or graph scale is. A native acceleration needs an XSH reference implementation and measurements naming the smallest operation.
2. **Production starts no helper process.** No `btop`, `top`, `ps`, `df`, `free`, `ip`, `sensors`, `stty`, `tput`, Python, or another XSH. References, generators, emulators, and profilers live in the development harness only.
3. **Canonical rooted collectors.** Procfs/sysfs acquisition lives in `core/lib/sys_*.xsh` through the rooted `FsRoot` source boundary. Not in `core/btop.xsh`, not duplicated from the system-report parsers, and not via live-only `linux.sample()`.
4. **One implementation serves live and replay.** Replay swaps observations, clocks, events, and effects. It never gets its own parsers, rate math, or widgets, and never falls back to the live host for a missing field.
5. **Existing contracts survive.** System-report JSON v1, its redaction and no-argv/no-environ policy, existing applets (including `top`), and the scalar-counting `tui.left_pad`/`right_pad` (pinned by `test_tui_pad_counts_unicode_scalars`). Add cell-aware APIs; do not change the legacy ones.
6. **Do not move goalposts.** The upstream inventory is frozen in G0. A missing capability is not zero, untested is not passed, and a fixture produced by candidate code is not an oracle.

## A4. Run protocol

### Loop

```text
earliest unsatisfied gate -> highest-impact failing/missing assertion
  -> smallest raw/event fixture that reproduces it
  -> expected value derived independently (arithmetic or pinned upstream source)
  -> implement in XSH (smallest generic capability only if required)
  -> narrow check -> update ledger -> commit -> next
```

- Every commit moves at least one assertion's status or is a named enabling change (a reader extraction, a registry primitive). The commit message lists the assertion IDs it moved and the narrow command that proved it.
- The narrow check is: build the touched binaries, run the touched test file, run the ledger check. Nothing wider until gate exit.
- **Progress invariant:** if five consecutive commits move no assertion toward `passed`, stop building, write a checkpoint in the ledger naming the obstacle, and either take the blocked rule below or end the run. Do not keep refactoring.
- Within a gate, work rows and slices top to bottom unless a dependency forces otherwise.

Priority: safety/correctness and boundary violations, then finishing the current gate, then compatibility coverage, then performance. Never accept a faster wrong result or a feature count bought by stubbing data.

### Blocked and stuck rules

- After **three distinct failed fix attempts** on one assertion, minimize the reproducer, record it in `blocked`, and move to the next assertion. Revisit each blocked item once at the end of its gate.
- `external` blockers (missing runner, device, Docker, credential, permission) never block independent work. Record exactly what is missing and the command that would exercise it. An unexercised lane is `blocked_external`, never `passed`.
- A gate cannot be marked passed with any required assertion `failed`, `not_run`, or `blocked_internal`. If an internal blocker has dependents, stop and write the final report (A7). If it has none, continue to the next gate and carry it as an open item.
- Oracle disagreement is resolved from the pinned upstream source and the kernel ABI. Never average, widen tolerances, or regenerate a golden from current output. A defective oracle may be corrected only with independent evidence recorded in the commit message.
- A genuine context or time limit ends the run with a precise checkpoint in the ledger. It is not a completion claim, and it is not a reason to pause after G1.

### Verification

- Linux evidence comes from the pinned lane (`Dockerfile.test`, `cargo dev test linux`). Per increment and at every gate exit, use host release binaries and label the result host-only in the ledger. The lane runs twice in the whole campaign: once at G0 for the baseline, and once after G4a (authorized here, as final-evidence), compared against that baseline. A host-only gate pass is not Linux-support evidence until the end-of-run lane run is recorded. If Docker is unavailable, record `blocked_external` and keep every result host-only.
- Build once per change set: `cargo build --release -p xsh --bins -p xsht --bin xsht`. Native tests: `target/release/xsht test PATH`. Rust integration targets spawn binaries and run only as `cargo test --release --test NAME FILTER`. Debug builds are for `--lib` unit tests only. Never use the `dist` profile.
- One full suite at a time. Ad hoc probes get a wall-clock limit. Reap every child you spawn.
- Docs: edit templates and snippets, then `make docs` and `make docs-check`. Never hand-edit generated docs.
- Resolve exact test IDs with `xsht test --list FILTER` and `--exact`. Never write an invented test name into the ledger.
- Every real bug gets its smallest reproducer added to the test corpus before the fix. A generic runtime/language defect also gets a minimized case in the language tests.

### Delegation

Use `xsh-lane` for a bounded vertical slice over an exclusive file set and `xsh-routine` for mechanical work (the coverage inventory, adding tests from an explicit list). You own integration and the ledger. Parallelizable file sets: `sys_cpu` / `sys_mem` / `sys_net` / `sys_proc` readers and their tests; pure TUI text/width vs event decoder vs surface diff. G1's critical path is serial; do not parallelize it.

### Performance policy

Performance is recorded from G1 on, not postponed. Hard gates (they fail the gate): indexed process joins (1,000 → 2,000 processes, median time ratio ≤ 3×), zero content bytes for an unchanged frame, bounded histories and caches. Everything else in B8 is a **target**: measure on release binaries, store the number in `measurements`, and carry a missed target as a stated limitation rather than a failure. Do not edit a target to make it pass.

## A5. Gates and deliverables

Each row is one deliverable. "Pass" means the named command exits zero and the named assertion IDs are `passed` in the ledger with real test IDs. Names marked *proposed* follow existing conventions but are not existing APIs.

### G0 — Prepare (bounded: one commit, then stop preparing)

| Deliverable | Path / command | Pass |
|---|---|---|
| Upstream pin extracted | `git -C ~/d/btop archive 6e39144aaf5a6bc01b9f795010b0914431067183 \| tar -x -C target/btop-ref` (git-ignored; do not touch `~/d/btop`'s working tree, which is 107 commits past the tag) | `git -C ~/d/btop rev-parse 'v1.4.7^{commit}'` equals the pin; all later upstream reads use `target/btop-ref` only |
| Frozen feature denominator | `dev/btop-coverage.json`: every CLI option, public config key, key binding, menu, and data family at the pin, each with `class`, `upstream_ref`, `gate` (`G1`…`G4a`, `deferred`, or `out_of_scope`), `note` | Row count recorded; every row outside G1–G4a is `deferred` or `out_of_scope` with a reason; internal runtime-state config entries excluded with a reason |
| Fixed defaults | Ledger `defaults` section: the v1.4.7 default of every config key that affects behavior or appearance of an in-scope feature, taken from the pinned config source | Each default names its source location and the in-scope assertion that depends on it |
| Resume ledger | `dev/btop-progress.json` (schema in A6) | Valid JSON; `next` populated |
| Baseline | Release build, then the baseline tests in B10 | Pass/fail recorded in `measurements.baseline`; pre-existing failures are labeled, not fixed here |
| API audit answers | Ledger `audit` section: (a) new-`core/*.xsh` registration requirements (`dev/tests/test-repository-contracts.xsh`); (b) launcher argument-separator spelling; (c) whether the stdlib loader supports multi-file modules such as `stdlib/tui/…`; (d) how `more.rs` is wired into `core_compat_extra` for a PTY test; (e) the registry spelling for a monotonic clock | Each answered with a file reference |
| Linux lane baseline | Run `cargo dev test linux` once on the untouched tree (Docker and an `xsh-test` image are present on this machine; the lane takes no filter and runs the whole suite) | Wall-clock duration and the set of failing tests recorded in `measurements.linux_lane`. The end-of-run lane run compares against this set; only new failures are regressions. If the lane cannot run, `blocked_external` and every result is host-only |
| Ledger check | A native test (`dev/tests/test-btop-ledger.xsh`) that fails when: the schema is invalid; an assertion is `passed` without cited tests, or a cited `path::name` has no matching `test name {` declaration; the coverage row set differs from the digest recorded at G0 (changing the denominator requires a `limitations` entry saying why) | Test passes at G0 and runs in every commit's narrow check |
| Reference build (optional) | Try to build `target/btop-ref` with its documented options | Built, or `reference_unavailable` with the missing tool named; G1–G3 do not depend on it |

### G1 — Small real monitor (checkpoint, not the end)

A four-panel ASCII monitor (CPU, memory, network, processes) over live sources and raw-fixture replay, at `core/btop.xsh`. Scope detail in B2; numerical oracle in B3.

| Deliverable | Path (proposed) | Pass |
|---|---|---|
| Shared readers: aggregate CPU + uptime + load, memory/swap, one interface's counters, process identity + stat/statm | `core/lib/sys_cpu.xsh`, `sys_mem.xsh`, `sys_net.xsh`, `sys_proc.xsh`; extract from `system_report_collect.xsh` where a parser exists, leaving wrappers | `target/release/xsht test core/tests/test-sys-` and `tests/xsh/system-report` still green |
| Interval derivation | `core/lib/btop_rates.xsh` | `g1.cpu.*`, `g1.proc.cpu_*`, `g1.net.rates`, `g1.mem.*` against the B3 literals |
| Model, source adaptation, replay | `btop_model.xsh`, `btop_source.xsh`, `btop_replay.xsh` | `g1.src.*`, `g1.replay.*` |
| Renderer (whole-frame buffered) | `btop_render.xsh` | `g1.screen.*` at 80×24, 120×40, and below-minimum |
| Entrypoint + CLI | `core/btop.xsh`; unsupported options rejected with an error | `g1.cli.*` |
| Monotonic clock, only if absent | smallest registry-conformant API (*proposed* `time.monotonic`) with registry docs and tests | Expansion note (B9) in the commit; `cargo test -p xsh-registry`; `target/release/xsht test tests/xsh/api-tool.xsh` |
| Fixture builder | `core/tests/support/` helper that writes complete, valid kernel-format trees into `fs.tempdir()`; expected values are literals in tests | No expected value is computed by candidate code |
| Native tests | `core/tests/test-btop-*.xsh` | `target/release/xsht test core/tests/test-btop` |
| PTY test of the real entrypoint | Rust target beside `tests/core_compat_extra/pty.rs` / `more.rs` | `g1.pty.*`: two updates observed, resize, `q`, Ctrl-C; termios and modes restored |
| No-subprocess check | `xsht trace --syscalls` (or `test.run_xsht_trace`) over live and replay runs | `g1.noexec`: zero child exec, and no host reads in replay outside the bundle |
| Docs | `core/BTOP.md` (monitor contract, CLI, limits, evidence); registry docs for new API; `make docs-check` | Clean |
| Repository contracts | `target/release/xsht test -j 1 dev/tests/test-repository-contracts.xsh` | Green with `core/btop.xsh` present; any registration the contract demands is the minimum for a new core script, not packaging |

G1 screen assertions are per field: a label and its value at a position relative to its panel, plus the documented minimum-size message. They do not pin the whole-screen arrangement, which G4a replaces.

Required G1 assertions: `g1.cpu.aggregate`, `g1.cpu.cold`, `g1.cpu.idle_interval`, `g1.proc.cpu_total`, `g1.proc.cpu_per_core`, `g1.proc.rss`, `g1.proc.sort_ties`, `g1.proc.lifetime` (exit, appear, same PID new start), `g1.net.rates`, `g1.net.unavailable`, `g1.net.select`, `g1.mem.used`, `g1.mem.swap_absent_vs_zero`, `g1.mem.malformed`, `g1.src.zero_vs_absent`, `g1.src.truncation_no_prefix`, `g1.screen.80x24`, `g1.screen.120x40`, `g1.screen.too_small`, `g1.cli.reject_unknown`, `g1.replay.deterministic`, `g1.replay.no_live_fallback`, `g1.replay.headless_no_sleep`, `g1.pty.update_resize_quit`, `g1.pty.ctrl_c`, `g1.noexec`, `g1.regress.system_report`, `g1.regress.top`.

Gate exit: all of the above passed on host release binaries, commit "btop G1", ledger updated. **Continue straight to G2.**

### G2 — Reusable terminal substrate

Grow G1 in tested increments. Terminal mechanics, pure rendering, and monitor policy stay separate. Detail in B4–B6.

| Deliverable | Pass |
|---|---|
| Owned terminal session over the audited primitives (`unix.tty_attrs`/`set_tty_attrs`/`tty_mode`, `poll_fd`, `read_fd`, `window_size`), composed in XSH with `defer`; no new native terminal resource. Raw mode disables signal generation, so Ctrl-C arrives as a byte | `g2.term.cleanup`: original attributes and modes restored on normal return, `q`, Ctrl-C byte, stdin EOF, handled write failure, runtime error mid-render, and partial initialization. Signal-driven termination is out of scope (A2) |
| Non-terminal behavior: piped stdin is never consumed as keys; interactive start without a usable terminal fails before changing anything | `g2.term.nontty` |
| Capability detection: color depth, UTF-8, and symbol set as one pure function of `TERM`, `COLORTERM`, `NO_COLOR`, locale variables, and window size; replay takes the same inputs from the manifest | `g2.caps.detect` (table-driven cases, including the Linux console and `TERM=dumb`) |
| Deadline-driven loop with decoupled "sample due" and "sample completed", coalesced backlog, monotonic intervals | `g2.loop.deadline`, `g2.loop.coalesce` (virtual clock) |
| Incremental event decoder in XSH | `g2.input.split_points` (every split of UTF-8/CSI/SS3), `g2.input.multi_per_read`, `g2.input.esc_timeout`, `g2.input.malformed_utf8`, `g2.input.bounded`, `g2.input.paste_inert` |
| Cell-aware text: display width, cell clip, cell pad, safe single line | `g2.text.width` (CJK, combining, emoji policy, two-cell boundary), `g2.text.hostile` (ESC/OSC/CR/bidi/LF inert); legacy pad tests untouched |
| Surface, clipping, delta presentation with a committed-frame model | `g2.surface.clip`, `g2.surface.diff_equals_full`, `g2.surface.partial_write_recovery`, `g2.surface.unchanged_zero_bytes`, `g2.surface.bottom_right` |
| Independent screen oracle: a minimal VT reconstructor in `dev/` (or inside the Rust test target) supporting exactly the emitted sequences and rejecting others; no new dependency | Used by every PTY and delta test |
| Read-only interaction: selection by process identity, scroll, sort + reverse, filter edit/clear/cancel, resize | `g2.ui.select_identity`, `g2.ui.sort_reverse`, `g2.ui.filter`, `g2.ui.resize`, interactive replay of filter/edit/select/resize |
| Reusability proof: a small generic table + input + overlay app in `tests/xsh/stdlib/` using only public `tui` APIs | `g2.generic_app` |
| No native btop logic | `g2.no_native_btop`: no monitor-specific symbol under `src/`, `crates/`, `stdlib/` (search recorded) |
| Latency | Input-to-present latency measured separately from collection interval and stored in `measurements` (target in B8) |
| Contracts | Registry docs for every new public `tui`/`time` function; `cargo test -p xsh-registry`; `make docs-check` |

Gate exit: all G1 assertions still pass, commit, continue.

### G3 — Correct Linux telemetry

Vertical slices, one commit each: raw fixture → shared reader → interval derivation → visible field → reference check → measurement. Rules in B3 and B7; matrix rows in B11.

| Slice | Content | Assertion prefix |
|---|---|---|
| S3.1 | Scheduling classes, cache TTL/invalidation, eviction of exited identities and old device epochs | `g3.sched.*`, `g3.cache.*` |
| S3.2 | Per-core CPU with sparse IDs and hotplug, guest accounting, iowait correction, counter reset → unavailable, load, uptime | `g3.cpu.*` |
| S3.3 | Memory categories, absent `MemAvailable`, no swap, duplicates, malformed, large values | `g3.mem.*` |
| S3.4 | Network: selection, rename/replacement epoch, reset, empty set, namespace change, unit conventions | `g3.net.*` |
| S3.5 | Processes: robust `stat` parse, identity race, indexed previous-sample join, parent/child adjacency with orphan/cycle/self-parent/PID reuse, tree aggregation, all pinned sort modes, filter, command line (shown locally by default, never environ, never exported or captured) | `g3.proc.*` |
| S3.6 | Observed-only CPU temperatures (signed, package and core, duplicate and missing channels), frequency, battery states. Fixture-tested only; there is no hardware to verify live battery behavior, and that gap is recorded as a limitation | `g3.sensor.*` |
| S3.7 | Counter precision at i64 bounds and large bases, clock edge cases (zero/reversed rejected, wall jump harmless, pause gap explicit) | `g3.counter.*`, `g3.clock.*` |
| S3.8 | **Last by decision.** Block I/O (512-byte sectors, `in_flight` gauge, no double counting across layers), mounts with captured capacity results, escaped paths | `g3.disk.*` |

Work the slices in order. Gate deliverables beyond slices:

- **Reader inventory** in `core/BTOP.md`: field → kernel source → owning `sys_*` function → unit → counter/gauge → static/dynamic → bounds. A search for duplicate parsers of the same ABI is recorded as clean.
- **Scaling hard gate:** 1,000 → 2,000 preloaded processes, median ratio ≤ 3×, reproducibly generated from a seed.
- **Churn test:** many process/device identities cycle with no history contamination and bounded cache counts.
- **Degradation:** removing any optional source leaves the monitor usable and labels the loss.
- Existing system-report, `sys_*`, and `top` tests unchanged and green.

Gate exit: all earlier assertions still pass, commit, continue.

### G4a — Graphs, default look, process tree

Port upstream behavior conceptually; do not transliterate its global state. The result is upstream's default experience, fixed. Detail in B6 and B11.

| Deliverable | Assertion prefix |
|---|---|
| Graph engine: braille by default with the TTY symbol set chosen automatically by capability detection (no block mode); inversion, zero handling, gaps for missing values, deterministic autoscale that keeps spikes, retention independent of viewport, resize never invents samples. Braille gets an exhaustive 2×4 dot-mapping and time-column test | `g4.graph.*` |
| Panels with graphs for CPU (including temperatures), memory, and network (autoscaled), in upstream's default arrangement; borders, titles, labels. The memory box follows upstream's default geometry including its disks section from the start; until the disk panel lands, that section renders an explicit unavailable marker, so no golden is rewritten when it does | `g4.panel.*` |
| One built-in palette defined in truecolor and mapped down to 256, 16, or none by capability detection (the mapping is a pure tested function); `NO_COLOR` honored | `g4.color.*` |
| Fixed defaults: every default extracted in G0 lives in one module as a named constant carrying its upstream value, and a test asserts the module equals the ledger extraction. The product accepts no options and reads no configuration; nothing it does depends on `~/.config` | `g4.defaults.*` |
| Process tree: toggle, collapse/expand, expand/collapse all, aggregation; sort cycling, reverse, and filter matching the pinned defaults | `g4.proc.*` |
| Default key bindings for in-scope features only; every other upstream key is a no-op, and a test asserts it triggers nothing | `g4.keys.*` |
| Semantic goldens: expected cells and styles derived independently from the pinned source or arithmetic; region-level diagnostics; no whole-screen similarity score | all of the above |
| **Last by decision.** Disk section of the memory box: device and mount list with usage and I/O rates and the I/O graph, using the G3 disk readers | `g4.disk.*` |

Work the rows in order; the disk row is the final deliverable of the run. G4a's layout replaces G1's temporary fixed arrangement by a stated expected-behavior change; this is why G1 screen assertions are per-field (see G1).

Gate exit: all earlier assertions still pass, commit. Then run the end-of-run Linux lane once (A4), record its result against the G0 baseline, and write the final report.

## A6. Ledger

`dev/btop-progress.json` is small and is the only resume point. Large logs, profiles, binaries, and any real host capture stay untracked.

```json
{
  "schema": 1,
  "xsh_start_commit": "",
  "upstream": {"tag": "v1.4.7", "commit": "6e39144aaf5a6bc01b9f795010b0914431067183"},
  "gate": "G0",
  "last_green_commit": "",
  "audit": {},
  "assertions": [
    {"id": "g1.cpu.aggregate", "gate": "G1", "implemented": false,
     "status": "not_run", "tests": [], "evidence": ""}
  ],
  "blocked": [{"id": "", "kind": "external", "reason": "", "reproducer": "", "unblock_command": ""}],
  "contract_changes": [{"kind": "api", "name": "", "commit": "", "summary": ""}],
  "limitations": [],
  "measurements": {},
  "next": []
}
```

`status` is one of `not_run`, `failed`, `passed`, `blocked_external`, `blocked_internal`, `reference_unavailable`, `unstable`, `not_applicable`. `passed` requires at least one real test ID and an evidence line. `implemented` is independent of `status`. `dev/btop-coverage.json` is the fixed denominator and is never filtered by candidate output.

## A7. Completion and final report

The run is complete when G0–G3 and G4a are all `passed` with evidence, or when stopped by a limit or blocker with the ledger current. The final report (in the last assistant message and summarized in `core/BTOP.md`) contains, in order:

1. Contract changes (every `contract_changes` entry).
2. Per-gate status: required, passed, and each non-pass class, by domain. No single composite percentage.
3. Open blockers with reproducers and unblock commands.
4. Measurements against B8 targets, with misses stated.
5. Known limitations and intentional differences from upstream, including the evidence lane (host-only or Linux lane, with the end-of-run result against the baseline), the unverified live battery path, and terminal state after signal-driven termination.
6. Out-of-scope decisions (A2) and follow-on campaigns (A8) with their entry conditions.

Never write "full btop parity". Name the pinned release, Linux, the scope of this run, and what is deferred.

## A8. Follow-on campaigns (not part of this run)

| Campaign | Content | Entry condition |
|---|---|---|
| **G4b** | Safe process actions (signals and renice via an identity-bound handle; effect-recording sink; owned helper children only; replay never acts), per-process detail view with on-demand smaps and I/O, process following, upstream render/input adapter for exact comparison, suspend/continue | G4a passed. The reducer already models requested effects; no executor exists until here. |
| **G5** | Multi-battery and power edges, ZFS-specific behavior, GPU lanes (kernel-exported first; narrow optional vendor adapters returning raw typed observations; kernel-only vs vendor-adapter profiles published; live hardware lanes separate from ABI-double tests) | G4b passed; eligible hardware or an explicit decision to ship decoder-only |
| **G6** | Full benchmark lanes and budgets (B8), endurance and fault injection, bounded fuzzing | G4a passed |
| **Portability** | macOS first (replay evidence is not a live collector), then other upstream systems per runtime support | Linux product qualified |

---

# Part B — Reference

## B1. Audited baseline (checked against the working tree)

**Present in the registry**

- Terminal: `unix.isatty`, `controlling_tty`, `tty_attrs`, `set_tty_attrs`, `tty_mode` (raw, cooked, cbreak, sane), `window_size`, `set_window_size`, `open_pty`, `foreground_group`.
- Descriptors: `unix.poll_fd(fd, events, timeout_ms)` (readable/writable; `-1` waits forever; deadlines survive EINTR), `read_fd(fd, max_bytes)`, `open_fd`, `close_fd`.
- Signals: signal hooks in the language (SPEC §12.2); `WINCH` is in the signal table.
- Cleanup: `defer`.
- Time: `time.now` (wall-clock), `sleep`, `measure`, `millis`/`seconds`, calendar functions. **No monotonic clock.**
- `Int` is signed 64-bit.
- `tui`: ANSI helper strings, `left_pad`/`right_pad` (scalar counting), `read_secret`. No width, clipping, surface, or event decoding.
- Tests build fixture trees with `fs.tempdir()` and `root.write`/`mkdir`/`symlink`, as in `core/tests/test-sys-block.xsh`.

**Prior art, not a dependency:** `crates/xshi/src/interactive/{term,input,render,session}.rs` is a Rust raw-mode session, input decoder, and renderer for the shell's line editor, feature-gated and tuned for `ish` parity. It informs cleanup and SIGWINCH policy. It is not the monitor's substrate.

**Existing owners to reuse**

| Owner | Use | Do not |
|---|---|---|
| `core/lib/sys_source.xsh` | Bounded reads, complete-observation rule, field-addressed issues, numeric lexical checks | Turn an incomplete read into a valid-looking prefix |
| `core/lib/sys_block.xsh`, `sys_mount.xsh`, `sys_pci.xsh`, `sys_usb.xsh` | Kernel-ABI models with identity and relationships | Reparse `lsblk`/`iostat` text; confuse a gauge with a counter |
| `core/lib/system_report_collect.xsh` | Existing parsers: extract, leave wrappers | Add a second parser for the same ABI |
| `core/lib/system_report_live.xsh` | Static discovery and race handling | Run full discovery every refresh |
| `core/lib/system_report.xsh` | Observation vocabulary, CPU-list semantics | Add values to JSON-v1 enums |
| `core/lib/procps.xsh` | CPU math (guest not double-counted, iowait correction) | Change `cpu_percent` legacy behavior |
| `core/top.xsh` | Evidence only: batch, `linux.sample()`, O(P²) previous-sample filter | Copy the nested scan |
| `dev/system_report_check.xsh` (20k+ lines) | Evidence-model patterns | Append to it |

Naming: new `sys_*` modules follow the existing pattern and tests `core/tests/test-sys-*.xsh`. Fixture bundles for `--replay` go under `core/tests/data/btop/`.

## B2. G1 behavior detail

| Panel | Minimum content |
|---|---|
| CPU | Aggregate interval usage with a simple meter; uptime and load when observed |
| Memory | Total, available, used (`MemTotal - MemAvailable`); swap total/used or observed absence |
| Network | One deterministically selected interface (name order, skipping loopback when others exist); RX/TX rates and totals |
| Processes | PID, numeric UID when observed, command name, interval CPU, RSS; descending CPU with stable tie-breaks |

- Sizes: 80×24 and 120×40. Below the minimum, a safe message; no crash, no out-of-bounds.
- Empty, unavailable, and zero are visibly different. The first rate sample is `warming`, never fabricated inactivity.
- `q` and Ctrl-C exit immediately without Return. Periodic refresh. Resize handling. Restoration on exit.
- Not in G1: graphs, tree, detail view, signals, renice, GPU.
- Whole-frame buffered redraw is acceptable in G1. Per-character writes, shelling out for modes, and calling a full system report per tick are not.

**CLI.** The product takes no options. The flags below are harness flags that select the observation source and output sink (live vs bundle, terminal vs headless, dumps); they never change what the monitor displays or how it behaves. Any other argument is rejected with an error. The launcher separator spelling is an audit answer.

```text
btop                                  live monitor
btop --replay BUNDLE                  interactive deterministic replay
btop --replay BUNDLE --headless       no terminal, no real-time sleeping
btop --replay BUNDLE --headless --dump-model F.jsonl
btop --replay BUNDLE --headless --dump-screen F.jsonl
btop --frames N                       bounded diagnostic run
```

Decide and document whether `--frames` counts presented frames, completed samples, or both. A replay ends at the end of its finite stream. Its action backend can never target a live PID.

**Inject, do not infer:** page size, clock ticks per second, monotonic observation times, and source identity. A source root does not encode them.

## B3. Gate-1 numerical oracle

Two logical CPUs, 100 ticks/s, 4096-byte pages, exactly one second between observations:

```text
                      t0                      t1
/proc/stat aggregate  user=100 system=50      user=160 system=70
                      idle=850 other=0        idle=970 other=0
(cpu0 and cpu1 lines present and consistent with the aggregate; /proc/uptime and
 /proc/loadavg present)
PID 42, same start    cpu_ticks=10            cpu_ticks=35        RSS=16384 pages
interface RX bytes    1,000,000               1,010,240
interface TX bytes      500,000                 502,048
MemTotal              1,048,576 kB            unchanged
MemAvailable            262,144 kB            unchanged
```

Expected at t1:

```text
aggregate CPU        (60 + 20) / (60 + 20 + 120) = 40.0%
PID 42, total-machine 25 / 200 = 12.5%     (default, proc_per_core=false)
PID 42, per-core      25 / 200 × 2 = 25.0%
PID 42 RSS            16384 × 4096 = 67,108,864 B = 64 MiB
RX 10,240 B/s = 10 KiB/s     TX 2,048 B/s = 2 KiB/s
used memory           1 GiB − 256 MiB = 768 MiB
```

- Files are complete, valid kernel formats (the full `stat` field set), not pseudo-files with only these lines. Assert raw bytes/digests and arithmetic. The values above are the oracle.
- Label the default CPU convention. Never compare values under different denominators.
- G1 cases beyond the table: cold start; idle interval; empty process enumeration; process disappearance and same-PID/new-start reuse; unavailable interface; malformed and truncated memory input; resize then quit.

## B4. Rates and validity rules

Document in `core/BTOP.md` and test as pure functions.

- **Interval timing.** Use measured monotonic times per subsystem, never the requested sleep. A zero or negative interval is an invalid temporal state, not a division by zero.
- **Process CPU (upstream parity).** Upstream divides a process's tick delta by the delta of the sum of the first eight aggregate `/proc/stat` fields (all CPUs, including idle and iowait, excluding guest), multiplies by core count only in per-core mode, rounds to 0.1, and clamps to `[0, 100 × cores]`. Reproduce this for the compatibility mode and verify the exact expression in the pinned `btop_collect.cpp` before writing the test. Net and disk rates use measured monotonic time. Affinity, cgroup quota, and host CPU capacity are not interchangeable; any cgroup-aware denominator is an explicit separate mode.
- **First observation.** No compatible predecessor means `warming` (or an explicitly labeled since-boot value if requested), never a measured rate.
- **Reset and wrap.** A decrease is a reset and an invalid interval unless a modular width and tested wrap policy exist. A changed boot/session/device/process identity resets the baseline. Never clamp negatives to zero and call it inactivity. The iowait correction is a CPU-specific policy, separate from reset.
- **Counter range.** `Int` is i64. A kernel u64 beyond i64 is an out-of-range observation, not clamped. The monitor's wire form carries wide counters as canonical decimal strings; JSON v1's exact-number bound is untouched.
- **CPU.** Guest time already inside user/nice is not double-counted. Sparse logical IDs and hotplug are fixtures. Missing topology does not block utilization.
- **Memory.** Keep raw counters. Absent `MemAvailable` is not zero (any fallback is separately specified and tested). Overlapping categories are not summed into a total. Swap absence and zero use differ.
- **Block I/O.** Reuse the 11/15/17-field parsing and forward-field policy. `in_flight` is a gauge. Accounting sectors are 512 bytes regardless of physical size. Disk plus partitions plus stacked layers are not summed.
- **Mounts.** Distinguish available-to-user from free space and their percentage conventions. Replay uses captured capacity results, never `statvfs` of a similarly named live mount.
- **Network.** Rates over measured time; byte/bit and binary/decimal are explicit. Interface name alone does not carry history across replacement. Do not apply deltas across namespaces.
- **Sensors and power.** Keep signed values, stable channel identity, units, and absent/failed distinctions. Requested frequency is not measured clock speed. Charge is not converted to energy without the observed inputs.
- **Temporal states** `warming`, `reset`, `stale`, `gap` belong to the monitor model; do not add them to system-report enums.

## B5. State and identity

- Typed records and unions in the hot path; wire decoding at a narrow boundary.
- Distinguish: source scope and boot/session identity; inventory vs latest observation; raw counters vs derived rates; availability vs temporal validity; selected process identity vs its current row; history storage vs viewport width; collection scheduling vs UI state; requested effects vs effect results.
- A process key is source session + PID + start ticks. CPU histories use actual sparse IDs. Device histories use stable identity plus a removal/replacement epoch.
- The process set and device set can change between list and read; every read re-validates identity.
- Pure functions touch no filesystem, clock, environment, terminal, or process. Boundaries:

```text
initialize(capabilities, inventory)         -> State
parse_sources(raw, units)                   -> RawSample
derive(previous, current, interval)         -> DerivedSample
reduce(state, event)                        -> {state, requested_effects}
layout(viewport, panels)                    -> Layout
render(state, layout, display_policy)       -> Surface
present_delta(previous, next)               -> EncodedFrame
```

- Scheduling classes: startup/invalidated inventory; frequent counters (CPU, network, process CPU/RSS); less frequent gauges (temperature, frequency, battery); slow or blocking (mount capacity) bounded with age and status. A selection-dependent class (smaps, per-process I/O) arrives with the detail view in G4b. Cached UID/command are not immortal: exec or credential change invalidates them, so use triggers or TTLs and label stale values.

## B6. Terminal, input, text, surface rules

**Session**

- One active session per terminal. No global broker or daemon. No XSH code or restoration inside an async signal handler; hand off through the established mechanism.
- Use raw mode with signal generation off, so Ctrl-C and Ctrl-Z arrive as bytes: the decoder turns Ctrl-C into cancellation and ignores Ctrl-Z. Restoration runs on every in-process exit path. An external SIGTERM, SIGHUP, or SIGINT kills the process without restoring the terminal; this is a documented limitation in the product contract (the user's recovery is `reset`), not something to engineer around in this run. Suspend/continue belongs to G4b.
- Assert original termios, cursor visibility, alternate-screen exit, paste mode, and style reset to the extent each was changed. SIGKILL and a dead transport are documented limits.
- Deadlines, not `collect; render; sleep`. One event loop first; add bounded concurrency only if measurement forces it, with workers returning typed samples and never touching the terminal. Cap concurrent work, input queue length, and retained samples; record coalesced/dropped sample events so replay stays explicit. On a detected suspend or discontinuity, reset or mark baselines.

**Input**

- XSH implements the decoder over chunks. Handles ASCII controls, UTF-8, CSI/SS3 arrows and navigation, the function keys used by the chosen keymap, EOF, and incomplete sequences. A lone Escape gets a bounded, replayable timeout.
- Unknown sequences are ignored or rejected under a bounded rule. Unterminated escapes and pastes have caps. Huge claimed parameters never allocate huge buffers. Pasted text is text: a paste containing `q`, Enter, or a hotkey activates nothing. Encoding references: xterm control sequences.
- Bracketed paste is part of G2 because pasted input must stay inert. Mouse reporting is never enabled, so mouse sequences are unsolicited input and fall under the unknown-sequence rule.

**Text and styles**

- New cell-aware operations next to the legacy scalar ones. Pin a display policy: Unicode data version, ambiguous-width convention, zero-width and unsupported cluster handling, ASCII fallback. East Asian Width alone is not a terminal-width algorithm.
- Keep the source value separate from the sanitized display value. Data containing ESC/OSC/CR/bidi/line separators cannot move the cursor, set a title, touch the clipboard, spoof a row, or leave a style open. Reuse system-report's sanitization policy without its report redaction in the local view. Raw ANSI inside data is never a "styled label".

**Surface and presentation**

- Pure rectangle intersection, clipping, borders, labels, meters, table rows, selection, viewport scrolling, and a small input field. Validate geometry and cap allocations.
- Keep a **committed** surface. Advance it only after the whole frame is written. On a partial write or abandoned frame, invalidate the assumed screen and recover with a controlled full redraw. Coalesce desired state, never a chain of patches.
- Clear shortened rows, erased overlays, trailing text, resized regions, and both halves of a replaced wide glyph. Test the last column and bottom-right cell with no autowrap scroll. An unchanged frame emits no content bytes. Synchronized output is optional and always closed on handled failure.

## B7. Process model and privacy

- Parse `/proc/[pid]/stat` by bracketing the command field with the last `)`, never by whitespace splitting.
- Previous-sample lookup is an identity-keyed map. Build parent/child adjacency once per sample. Detect self-parent, cycles, missing parents, and PID reuse without recursion that can exhaust the stack. Sort and collapse tie-breaks are deterministic. Visible rows are a view; the complete retained set is what sorting and aggregation use.
- System-report never reads command lines or environments. Keep that. The monitor displays each process's command line locally by default, as upstream does: a bounded read through a monitor-specific view over a shared raw reader, subject to ordinary procfs permissions. **Environments are never read.** Command lines are never written to a replay bundle, dump, or capture unless the bundle is synthetic, and the dump flags in B2 omit them.
- Permission failures preserve "unknown".

## B8. Performance targets (record, do not edit)

Normal profile: 1,000 synthetic processes, 32 logical CPUs, 120×40, 2 s interval, release binaries, one named runner recorded in the ledger. Keep the runner stable; do not claim these absolute numbers elsewhere.

| Measure | Target |
|---|---|
| Input-to-completed-present latency | p95 ≤ 50 ms, p99 ≤ 150 ms |
| Frequent-source sample + update work | p95 ≤ 20% of the interval |
| Preloaded model update + render + diff | p95 ≤ 50 ms |
| Unchanged frame (clock frozen) | 0 content bytes after the first commit (**hard gate**) |
| Process join | indexed; 1k → 2k median ratio ≤ 3× (**hard gate**) |
| Histories and caches | count bounds asserted directly (**hard gate**) |
| Endurance (G6) | second equal replay window adds < max(8 MiB, 10% of steady RSS) |

Keep collection, pure update, pure render, transport, and startup separate so a filesystem change cannot pass as a faster runtime. Use `xsht runtime-stats --json REPORT SCRIPT`, `xsht trace --syscalls --trace-top-syscalls 10 SCRIPT`, and `bench/stdlib-port` conventions; pair allocation counts with host RSS. Optimize in order: unnecessary work, then algorithms, then representation/runtime costs, and only then a new reusable runtime capability. Never a native `collect_all_metrics` or `render_btop`.

## B9. Language/runtime expansion note

Every generic API or runtime change carries this in its commit message (or the ledger):

```text
Need:             concrete monitor behavior that is missing or too slow
Existing surface: APIs inspected and tried
Evidence:         minimal reproducer; for performance, a measured profile
Smallest change:  why it is a general API, not btop code
Contract:         types, effects, ownership, cleanup, errors, bounds, determinism
Independent use:  at least one non-btop test that guards it
Verification:     tests run
Cost:             dependencies, binary/build cost, allocations, platform
```

Likely candidates, not a work list: monotonic clock, cell width, bounded byte scanning, bulk buffer building, map/update efficiency. Prefer a library or representation change over new syntax. Update parser, checker, lowering, verifier, and registry together if a language change is unavoidable; never add a fallback interpreter. If a primitive is accelerated, keep an XSH reference implementation in tests and verify equivalence on randomized input.

## B10. Commands

```sh
# build (once per change set)
cargo build --release -p xsh --bins -p xsht --bin xsht

# narrow native tests
target/release/xsht test tests/xsh/stdlib/tui.xsh
target/release/xsht test core/tests/test-btop
target/release/xsht test core/tests/test-sys-
target/release/xsht test tests/xsh/system-report.xsh
target/release/xsht test tests/xsh/system-report-collect.xsh
target/release/xsht test dev/tests/test-system-report-check.xsh
target/release/xsht test --list FILTER        # resolve IDs; --exact for one

# host boundary (spawns binaries: release only)
cargo test --release --test core_compat_extra FILTER

# new public API
cargo test -p xsh-registry
cargo test -p xsh --lib modules::signature
cargo test --release -p xsht --test integration api::
target/release/xsht test tests/xsh/api-tool.xsh
target/release/xsht check docs/snippets/api

# repository contracts when adding core/btop.xsh
target/release/xsht test -j 1 dev/tests/test-repository-contracts.xsh
python3 core/tests/origins/check_origins.py --strict

# docs
make docs && make docs-check

# canonical Linux lane (G0 baseline and once after G4a)
cargo dev test linux
```

Baseline set recorded in G0: `tests/xsh/stdlib/tui.xsh`, `core/tests/test-sys-`, the two `tests/xsh/system-report*.xsh` files, `dev/tests/test-system-report-check.xsh`, and `dev/tests/test-repository-contracts.xsh`.

Do not add a `cargo dev btop-check` command in this run. Gate evidence is `xsht test`, the Rust PTY target, and the ledger. Revisit in G6 if a combined checker earns its cost.

## B11. Fixtures, replay, and oracles

**Three levels, three claims**

| Level | Input | Proves |
|---|---|---|
| Raw-source replay | Raw procfs/sysfs observations plus recorded OS scalars and events | Production acquisition, parsing, identity, derivation, reducer, render |
| Typed model replay | Independently built typed samples and events | Pure state/layout/widget behavior, scale tests |
| PTY transport | The real program, raw input chunks, size changes, signals, captured output | Descriptor lifecycle, decoding, presentation, cleanup |

Report which level each result exercises. Typed fixtures never replace raw tests; screenshots never replace numerical assertions.

**Bundle (start simple)**

```text
bundle/
  manifest.json        schema version, origin (synthetic vs capture), scope/boot identity,
                       page size, ticks/s, capabilities (TERM/COLORTERM/locale), size, digests, limits
  events.jsonl         sequence, virtual timestamp, kind, typed payload
  snapshots/0000/root/...   complete synthetic tree at sample 0 (and 0001, ...)
  observations.jsonl   non-file results: scalars, monotonic readings, mount capacity, errors
  expected/model.jsonl   expectations established independently
  expected/screens.jsonl canonical cells/styles at checkpoints
```

- Total order `(timestamp, sequence)`; duplicates and misordering are rejected. When acquisition becomes asynchronous, recorded completion order decides transitions. Never mutate a tree while pretending the observation was atomic.
- Freeze randomness by seed, wall clock by explicit input, monotonic time by event timestamps, environment by manifest, terminal by capabilities. A replay bundle is untrusted: bound files, entries, nesting, bytes, events, and work; reject traversal, escaping symlinks, and device files.
- Races (a process changing between two reads) and permission denial under root cannot be modeled by static trees or `chmod 000`. Use the existing mocks if they intercept the rooted read boundary, otherwise the smallest reusable observation tape with per-operation ordering, errno, disappearance, and truncation. Real parsers stay downstream and unchanged. No general virtual filesystem.
- Real captures stay private (mode 0700, outside tracked source), are never auto-committed, and never include environments or, without separate authorization, command lines. Hashing does not anonymize. Synthetic fixtures are the checked-in corpus.

**Oracles**

- Simple fixtures: arithmetic from the fixture specification (B3).
- Richer cases: a development-only decoder/calculator or pinned utility that does not import candidate helpers.
- Upstream: (1) tests derived from the pinned source with independently computed values; (2) optional controlled live comparisons with bracketed telemetry and fixed size, locale, and color mode; (3) a narrow upstream adapter is G4b. Never patch upstream until it agrees with the candidate.
- Screen oracle: the dev-only VT reconstructor consumes real output and compares effective cells, styles, and cursor at declared checkpoints. For every delta test, also render the target as a fresh full frame and require identical decoded state; inject partial writes and resize between frames. Raw bytes are diagnostics, not the golden.
- Checker self-tests: changed raw bytes or metadata, missing expected assertion, contradictory states, duplicate event IDs, unknown schema, wrong units, wrong oracle digest, candidate-declared `unsupported` for a required fixture.

**Regression matrix (in-scope rows)**

| Family | Minimum assertions | Gate |
|---|---|---|
| Cold and second sample | warming first; independent second rate; no fake zeros | 1 |
| Idle vs unavailable | zero ≠ absent/permission/malformed | 1 |
| Process lifetime | exit, appear, same PID new start, stable ties | 1 |
| Bounded source failures | truncated prefix never a measurement; neighbors kept | 1 |
| Geometry and resize | 80×24, 120×40, below minimum; quit and cleanup | 1 |
| Input chunking | every split; multiple events per read; Escape timeout | 2 |
| Terminal lifecycle | partial open, handled error, Ctrl-C byte, stdin EOF | 2 |
| Cell layout | CJK, combining, emoji policy, two-cell boundary, zero-size | 2 |
| Hostile text | ESC/OSC/bidi/CR/LF inert | 2 |
| Damage correctness | shortened line, overlay removal, partial write, forced redraw | 2 |
| Input fairness | filter/scroll/quit during sampling; bounded queue | 2 |
| CPU accounting | sparse IDs, hotplug, guest, iowait, reset | 3 |
| Counter precision | values at 2^53 and i64 bounds; small deltas on large bases | 3 |
| Clock behavior | zero/reversed rejected; wall jump harmless; pause gap explicit | 3 |
| Process-stat parsing | parens/spaces/odd bytes; partial read; identity race | 3 |
| Process tree | orphan, cycle, reparent, collapse, aggregation, filtered tree, reuse | 3, 4a |
| Memory/swap | none, zero, absent available, duplicates, malformed, large | 3 |
| Mount/block | escaped paths, stacked, removal, 11/15/17/future stats, in-flight | 3 |
| Network | rename, replacement, namespace, reset, known wrap, empty set | 3 |
| Slow metadata | cache age, exec/UID change, source disappearance, invalidation | 3 |
| Sensor/power | negative readings, missing channel, duplicate labels, charging, none | 3 |
| Graph symbols/scaling | braille/block/TTY, thresholds, zero/gap, inversion, resize, autoscale | 4a |
| Fixed defaults and color | defaults table equals upstream extraction; no option or config read; color-depth mapping and `NO_COLOR` | 4a |

## B12. Upstream and references

Pin: `https://github.com/aristocratos/btop` commit `6e39144aaf5a6bc01b9f795010b0914431067183` = tag `v1.4.7` (2026-05-01), license Apache-2.0. Read only from `target/btop-ref` (the archived pin). `~/d/btop` is at `v1.4.7-107` and its collector and draw code have diverged; an exploratory read of it is not authority for the v1.4.7 denominator. Preserve notices for any copied or translated source or assets, and check vendored components individually.

| ID | Upstream file | Use |
|---|---|---|
| B-collect | `src/linux/btop_collect.cpp`, `src/btop_shared.hpp` | Acquisition, units, process CPU formula |
| B-config | `src/btop_config.cpp` | Source of the pinned default values only; the option surface is out of scope |
| B-draw | `src/btop_draw.{hpp,cpp}` | `Graph`, `Meter`, `TextEdit`, boxes, layout |
| B-input | `src/btop_input.cpp` | Default key dispatch for in-scope features (mouse and vim keys out of scope) |
| B-lifecycle | `src/btop.cpp` | Startup, refresh loop, and shutdown behavior (menus and CLI out of scope) |
| B-theme | `src/btop_theme.cpp` | Default palette values only (theme loading and bundled themes out of scope) |

Kernel and terminal: `https://docs.kernel.org/filesystems/proc.html`, `https://docs.kernel.org/admin-guide/iostats.html`, `https://invisible-island.net/xterm/ctlseqs/ctlseqs.html`, `https://www.unicode.org/reports/tr11/`.

XSH owners and their blob IDs at the plan's base commit (a changed file is inspected, not reset):

```text
AGENTS.md                          48affe50771251b2e003cd032502d97cb7ab6eba
core/SYSTEM-REPORT.md              a7b6e75b90aed88893fb92da9c12e14473386b66
core/system-report.xsh             87f40515764a80a9bb746f1a6ba4b39c02e47e74
core/lib/system_report.xsh         591e8064caa4e6426b8bd4e6675c0039da7302d8
core/lib/system_report_live.xsh    fc3296f644baf2d622e6447b34c28f12a768bb93
core/lib/sys_source.xsh            46e57e57f6fa1051e775f196ba5cdafcaec238a3
stdlib/tui.xsh                     78d7f1e175ebcf0ba5a23684e98d2786847fe512
src/modules/tui.rs                 9a890762422ff46ae8e6d80c474ceb5ce8f57203
tests/xsh/stdlib/tui.xsh           b0ae7ce2c5a35b02cdfc33ce5413c23eac1042f5
core/top.xsh                       1da938ae459ce35921ecf920b571b4a681ec8c13
core/lib/procps.xsh                f89732b16b1ba5fd91b09a5e413ba812424473ce
```

Further local owners to inspect as changes require: `src/modules/time.rs`, `src/modules/unix.rs`, `src/modules/unix/tty.rs`, `src/runtime/eval/modules/`, `crates/xsh-registry/src/{managed_resource,runtime_op}.rs`, `crates/xsh-registry/src/signature/{modules,methods,docs}.rs`, `tests/core_compat_extra/{pty,more}.rs`, `bench/stdlib-port/`.
