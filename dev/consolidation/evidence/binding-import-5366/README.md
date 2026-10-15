# Standard-module import repair

Repair: `5366d950ad644fec8652b2a409148c75261a1358`, tree
`185404824d6d5bdcb3be3c13153a6021ae18d3b9`.
The nine production lines omit accepted namespace-only standard imports from
runtime value-binding facts. User imports, invalid aliases, unknown imports,
and ordinary missing-value rejection retain their existing paths. The native
regression adds 24 lines; no assertion was weakened.

## Reproduction and focused checks

`stdmodule-fact-min-run.xsh` runs successfully with baseline 2688 and fails
checking with candidate 2cd. Their complete outputs and the exact native
regression's red output are retained here. The regression subsequently passes
with verification products, alongside five existing exact tests covering
implicit modules, aliases, shadowing, user-module exports, and sibling imports.
The six `verification-native-*.log` files each record one pass.

Four exact private Missing-identity tests also pass, with their compile-only
unit-harness output retained. The lane source was
`618a6adb9e920cbe383872a626a5533e5b822e4c`; its checker files and native test
file match the merged source exactly. Earlier three misqualified Rust filters
selected zero tests and are not counted. The qualified logs each select one.

## Merged products and diagnostic parity

The merged-source verification build succeeded in pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
with eight CPUs, 16 GiB, no swap, Kache, mold, static CRT, and the `sscanf` and
`strtol` isoc23 aliases. Jemalloc preload applied only to compilation.

| Product | Bytes | SHA-256 |
|---|---:|---|
| xsh | 31,593,840 | `83ffe690b3a7feb215ef7cb5fb5800d8818c1fb71ef67bd8ef712753b5c733a7` |
| xsht | 38,740,888 | `7303cca98dcfb32012113fdcdb821e87009faeb75a94530c5caa58a910e55ead` |

The merged products match the lane verification products byte for byte.
Baseline XSHT 2688 has SHA-256
`a355ab671ce00844eff4a942c34dd092b0cf77a8e17a0425130828f2d11f7576`.

Both products ran `xsht check .` and `xsht lint .` against the same clean XSH
source above and clean Laputa `ca658ce1da9d89c55209c25268dfdf687ff4fdc6`.
Sources were read-only at `/work` and `/laputa`. The same pinned image had
eight CPUs, 16 GiB, no swap, UID/GID 1000, no network or preload, disk-backed
`TMPDIR=/scratch`, and a 240-second bound per command.

All eight commands exit zero with empty stdout. Diagnostics match byte for
byte after validating and removing only the final timing line, following
`stderr_before_timing_line` in the integration harness. Complete stderr,
`status.tsv`, and `parity.txt` retain the observations. No timing result is
claimed. The [failed 2cd comparison](../binding-reader-2cd-comparison/README.md)
remains separate. Canonical and final corpus acceptance are still open.
