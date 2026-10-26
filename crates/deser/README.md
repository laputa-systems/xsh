# Vendored Deser

Upstream: https://github.com/mitsuhiko/deser

Frozen revision: `8c15055178fcf2488dcd1ac70e6d1ff18689b486` (0.10.1).

Retained packages provide JSON, YAML, TOML, INI, CSV/TSV, CBOR and MessagePack,
the core drivers, derive, path/location tracking, the host tooling value tree,
and encoding helpers used by retained conformance tests. External collection and
calendar/decimal adapters are removed, except jiff for retained TOML conformance
tests; it is not enabled in production. Other codecs, bridges,
benchmarks and repository automation are excluded. Apache-2.0 applies.

Ordered collection adapters use the vendored `bbtree::unsync::BBTreeMap` and
`BBTreeSet` in place of the standard-library collections. Reapply this local
integration change when updating Deser. The bbtree source and license live in
`third_party/bbtree` at the repository root.

To update, review a selected upstream revision, replace these packages and their
fixtures, retain the explicit workspace package metadata, and rerun the adapter,
host consumer and upstream conformance tests plus campaign cost gates. Local
JSON generated-source changes must be reapplied and verified when replacing it.

The XSH production graph uses `std`, synchronous `io`, derive for Rust-owned
structures, and codec speedups. JSON adds `itoa` 1.0.18, `zmij` 1.0.23 and
`simdutf8` 0.1.5; the host DOM uses `indexmap` 2.14 and `hashbrown` 0.17.1.
Derive uses `syn` 3 alongside the workspace's existing syn 2. These packages
supply codec number formatting, UTF-8 validation, host maps and macro parsing
that the standard library does not provide. The production graph excludes jiff,
calendar/decimal adapters and the removed JSON backend. Jiff and its timezone
support remain development-only dependencies for retained TOML conformance.
Deser packages are Apache-2.0; these production additions are MIT, Apache-2.0,
or dual licensed under those licenses. The frozen source and focused upstream
coverage bound update risk; upstream activity alone does not establish stability.

The foundation's local verification build peaked at 4,273,080 KiB RSS. This
observation controls build concurrency only. Matched binary-size, build CPU,
startup and runtime acceptance remains the campaign's final cost gate.

Local integration patches retain upstream defaults while adding explicit YAML
version admission and merge-conflict rejection. XSH selects YAML 1.2 and rejects
conflicting merged keys. TOML and INI parser failures carry typed syntax details
for duplicate/range classification without matching error prose. CBOR checks
configured scalar length before growing indefinite-string scratch storage.
These patches have retained upstream corpus and focused negative controls;
reapply and verify them when updating the frozen source.

The core reader accepts a bounded input buffer. CSV checks header uniqueness,
field lengths and record limits while scanning, and can emit an explicit header
for an empty document. Binary decoders check configured string and byte lengths
before reading or allocating declared payloads. YAML bounds parser scratch,
nodes and anchors before producing events; its sequence context also shares an
alias replay allowance across documents. Scalar limits apply to resolved values,
including decoded binary tags, rather than tag metadata or stripped whitespace.
These opt-in limits preserve the standalone packages' defaults.
