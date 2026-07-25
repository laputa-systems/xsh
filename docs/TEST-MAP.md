# Test Map

Choose the narrowest useful command first, then run the broader gate for the
area touched. Do not run formatter or autofix commands for agent work.

## Common Gates

| Change | Narrow command | Broader gate |
|---|---|---|
| Rust compile only | `cargo build` | `cargo test` |
| Syntax parser/formatter | targeted `cargo test --test integration syntax::TEST_NAME` | `cargo test --test integration syntax::` |
| Checker or lint | targeted `cargo test --test integration sema::TEST_NAME` for checker or `cargo test -p xsht --test lint TEST_NAME` for lint | `cargo test --test integration sema::` for checker or `cargo test -p xsht --test lint` for lint |
| Runtime behavior | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test --test integration runtime::` |
| One runtime fixture | `cargo test --test integration runtime::TEST_NAME` | `cargo test --test integration runtime::` |
| CLI/tooling | targeted `cargo test -p xsht --test cli TEST_NAME` or `cargo test -p xsht --test grep TEST_NAME` | `cargo test -p xsht --test cli --test grep` if both apply |
| Benchmark workload | `cargo bench -p xsh-multicall --bench bench BENCHMARK -- --sample-count 1 --sample-size 1` | `make bench-fast` (campaign/memory) or `make bench` (latency) |
| Arena or lowered-IR layout | `scripts/ir-layout.py` (or `--only TYPE` for a focused report) | focused Divan workload plus the applicable behavior tests |
| Frontend retained/peak accounting | `cargo test -p xsh --lib frontend_stats::tests` and `cargo run --bin xsh-frontend-stats -- --json tests/fixtures/frontend-campaign` | `scripts/frontend-campaign-phase0` |
| Indexed IR vertical slice | `cargo test -p xsh --lib runtime::eval::indexed::tests::` | `scripts/frontend-campaign-phase1` |
| Indexed dependency/SCC construction | targeted `cargo test -p xsh --lib runtime::eval::indexed::tests::failed_mutual_recursion_scc_leaves_checkpoint_uncommitted` | `scripts/frontend-campaign-phase2` |
| Interned semantic identities | targeted `cargo test -p xsh --lib runtime::eval::indexed::semantic::tests::` | `scripts/frontend-campaign-phase3` |
| Full indexed function bodies | targeted `cargo test -p xsh --lib runtime::eval::indexed::full::tests::` | `scripts/frontend-campaign-phase4` |
| PGO workflow | `make pgo-profile` | `make bench-pgo` |
| Syscall diagnostics | benchmark smoke test on the host | `make bench-syscalls` on Linux/Docker |
| LLVM IR size | `tools/llvm-lines-repeat-offenders.xsh` over an existing capture | fresh `cargo llvm-lines` capture plus the applicable behavior/benchmark gate |
| Docs/reference/examples | see `Docs Gate` below | same |
| Broad cross-cutting work | closest targeted tests | `cargo test` |

## Docs Gate

Use these commands instead of `make docs`; the make target runs `cargo fmt`.

```sh
cargo build --bin xsh
cargo run -p xsht --features docs-html -- docs build
cargo run -p xsht --features docs-html -- docs check
cargo test -p xsht --features docs-html docs
cargo test --test integration runtime::examples::
```

Run the runtime example test when chapter examples, `examples/catalog.json`, or
generated tutorial output changes.

## Runtime Test Modules

| Area | File |
|---|---|
| collection values and methods | `tests/runtime/collections.rs` |
| coverage, lint, grep-adjacent tooling | `tests/runtime/coverage.rs` |
| frontend campaign scorecard | `tests/runtime/frontend_campaign.rs` |
| cataloged examples | `tests/runtime/examples.rs` |
| interactive behavior | `tests/runtime/interactive.rs` |
| Linux-specific behavior | `tests/runtime/linux.rs` |
| standard modules | `tests/runtime/modules.rs` |
| OS-facing runtime behavior | `tests/runtime/os.rs`, `tests/runtime/unix.rs` |
| process execution and argv | `tests/runtime/process.rs`, `tests/runtime/run.rs` |
| retry blocks | `tests/runtime/retry.rs` |
| structured streams | `tests/runtime/streams.rs` |

## Fixture Locations

| Fixture | Purpose |
|---|---|
| `tests/fixtures/syntax` | parser and formatter fixture sources |
| `tests/fixtures/sema` | checker fixture sources |
| `tests/fixtures/runtime` | executable runtime fixture scripts |
| `tests/fixtures/frontend-campaign` | frozen compact-IR scorecard and blocker inputs |
| `examples` | tutorial examples cataloged in `examples/catalog.json` |
| `showcase` and `showcase/tests` | larger standalone scripts and native tests |

## Commands To Avoid

- `make lint`, `cargo fmt`, `cargo fmt --all`, `cargo clippy --fix`, `xsht fmt`,
  and `xsht lint --fix` rewrite files and are left to the user.
- `make docs` currently runs `cargo fmt`; use the formatter-free docs gate above.
- The `dist` profile is reserved for release packaging, not local agent
  verification.
- Benchmark and PGO commands intentionally use release code generation.
