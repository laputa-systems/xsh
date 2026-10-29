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
`-Cpanic=immediate-abort` and `-Zlocation-detail=none`, enables only
`xsh/net xsh/tools xsht/native-tests`, and selects the target CPU in
`dev/targets.xsh`: `x86-64-v3` for x86_64 and `neoverse-n2` without SVE for
aarch64. Linux products link statically against musl with mold.

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

## Size record

| Date | Source | Product | Stripped `dist` bytes |
|---|---|---|---|
| 2026-10-29 | `e0f3028ce` | `xsh` | 25,945,608 |
| 2026-10-29 | `e0f3028ce` | `xsht` | 37,090,696 |
| 2026-10-29 | `e0f3028ce` | `xshi` | 43,952 |

The performance campaign's binary-size gate allows each release product at
most 5% growth over its baseline; this record is the current baseline for that
comparison.
