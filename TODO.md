Prefix: XSH
Next: 144

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

### XSH-143 · P2 · Find why `timeout .1 tail -f` can outlive its 10 s bound on arm64 CI

- **Area:** Core commands
- **Where:** `core/tests/test-gnu-tail.xsh:556` (`test_gnu_tail_wait_log`), `core/timeout.xsh`, `core/tail.xsh:97`, `src/runtime/eval.rs` (`service_pending_signal`, `ignore_pending_primary`)
- **Problem:** Master Test run 44713717696 (265e0626c, a docs-only commit, arm64 runner) failed only `test_gnu_tail_wait_log`: one `wrapped(s, [timeout, .1], tail -s.1 --max-unchanged-stats=1 -f|-F ...)` call hit the harness's 10 s process timeout (`timeout: process timed out`, 12.0 s), so `timeout` did not end `tail` within 10 s. The same test passed in 2.4 s on the previous run (44703633567). Suspected path, unproven: `on TERM [] {}` in core/tail.xsh, and `register_indexed_signal_hook` sets `ignore_pending_primary` when TERM is already recorded at arming; `service_pending_signal` then returns without running the hook or the unhooked shutdown, so a TERM landing between the last checkpoint and arming is dropped, while SPEC 12.2 says such a signal is treated as having no hook. Local attempts did not reproduce a hang: on x86_64 with the verification build, 100 runs at `.1` and 240 at 5 to 50 ms gave no hang with either the current code or a build that routes that case to the unhooked path. A TERM blocked and raised before exec is never delivered, so it is not a usable seam. `core/timeout.xsh`'s `send` also ignores ESRCH from `kill_group`.
- **Recommendation:** Reproduce on arm64 under load (m1-max OrbStack linux/arm64 container with the CI image, the native case run repeatedly), with `xsht trace` or a signal trace on the `timeout` and `tail` processes to see which one is stuck and why. If the arming race is the cause, route a TERM pending at arming to the unhooked path as SPEC 12.2 states, with a regression test that lands the signal inside arming. Acceptance: the cause is named with evidence and a fix has a test that failed before it.
- **Fix with:** root-cause
- **Effort:** M

### XSH-112 · P4 · Port buildlib's source locks to XSH

- **Area:** Tooling
- **Where:** `bench/consolidation/prepare.py`, [laputa-buildlib](https://github.com/laputa-systems/buildlib) `buildlib.locks`
- **Problem:** `bench/consolidation/prepare.py` fetches its inputs without buildlib's verified source locks, and buildlib is Python-only, so XSH build scripts cannot use the contract natively.
- **Recommendation:** Port `buildlib.locks` (source-input validation and verified downloads) and `buildlib.hashing` to an XSH library that follows the [build repository contract](https://github.com/laputa-systems/stuff/blob/main/docs/build-repos.md), then move the consolidation bench preparation onto it. Acceptance: The XSH library reads `sources.lock.json`, rejects inputs whose SHA-256 or size differs, and the consolidation preparation fetches through it.
- **Fix with:** reference-parity
- **Effort:** M

### XSH-113 · P4 · Stop locking unused Deser bridge crates

- **Area:** Dependencies
- **Where:** [laputa-systems/deser](https://github.com/laputa-systems/deser) `deser-core/Cargo.toml`, XSH `Cargo.lock`
- **Problem:** `laputa-deser-core`'s `std` feature names its optional bridges through weak features (`rust_decimal?/std`, `bigdecimal?/std` and others). Cargo locks dependencies named that way even when nothing enables them ([rust-lang/cargo#10801](https://github.com/rust-lang/cargo/issues/10801)), so XSH's lockfile carries `rust_decimal`, `borsh`, `rand`, `wasm-bindgen` and `toml_edit`, none of which is built. `cargo fetch` and Laputa's vendoring still download them.
- **Recommendation:** In the fork, give each bridge a feature that enables `dep:X` and `X/std` together so `std` no longer names the optional crates; publish and update XSH. Acceptance: XSH's `Cargo.lock` contains no Deser bridge crate that no enabled feature builds.
- **Fix with:** none
- **Effort:** S

