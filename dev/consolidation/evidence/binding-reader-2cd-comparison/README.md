# Imported-module regression at 2cd

Both products checked the same clean XSH checkout at
`34772ab6a5b41735706aef23cce0ec431256a244`, tree
`cb266327d06351d1abdddf8d3bfd9ae127e306d1`, and clean Laputa checkout at
`ca658ce1da9d89c55209c25268dfdf687ff4fdc6`.

The paired commands were `xsht check .` and `xsht lint .`, from `/work` and
`/laputa`. Both source mounts were read-only. The pinned image was
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
with eight CPUs, 16 GiB memory, no swap, UID/GID 1000, no network, and disk-backed
`TMPDIR=/scratch`. There was no runtime preload. No timing result is claimed.

Baseline 2688 XSHT SHA-256:
`a355ab671ce00844eff4a942c34dd092b0cf77a8e17a0425130828f2d11f7576`.
Candidate 2cd XSHT SHA-256:
`779ec6ae29ca09ce4ef1f4600eebbd03f93f81b4ecf0c8d953a6246bb40c1a4c`.
The candidate's Rust source is unchanged between its build commit
`2cd2baa86d60ec5bbeb62e06fcc062159535450f` and the checked XSH checkout.

All four baseline commands exit zero. Candidate repository and Laputa checking
exit 2; Laputa linting exits 2. Repository linting exits zero but emits new
checker errors, so it also fails diagnostic parity. `status.tsv` and the complete
stdout/stderr files retain all eight observations.

The new errors reject namespace calls through imported standard modules,
including `system`, `unix`, `tui`, `time`, `elf`, and `hash`. A minimal source
containing `use system`, `use unix`, and calls to `system.uname()` and
`unix.cpu_features()` reproduces the baseline/candidate difference. Lexical
preparation assigns runtime binding IDs to these namespace-only imports;
checker import handling creates no corresponding runtime bindings. The new
identity lookup therefore reports a missing binding before module lookup.
These failed results remain evidence for the repair; later passing checks do
not replace them.
