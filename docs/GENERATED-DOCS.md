# Generated Docs

Only `docs/STDLIB.md` and `docs/REFERENCE.md` are generated. `xsht docs build`
rewrites those two files, and `xsht docs check` reports drift without changing
the worktree.

| Output | Source of truth | Verification |
|---|---|---|
| `docs/STDLIB.md` | `crates/xsh-registry/src/signature/*`, `src/sema/records.rs`, `crates/xsht/src/docs.rs` | `cargo run -p xsht -- docs build`, `cargo run -p xsht -- docs check`, `cargo test -p xsht docs` |
| `docs/REFERENCE.md` | `crates/xsh-registry/src/reference.rs`, `crates/xsht/src/docs.rs` | `cargo run -p xsht -- docs build`, `cargo run -p xsht -- docs check`, `cargo test -p xsht docs` |

Edit the implementation metadata first, then regenerate the affected output in
the same change. Canonical prose in `docs/` is hand-maintained; use
`docs/DOCS-STYLE.md` to choose its owner.
