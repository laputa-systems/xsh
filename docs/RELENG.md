# Release engineering

This document owns XSH's optimized build profiles, the shipped product
configuration, and the record of product sizes. [Testing](TESTING.md) owns which
profile each test and gate uses.

## Profiles

| Profile | Use | Settings |
|---|---|---|
| `dev` | unit tests and local iteration | no debug info |
| `release` | performance measurements | `opt-level = 2`, thin LTO |
| `verification` | process tests, native suites, gate products | inherits `release`, `opt-level = 2`, no LTO, 64 codegen units |
| `dist` | shipped products | inherits `release`, fat LTO, one codegen unit, stripped |

`cargo dev dist` builds `dist` products under `target/$TARGET/dist`. It also
rebuilds the standard library (`-Z build-std=std`) with
`-Cpanic=immediate-abort` and `-Zlocation-detail=none`, requests
`xsh/net xsh/tools xsht/native-tests`, and selects the target CPU in
`dev/targets.xsh`: `x86-64-v3` for x86_64 and `neoverse-n2` without SVE for
aarch64. `xsht/native-tests` enables `xsh/default`, including SSH, through
Cargo feature unification. Linux products link statically against musl: with the
host's mold on x86_64 Linux hosts, and with `rust-lld` (the image default) elsewhere.

Darwin products link through the system C compiler driver without ICF: Apple's
`ld` has no ICF option, and the pinned `rust-lld` cannot read the macOS 27 SDK
TAPI files. Darwin `dist` products are therefore larger than an ICF-capable link
would produce. Revisit ICF when the pinned LLVM reads the installed SDK.

## Historical P0 product sizes

The scoped-verification P0 built these products on `fcdae517e` in the pinned
Linux image for x86_64. Both profiles used O3. Current profiles use O2, so these
sizes do not establish an O2 baseline. The O2 `dist` comparison appears below.

| Product | Verification bytes | Release bytes (thin LTO) |
|---|---|---|
| `xsh` | 48,039,856 | 45,461,864 |
| `xsht` | 57,345,872 | 60,585,776 |
| `xshi` | 587,320 | 566,240 |

## Optimization level

Products build at `opt-level = 2`. O3 made the shipped binaries about 5% larger
for no speedup we could measure, and `verification` follows `release` so tests
exercise the shipped code generation. Reconsider O3 only for a measured hot path
where its gain exceeds the size cost, and prefer fixing that path first.

Measurement on `fcdae517e`, x86_64 musl, built in the pinned image as
`cargo dev dist` does:

| Product, stripped `dist` | O3 bytes | O2 bytes | Change |
|---|---|---|---|
| `xsh` | 27,342,344 | 25,945,608 | -5.1% |
| `xsht` | 39,288,200 | 37,090,696 | -5.6% |
| `xshi` | 44,016 | 43,952 | -0.1% |

Ten interleaved pairs of the `dist` binaries, CPU time, O2 over O3:

| Workload | Median ratio | Range |
|---|---|---|
| `xsht check core` (928 files) | 1.026 | 0.950 to 1.043 |
| `sort` over 300,000 lines | 1.041 | 0.977 to 1.272 |
| `wc` over 300,000 lines | 0.984 | 0.887 to 1.011 |
| `xsh --startup`, 200 runs | 1.013 | 0.965 to 1.433 |

Every median falls inside its own pair spread. The host was loaded by other
work during the pairs, so these bound the cost rather than measure it exactly.

## Where the size goes

The unstripped O2 `dist` `xsh` holds 23.1 MB of code, 1.2 MB of read-only data
and 1.1 MB of unwind tables. By symbol owner:

| Owner | Code |
|---|---|
| `xsh` | 12.0 MB |
| `core` and `alloc` generics instantiated for XSH types | 4.6 MB |
| Graviola | 1.2 MB |
| `rustls` and `rustls_graviola` | 0.6 MB |
| Deser codecs | about 0.6 MB |
| `xsh_registry` | 0.3 MB |
| regex | about 0.3 MB |

Inside `xsh`, the largest owners are the indexed executor and verifier
(3.0 MB), the checker (1.7 MB), the evaluator (1.1 MB), lowered execution
(0.8 MB), lowering (0.7 MB) and the constraint solver (0.7 MB). The largest
single functions are Graviola's P-384 and P-256 routines (110 to 258 KB) and the
executor's `ExplicitFrames::continue_value` (143 KB).

Fat LTO and one codegen unit roughly halve the generic instantiations compared
with `release` (9.4 MB there), so measure size on `dist` products. To reproduce
the attribution, build `dist` with `CARGO_PROFILE_DIST_STRIP=false`, then sum
`nm -S --size-sort -C` text symbols by their leading path segment.

## Size investigation, 2026-10-29

The 12 MB of `xsh` code is mostly single-copy: `cargo llvm-lines` over the `xsh`
library (8.57 million IR lines) finds only 4.5% reclaimable duplication in
`xsh`-owned functions. The duplication sits in standard-library generics
instantiated over `xsh` types, about 4.3 MB of final code: `BTreeMap` and
`BTreeSet` across about 301 key and value types (2.7 MB), 239 stable-sort
instantiations (1.5 MB), and iterator adapters (1.4 MB), overlapping. The
verifier, which every built program runs, is about 2.0 MB.
`tools/llvm-lines-repeat-offenders.xsh` reproduces the duplication figures.

Measured `dist` variants of `xsh` against the O2 baseline (25,944,584 bytes
stripped). CPU ratios are interleaved host pairs with readiness checks; their
ranges are wide, so they bound the cost:

| Variant | Stripped bytes | Size | CPU cost |
|---|---|---|---|
| `xsh` package at `opt-level = "s"` | 21,179,656 | -18.4% | sort 1.15, wc 1.14, startup 1.05 |
| `-C llvm-args=-inline-threshold=100` | 23,581,192 | -9.1% | sort 1.07, others within spread |
| `-Zshare-generics=yes` | 26,021,000 | +0.3% | not measured |
| thin LTO, one codegen unit | 26,695,368 | +2.9% | not measured |
| `-Zmerge-functions=aliases`, LLVM merge-functions | 25,944,584 | none | not measured |

Decisions of 2026-10-29: build options stay as they are, so neither knob is
taken. The verifier stays in the shipped `xsh`: indexed execution relies on
verified programs, so removing it would trade 2 MB for an unchecked execution
path. Unmeasured source candidates, estimated from symbol counts: routing
name-keyed sorts through one helper (0.5 to 0.8 MB), consolidating cold-path
`BTreeMap` types (0.8 to 1.5 MB), and table-driven host-call errors (0.2 to
0.4 MB).

## Allocator

On Linux, `xsh` and `xsht` use mimalloc as the global allocator. `xshi` uses
it on Linux only when its `laputa-asterinas` feature is enabled; the standard
distribution ships the disabled stub. Other platforms use the system allocator.
The jemalloc package in `Dockerfile.test` is a different thing: `cargo dev` preloads
it into compiler steps only, to speed up builds, and no product links it.
Changing the product allocator needs a paired runtime and memory comparison
first.

`xsh` reserves mimalloc arenas without committing them up front
(`src/entrypoints/allocator_commit.c`), so arena space a process never touches
stays out of its resident memory. On hosts with transparent huge pages set to
`always`, that costs allocation-heavy scripts kernel time: a 20,000-record JSON
roundtrip takes about 13,000 page faults instead of about 520 and about 16%
more CPU than with eager commit, which would add about 4 MB of idle and 13 MB
of peak resident memory per process. Many `xsh` processes run at once, so the
memory is kept.

## C toolchain

`Dockerfile.test` installs the prebuilt musl LLVM 23.1.5 from
`laputa-systems/llvm-prebuilt-musl` release `llvm-musl-23.1.5-dc0d9b9`. The
Linux musl products are linked by its `clang-23` driving mold, so they take
that bundle's static musl 1.2.6 `libc.a`, built by the bundle's clang, and
LLVM's `libunwind.a`, rather than the GCC-built musl and unwinder in Rust's
self-contained sysroot. Rust's crt objects are still used. Against the same
source in the previous image this added 28,888 bytes to `xsh`, 29,016 to
`xsht`, and 3,240 to the `xshi` stub, all `.text` from clang's codegen of
musl (`vfprintf`, the regex engine, the crypt functions); the `xshi` stub's
7.2% was accepted on 2026-11-07 as fixed libc cost in a 45 KB binary.

## Size record

SQLite adds `rusqlite` 0.43.0 with the `bundled` and `hooks` features and
default features disabled. Its bundled engine is SQLite 3.54.1. Both builds
below used the pinned Linux test image and `cargo dev dist --docker never`
for stripped x86_64 musl `dist` products. The baseline is `6ab55223c`.

| Product | Baseline bytes | SQLite bytes | Change |
|---|---:|---:|---:|
| `xsh` | 27,023,152 | 28,820,816 | +1,797,664 |
| `xsht` | 39,356,784 | 41,178,192 | +1,821,408 |
| `xshi` | 44,240 | 44,240 | 0 |

| Date | Source | Product | Stripped `dist` bytes |
|---|---|---|---|
| 2026-10-29 | `e0f3028ce` | `xsh` | 25,945,608 |
| 2026-10-29 | `e0f3028ce` | `xsht` | 37,090,696 |
| 2026-10-29 | `e0f3028ce` | `xshi` | 43,952 |
| 2026-10-30 | `de0ed2606` | `xsh` | 23,704,272 |
| 2026-10-30 | `de0ed2606` | `xsht` | 32,515,056 |
| 2026-11-07 | `73ade365f` | `xsh` | 26,618,120 |
| 2026-11-07 | `73ade365f` | `xsht` | 38,358,728 |
| 2026-11-07 | `73ade365f` | `xshi` | 48,112 |

On 2026-11-16 the pinned x86_64 Linux image built source
`12827bb0be864a42747ec82f60560fda1b8578cd` with the `release` profile,
`xsh/net xsh/tools xsht/native-tests`, and static musl. GNU `strip --strip-all`
produced these sizes. These are release-profile sizes, not `dist` sizes.

| Product | Stripped `release` bytes |
|---|---:|
| `xsh` | 37,262,856 |
| `xsht` | 47,003,080 |
| `xshi` | 403,400 |

The existing 5% size gate compares like profiles and targets. The last `dist`
rows above remain its `dist` reference until a current `dist` build is measured.
[Testing](TESTING.md#performance-baseline) records the CPU measurements.

## Linux test image lock

`dev/images.lock` contains the published image reference and the SHA-256 of
its `Dockerfile.test`, on separate lines. The all-zero digest is a placeholder
until the image is published. Consumers reject a stale Dockerfile hash.

To rebuild and repin, select a multi-platform Docker buildx builder and run
`cargo dev image build`. This produces `.work/test-image.oci`. The publisher
publishes that layout, replaces the placeholder with the published index digest,
and updates the second line with `sha256sum Dockerfile.test`. Do not repin
the Dockerfile hash to an image built from different contents. Alpine packages
are fixed by the published image, not by rebuilding the rolling edge repository.
For local troubleshooting, `XSH_TEST_IMAGE` selects an already loaded image;
`XSH_TEST_IMAGE_BUILD=1` explicitly builds a local image.

## G0 release candidate

G0 is complete. The candidate is the single immutable XSH commit `b268b5007`,
tagged `xsh-g0-rc1`, with its xsh, xsht, xshi, and core script products.

The G0 freeze admitted only these changes:

- correctness, data-loss, and resource-ownership repairs;
- fixes that Laputa's G0 consumers need;
- test and documentation corrections;
- verification repairs without which a result cannot be trusted.

New APIs, codecs, language changes, speculative optimization, and verification
features waited for a later milestone. An inference failure belonged to G0 only
when it breaks a documented language guarantee or a Laputa program.

XSH-53 and XSH-54 (supervised DHCPv4 and syslog) add APIs on `master` under the
owner's campaign authorization. They are outside the frozen G0 candidate.

A result counts only for the executable and source it ran. Any later executable
change reruns the affected qualification. A later commit whose diff touches only
planning records carries its qualification results forward.

### Scope exclusions

- Performance and resource gates do not block G0. The owner waived the quiet-host requirement on 2026-10-28, and
  no G0 claim depends on a timing.
- The graphical profile is outside G0, so Laputa's Muon build blocker does not
  gate it.
- macOS, Linux aarch64, and Asterinas are outside the G0 claim.

### Recorded result

All four completion criteria hold for `b268b5007`.

1. **Native failures.** Every failing selector in the last full native run has a
   recorded cause, a focused reproducer, and a repair or an out-of-scope reason.
2. **Combined run.** A privileged run on `8d7330542`, whose products and tests
   are identical to the candidate's, passed 14,588 tests with 151 skips and one
   incomplete. The incomplete gate test saw commits land on the checkout during
   the run and passes on a quiet tree. Of the skips, 72 are the
   private-namespace lane's tests, which pass 72 of 72 on the same products. The
   other 79 need root, external network or TLS fixtures, or tools absent from
   the image.
3. **Laputa suites.** Laputa's check, test, PM, xinit, and system suites pass
   with the candidate's products, with recipes that rebuild rather than reuse
   cached artifacts.
4. **Laputa qualification.** An earlier `make verify` with the candidate's
   seed found no XSH defect and eight Laputa defects, fixed through Laputa
   `9485988`. One uninterrupted run at Laputa `7d550bc` then passed every step.

Identities of the passing run:

- **Seed:** XSH `b268b5007`, clean, `release, lto=false`, x86_64-musl. Product
  sha256 prefixes: `xsh` `abb9e5cffd31e9ec`, `xsht` `4e964e4271999830`, `xshi`
  `f6355b0fca563977`.
- **Kernel:** artifact `3c602079f1ed…`, rebuilt from clean.
- **Profile:** `qemu-dwl-foot` generation `d37ec0bbb131…`.
