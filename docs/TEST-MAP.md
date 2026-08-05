# Test Map

Choose the narrowest useful command first, then run the broader gate for the
area touched. Do not run formatter or autofix commands for agent work.

## Common Gates

| Change | Narrow command | Broader gate |
|---|---|---|
| Rust compile only | `cargo build` | `cargo test` |
| `Lexer::lex_compact`, `Parser::parse_source_arena_only`, or formatter | targeted `cargo test --test integration syntax::TEST_NAME` | `cargo test --test integration syntax::` |
| `Checker::check_compact_declarations`, `Checker::probe_compact_bodies`, or lint | targeted `cargo test --test integration sema::TEST_NAME` for checker or `cargo test -p xsht --test integration lint::TEST_NAME` for lint | `cargo test --test integration sema::` for checker or `cargo test -p xsht --test integration` for lint |
| `Evaluator::prepare_compact_indexed_only`, `indexed_run`, or runtime behavior | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test --test integration runtime::` |
| One runtime fixture | `cargo test --test integration runtime::TEST_NAME` | `cargo test --test integration runtime::` |
| `xsht::cli::CliOutput`, `xsht::grep::find_matches_in_program`, or CLI/tooling | targeted `cargo test -p xsht --test integration cli::TEST_NAME` or `cargo test -p xsht --test integration grep::TEST_NAME` | `cargo test -p xsht --test integration` |
| Benchmark workload | `cargo bench -p xsh-multicall --bench bench --features benchmark-support BENCHMARK -- --sample-count 1 --sample-size 1` | `make bench-fast` (memory/regression) or `make bench` (latency) |
| Non-Tokio archive/network dependency update | `cargo tree -i tokio` and `cargo tree -p xsh-net -e features` | focused archive or network runtime gate |
| Lowered evaluator dispatch | `cargo bench -p xsh-multicall --bench bench --features benchmark-support xsh_lowered_scanner_1000_calls_execution -- --include-ignored --sample-count 1 --sample-size 1` | `make bench` after the focused A/B |
| Arena or indexed-IR layout | `scripts/ir-layout.py` (or `--only TYPE` for a focused report) | focused Divan workload plus the applicable behavior tests |
| Frontend retained/peak accounting | `cargo test -p xsh --lib frontend_stats::tests` and `cargo run --bin xsh-frontend-stats -- --json tests/fixtures/frontend-indexed` | `make bench-fast` after the applicable syntax/checker gate |
| `FullBuilder::build_compact`, `FullVerifier::verify`, or executable IR | targeted `cargo test -p xsh --lib runtime::eval::indexed::full::tests::` | `cargo test -p xsh runtime::eval::indexed::full::tests --lib --features native-tests` |
| Explicit execution frames | targeted `cargo test --test integration runtime::stack_depth -- --test-threads=1` | `cargo test -p xsh runner::tests --lib --features native-tests` plus the runtime gate |
| Production executable runtime | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test -p xsh --test integration runtime:: --features native-tests -- --test-threads=1` plus `cargo test -p xsh --test integration runtime::coverage::xsh_native_tests --features native-tests -- --exact` and `make bench-fast` |
| Explicit PGO/release investigation after ordinary gates pass | `make pgo-profile` | `make release-pgo` |
| Syscall diagnostics | benchmark smoke test on the host | `make bench-syscalls` on Linux/Docker |
| LLVM IR size | `tools/llvm-lines-repeat-offenders.xsh` over an existing capture | fresh `cargo llvm-lines` capture plus the applicable behavior/benchmark gate |
| API registry/reference/examples | see `API Gate` below | same |
| Broad cross-cutting work | closest targeted tests | `cargo test` |

## API Gate

```sh
cargo build --bin xsh
cargo build --bin xsht
cargo test -p xsh-registry --lib
cargo test -p xsh --lib modules::signature
cargo test -p xsht --test api
target/debug/xsht api
target/debug/xsht api summary --format jsonl
git diff --check
```

Run the relevant language or runtime test gate when the API contract or an
example exposes behavior that changed outside the registry and renderer.

## XSH Corpus Gate

Use the runnable-corpus integration test after changing core applets, native
tests, showcases, tools, benchmark scripts, or repository automation scripts.
It checks formatting and linting without rewriting files; intentional parser,
formatter, and runtime fixtures under `tests/fixtures/` are excluded.
Documentation fragments under `docs/snippets/` are excluded as well because
they may contain illustrative placeholders rather than complete programs.

```sh
cargo test --test integration runtime::coverage::runnable_xsh_corpus_is_formatted_and_lints_without_warnings
```

## Runtime Test Modules

| Area | File |
|---|---|
| collection values and methods | `tests/runtime/collections.rs` |
| coverage, lint, grep-adjacent tooling | `tests/runtime/coverage.rs` |
| frontend indexed fixtures | `tests/runtime/frontend_indexed.rs` |
| cataloged examples | `tests/runtime/examples.rs` |
| `core/pstree.xsh` process-tree output | `core/tests/test-pstree.xsh`, `tests/runtime/unix.rs` |
| interactive behavior | `tests/runtime/interactive.rs` |
| Linux-specific behavior | `tests/runtime/linux.rs` |
| standard modules | `tests/runtime/modules.rs` |
| OS-facing runtime behavior | `tests/runtime/os.rs`, `tests/runtime/unix.rs` |
| `run_capture`, `spawn_managed`, and process execution | `tests/runtime/process.rs`, `tests/runtime/run.rs` |
| retry blocks | `tests/runtime/retry.rs` |
| stack depth and explicit lowered frames | `tests/runtime/stack_depth.rs` |
| structured streams | `tests/runtime/streams.rs` |

## Fixture Locations

| Fixture | Purpose |
|---|---|
| `tests/fixtures/syntax` | parser and formatter fixture sources |
| `tests/fixtures/fmt` | annotated disk-backed formatter fixture and golden used by `tests/xsh/formatter.xsh` |
| `tests/fixtures/sema` | checker fixture sources |
| `tests/fixtures/runtime` | executable runtime fixture scripts |
| `tests/fixtures/frontend-indexed` | frozen indexed-execution and indexed-method fixtures |
| `examples` | standalone example scripts cataloged in `examples/catalog.json` |
| `showcase` and `showcase/tests` | larger standalone scripts and native tests |

## Commands To Avoid

- `make lint`, `cargo fmt`, `cargo fmt --all`, `cargo clippy --fix`, `xsht fmt`,
  and `xsht lint --fix` rewrite files and are left to the user.
- The `dist` profile is reserved for release packaging, not local agent
  verification.
- Benchmark and PGO commands intentionally use release code generation.
