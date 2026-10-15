# Structural and focused module checks

The scanner observes clean source `619810fb96ca5dbcb4b521012e6fe5a43d0eb0c2`,
tree `43822131f809f792feb33c8b794872a77fadb896`. Its Rust source matches the
flow repair at `d2c842812`. The scanner implementation matches the preserved
verification `xsh` product at `5366d950a`.

[The complete report](metrics.json) records zero manual payload calls,
optional raw calls, wide arena wildcards, and blanket dead-code lines.
Lowering type resolvers are zero. It counts 152 instructions, 17 patterns,
46 stages, one static type predicate, and two storage mappings. Five frame
factories are production sites, and one additional construction is test-only.
The source bound is 32. No runtime bound measurement is established here.

The scanner retains 268,479 production lines across 372 files, with 315,541
physical lines. [Source review](source-review.md) records the exact command,
product identity, allowances, remaining specialized lint queries, and ten
semantic qualification requirements. Static counts do not close those rows.

[Five exact XSHT module/lint tests](xsht-module-tests.log) each pass one test
with exit zero. The cached debug harness has the same Rust content as this
freeze and uses pinned Linux musl, eight CPUs, 16 GiB with no swap,
disk-backed scratch, and no runtime preload. The tests cover parallel shared
imports, converged fixes, imported enum migrations, imported effects, and
legacy enum-helper migration. They do not count module-record or body visits.
The final shared-module trace remains pending on frozen process products.

[The block-device preflight](block-nodev-preflight.log) verifies the isolated
runtime's disk-backed scratch can be remounted `nodev`, with block-node
creation permission only. The container creates, checks, and removes an 8:0
fixture node. Host mount flags are unchanged. This is a confinement preflight;
it executes no product test and establishes no privileged acceptance result.
