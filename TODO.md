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



### XSH-138 · P2 · Qualify benchmark resource and tooling outcomes

- **Area:** Performance
- **Where:** docs/TESTING.md#benchmarking; dev/performance/workloads.py; bench/stdlib-port/run.py
- **Problem:** Unproven: current fixed synthetic workloads do not establish workload-tree peak memory, representative xsht check and lint throughput, or retained artifact size budgets, so resource regressions can escape comparison.
- **Recommendation:** Add representative completed-work oracles and scoped resource measurements with accepted anchors for those outcomes.
- **Fix with:** refactor-benchmarks
- **Effort:** L


















### XSH-139 · P3 · Qualify timing gates against spread and attribution

- **Area:** Performance
- **Where:** bench/stdlib-port/run.py; dev/performance/README.md; docs/TESTING.md#profiling
- **Problem:** The standard-library runner reports round MAD but its fixed budget decision ignores spread; current CPU phase diagnostics exclude child processes and cannot attribute tree-wide memory or I/O.
- **Recommendation:** Compare paired measurements against observed spread and document separate process-tree CPU, allocation and syscall attribution runs.
- **Fix with:** benchmark-checklist
- **Effort:** M


### XSH-141 · P3 · Close the remaining Graviola hygiene and CPU-gate gaps outside the SSH client

- **Area:** Cryptography
- **Where:** `crates/xsh-net/src/{lib.rs,trust.rs}`, `src/modules/crypto.rs`, ssh-build `patches/graviola/0001-ssh-chacha20-poly1305.patch`, `crates/oxish-client/vendor/graviola/src/low/x86_64/cpu.rs`
- **Problem:** The [Graviola review](docs/reviews/XSH-111-graviola-review.md) left four Low or Info findings open. G-3: XSH's TLS, `crypto` module and SPKI digest never call a CPU gate, so a host without Graviola's features faults (for example a Raspberry Pi 4 without the crypto extensions running `net` HTTPS) instead of returning a typed error. G-2: the AVX-512 AES-GCM path leaves round keys in zmm16 to zmm31. G-6: `crypto.ed25519_verify` accepts small-order public keys, and its docs do not say so. G-7: the ChaCha20-Poly1305 patch zeroes the Poly1305 key with a plain loop the compiler may remove, and several temporaries are never wiped.
- **Recommendation:** Gate `xsh-net` and the `crypto` module on the same feature check as `oxish-client::cpu::require`, and return the typed hardware error. Use `zeroize` for the Poly1305 key in ssh-build's patch. Document the small-order acceptance on `crypto.ed25519_verify`. Report G-2 upstream to Graviola. Acceptance: a test with a feature reported missing makes an HTTPS request and `crypto.ed25519_verify` fail with the hardware error, and the stdlib reference states the small-order behaviour.
- **Fix with:** security
- **Effort:** M


### XSH-142 · P3 · Upgrade Graviola past 0.4.3 for its RSA and NIST-curve verification fixes

- **Area:** Cryptography
- **Where:** ssh-build `scripts/vendor-graviola.sh`, `patches/graviola/`, `crates/oxish-client/vendor/graviola`, workspace `[patch.crates-io]`
- **Problem:** Graviola 0.4.1's RSA verification accepts e=1, even exponents and short signatures (`mid/rsa_pub.rs:22`, `high/rsa.rs:136-155`), and P-256 and P-384 accept public coordinates of at least p (`mid/p256.rs:264-267`), per the [Graviola review](docs/reviews/XSH-111-graviola-review.md) (G-4, G-5). Only TLS through rustls-graviola reaches them; forging with e=1 still needs a trusted or user-added CA to certify such a key.
- **Recommendation:** Vendor Graviola 0.4.3 or later in ssh-build. Rebase the SSH ChaCha20-Poly1305 patch: 0.4.2 rewrote Poly1305 and added a NEON ChaCha20. Then revendor here. Acceptance: the 440 OpenSSH vectors in `cargo test -p oxish-graviola` pass on x86-64 Linux and macOS arm64, and a test refuses an e=1 RSA key.
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

