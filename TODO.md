Prefix: XSH
Next: 143

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






### XSH-56 · P2 · Upstream xsh-infra's macOS controller support

- **Area:** Remote execution consumer
- **Where:** `laputa-systems/xsh-infra`, `docs/rfc/XSH-56-xsh-infra.md`
- **Problem:** Linux qualification is complete (Alpine container and KVM suites, pyinfra differential, cancellation, Laputa `xsh` dialect; see the [RFC](docs/rfc/XSH-56-xsh-infra.md)). On 2026-11-20 a native macOS arm64 controller (m1-max, XSH `20b1706c2` verification build, Laputa's vendored xsh-infra `e293bfd`) passed all 156 tests that reach Alpine targets: every `test-alpine-*` suite, `test-transport-ssh` and the planning tests. That needed a scratch fixture change, because `fixtures/alpine.xsh` reaches containers on their Docker bridge address, which macOS cannot route to (`No route to host`); the scratch copy published sshd on `127.0.0.1` and wrote `port` into the inventory. The other 24 tests use the controller itself as the target. They run target scripts that need Linux (`/proc/sys/kernel/random/uuid`, GNU `stat -c`, `touch -d`) and fail on a macOS controller. The upstream `laputa-systems/xsh-infra` repository is not reachable from this environment: it is not on GitHub under that name, on this host, or on m1-max.
- **Recommendation:** In upstream xsh-infra, make the fixture publish sshd on loopback and record the port when the bridge address is unreachable (or always), and skip the local-target tests on non-Linux controllers, naming the Linux facility they need. Revendor into Laputa. Acceptance: on m1-max, `xsh fixtures/alpine.xsh up`, `xsht test --jobs 8` with the inventory, and `down` pass with the local-target tests skipped by reason, and the RFC records the macOS controller run.
- **Fix with:** reference-parity
- **Effort:** S

### XSH-129 · P2 · master CI is red on Lint and Test

- **Area:** Tooling
- **Where:** `dev/test_workflows.xsh:259`, `crates/xsht/tests/lint_format_invariance.rs`; corpus files `core/lib/ipc_report.xsh`, `core/lib/locks.xsh`, `core/lib/memory_report.xsh`, `core/lib/scols_escape.xsh`, `core/tests/test-lsipc.xsh`, `core/tests/test-lslocks.xsh`, `core/tests/test-tail.xsh`, `dev/tests/test-targets.xsh`
- **Problem:** Since at least `c4f2f0411` (2026-11-19), the CI Test job stops before any test because `HOST_UID` is unset when the job runs as root in the container. The Lint job, and locally `lint_format_invariance` in `cargo dev test linux`, fail on findings in the listed files: `lint.shadowing`, `check.ambiguous-grouping` in `ipc_report.xsh:158` after formatting, `check.redundant-parens`, and warnings. The linter's rules are newer than those files.
- **Recommendation:** Pass `HOST_UID`/`HOST_GID` from the workflow, or skip the ownership repair when they are absent. Fix the listed findings at their sources. Acceptance: Both CI jobs pass on master, and `cargo dev test linux` passes `lint_format_invariance`.
- **Fix with:** root-cause
- **Effort:** S



### XSH-138 · P2 · Capture benchmark anchors for tree memory, tooling throughput and artifact size

- **Area:** Performance
- **Where:** docs/TESTING.md#benchmarking; dev/performance/workloads.py; bench/stdlib-port/run.py
- **Problem:** `cargo dev bench` now has `tree-memory` (cgroup v2 `memory.peak` of a fresh pinned container), `tooling-check` and `tooling-lint` (a fixed six-file corpus with a zero-finding oracle), and `artifact-size` (unstripped release `xsh`, `xsht`, `xshi`). Each records its inputs and host readiness, and reports within budget, regression or inconclusive against an anchor. No anchors are committed: on 2026-11-20 `perf-gate-ready` reported this host busy (other users' QEMU guest and Chromium build), so every run is `inconclusive: host readiness check failed; anchor missing`. The memory workload is a small pipeline and the tooling corpus is six files, which bounds what they can detect.
- **Recommendation:** On a quiet host, capture anchors with the command in docs/TESTING.md#benchmarking and commit them with their provenance; widen the memory workload to a representative script if its peak proves too small to resolve a regression. Acceptance: each of the four workloads reports within budget against a committed anchor on a ready host.
- **Fix with:** refactor-benchmarks
- **Effort:** L




















### XSH-141 · P3 · Report Graviola's remaining register and temporary wiping gaps upstream

- **Area:** Cryptography
- **Where:** `crates/xsh-net/src/{lib.rs,trust.rs}`, `src/modules/crypto.rs`, ssh-build `patches/graviola/0001-ssh-chacha20-poly1305.patch`, `crates/oxish-client/vendor/graviola/src/low/x86_64/cpu.rs`
- **Problem:** XSH's TLS setup, SPKI digest and `crypto.ed25519_verify` now return a typed hardware error (`NetError.Hardware`, `crypto-ed25519-hardware`) through `crates/graviola-cpu` before Graviola runs on a CPU without its features, and the stdlib reference states that `crypto.ed25519_verify` accepts small-order public keys. The SSH patch's Poly1305 key is now zeroised (ssh-build `4552fe0`). Two review findings remain inside upstream Graviola ([review](docs/reviews/XSH-111-graviola-review.md)): G-2, the AVX-512 AES-GCM path leaves round keys in zmm16 to zmm31 because `leave_cpu_state` clears only with `vzeroall`; and the rest of G-7, unwiped Poly1305 state, per-call ChaCha20 key copies, the ML-KEM generation seed, SHA-3 contexts and `Blockwise`/padding copies. Graviola's own bmi1/bmi2 entry check (G-3) also needs an upstream report.
- **Recommendation:** File these upstream with ctz/graviola (an owner decision, since it publishes to a third-party tracker), or patch them in ssh-build's Graviola patch set if upstream declines. Acceptance: each finding has an upstream issue or a local patch with a test, recorded in the review.
- **Fix with:** security
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

