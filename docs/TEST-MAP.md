# Test Map

Choose the narrowest useful command first, then run the broader gate for the
area touched. Agents do not run formatters or linters; leave gates that invoke
them to the owner and report that limit. Unfiltered `cargo test` includes
`runtime::coverage` cases and two `runtime::examples` cases that launch
`xsht fmt` or `xsht lint`, so agents use the filtered runtime gate below.

## Routine CI

`.github/workflows/verify.yml` runs on pull requests and pushes to `master`
with read-only repository permission. Both jobs use `DIST_PROFILE=dev` and
dispatch through `cargo dev test`, which calls `dev/main.xsh`:

| Runner | Target | Features selected by the XSH workflow |
| --- | --- | --- |
| macOS ARM64 | `aarch64-apple-darwin` | `net tools` in `dev/test_workflows.xsh::macos_ci`, plus default features |
| pinned Linux ARM64 musl image | `aarch64-unknown-linux-musl` | `linux-priv-tests net tools` in `dev/internal.xsh::linux_ci_test`, plus default features |

`dev/docker.xsh::internal_argv` forwards the selected target and profile into
the pinned container. The manual release workflow retains the `dist` profile;
ordinary CI uses debug products. These full CI tests include formatter and
linter checks and are owner-run under the agent workflow rule.

The release matrix in `.github/workflows/release.yml` matches the three
triples in `dev/targets.xsh::resolve`: x86_64 and aarch64 Linux musl, and
aarch64 Darwin. `dev/dist.xsh::native_dist` builds `xsh`, `xsht`, and `xshi`
for each target; `dev/release.xsh::validate_artifacts` requires all nine
binary artifacts and their checksum sidecars. The names and validation
boundary are covered by `dev/tests/test-targets.xsh`. Actual `dist` builds
and package smoke checks run in the manual release workflow.

`tests/linux_priv.rs` is included only with `linux-priv-tests`. In the pinned
privileged image it runs under the CI `dev` profile with `net tools`; use
`--nocapture` to see capability or fixture skip reasons. Rust counts those
early-return cases as passed, so record whether a privileged case actually ran.

## Common Gates

| Change | Narrow command | Broader gate |
|---|---|---|
| Rust compile only | `cargo build` | relevant filtered package tests; unfiltered `cargo test` is owner-run |
| `Lexer::lex_compact`, `Parser::parse_source_arena_only`, or formatter | targeted `cargo test --test integration syntax::TEST_NAME` | `cargo test --test integration syntax::` |
| `Checker::check_compact_declarations`, `Checker::probe_compact_bodies`, or lint | targeted `cargo test --test integration sema::TEST_NAME` for checker or `cargo test -p xsht --test integration lint::TEST_NAME` for lint | `cargo test --test integration sema::` for checker or `cargo test -p xsht --test integration` for lint |
| `Evaluator::prepare_compact_indexed_only`, `indexed_run`, or runtime behavior | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test --test integration runtime:: -- --skip runtime::coverage:: --skip runtime::examples::example_corpus_is_formatted --skip runtime::examples::example_corpus_lints_without_warnings --test-threads=1`; run relevant `runtime::coverage` tests by exact name only when they do not invoke formatters or linters |
| native XSH module behavior | `target/debug/xsht test --exact --jobs 1 PATH::TEST_NAME` | `target/debug/xsht test --jobs 1 tests/xsh/stdlib` |
| Raw host path, argv, and environment bytes | `target/debug/xsht test --exact --jobs 1 tests/xsh/stdlib/fs.xsh::test_fs_root_and_children_preserve_non_utf8_name` and `cargo test --test integration runtime::modules::helper_binaries_cover_raw_argv_env_path_and_glob_boundaries -- --exact` | Native stdlib gate plus the filtered runtime gate; the filesystem-name case skips on macOS because the host returns `EILSEQ` |
| Privileged Linux mount and `switch_root` behavior | In the pinned privileged `xsh-test` image: `cargo test -p xsh --target aarch64-unknown-linux-musl --test linux_priv --features linux-priv-tests linux_priv_mount_and_switch_root_fail_within_private_namespace -- --exact --nocapture` | Run the full `linux_priv` test binary in the same image with the target and feature flags |
| Privileged Linux loop lifecycle | In the pinned privileged `xsh-test` image: `cargo test -p xsh --target aarch64-unknown-linux-musl --test linux_priv --features linux-priv-tests linux_priv_loop_attach_list_and_detach_release_device -- --exact --nocapture` | Run the full `linux_priv` test binary and `xsht test tests/xsh/stdlib/linux.xsh` in that image |
| Linux live process descriptor lifecycle | In the pinned ARM64 `xsh-test` image: `target/debug/xsht test --exact tests/xsh/stdlib/linux.xsh::test_linux_open_files_tracks_a_live_child_descriptor` | The native Linux stdlib gate in the same image |
| One runtime fixture | `cargo test --test integration runtime::TEST_NAME` | the filtered runtime gate above |
| Process, network job, and stream-worker cancellation | `cargo test --test integration --features net runtime::process::sigterm_drains_process_net_job_and_parallel_workers_with_trace_parentage -- --exact` | Run the filtered runtime gate above with `--features net` on macOS and in the pinned Linux image |
| `xsht::cli::CliOutput`, `xsht::grep::find_matches_in_program`, or CLI/tooling | targeted `cargo test -p xsht --test integration cli::TEST_NAME` or `cargo test -p xsht --test integration grep::TEST_NAME` | `cargo test -p xsht --test integration` is owner-run: even the `cli::` group invokes `fmt` and `lint` |
| Copied `xsht` formatter/linter parity on script-backed calls in static and loaded modules | owner-run `cargo test -p xsht --test integration cli::copied_xsht_formats_and_lints_script_backed_calls_in_static_and_loaded_modules -- --exact` | The runnable-corpus gate below and the existing copied-product check/run test |
| Migrated API parity across `xsh` profiles | `cargo test -p xsht --test profile_parity -- --nocapture` after building the four debug/release and default/no-default products | Run the same test and builds in the pinned `Dockerfile.test` ARM64 musl image using `dev/targets.xsh::docker_test_env`; missing products print their build commands |
| Copied product and packaged core smoke | `tools/copied-product-smoke.py` with all three debug binaries and a `dev/release.xsh::package_core` archive | Repeat with the pinned Linux ARM64 musl debug products and `--linux`; `bench/stdlib-port/README.md` gives the commands |
| Benchmark workload | `cargo bench -p xshi --bench bench --features benchmark BENCHMARK -- --sample-count 1 --sample-size 1` | `cargo dev bench --fast` (memory/regression) or `cargo dev bench` (latency) |
| `xshi` editor input and repaint | `cargo test -p xshi --lib interactive::edit::tests` | `cargo test -p xshi` |
| `xshi` terminal geometry | `cargo test -p xshi --lib interactive::render::tests` | `cargo test -p xshi` and `cargo test --test integration runtime::interactive:: -- --test-threads=1` |
| `xshi` completion cache invalidation | `cargo test -p xshi --lib path_completion_refreshes` and `cargo test -p xshi --lib completion_refreshes_cwd_snapshot` | `cargo test -p xshi` |
| `xshi` remote completion | `cargo test -p xshi --lib remote_completion` | `cargo test -p xshi` |
| `xshi` PTY terminal lifecycle | `cargo test --test integration runtime::interactive::xshi_pty_restores_terminal_mode_on_exit -- --exact --test-threads=1` | `cargo test --test integration runtime::interactive:: -- --test-threads=1` |
| `xshi` single-job state | `cargo test -p xshi --lib single_background_job_rejects_second_slot` and `cargo test -p xshi --lib stopped_background_job_resumes_then_foregrounds` | `cargo test -p xshi` and the interactive PTY gate for foreground process-group signals |
| Non-Tokio archive/network dependency update | `cargo tree -i tokio` and `cargo tree -p xsh-net -e features` | focused archive or network runtime gate |
| Arena or indexed-IR layout | `scripts/ir-layout.py` (or `--only TYPE` for a focused report) | focused rustybench workload plus the applicable behavior tests |
| Frontend retained/peak accounting | `cargo test -p xsh --lib frontend_stats::tests` and `cargo run --bin xsh-frontend-stats -- --json tests/fixtures/frontend-indexed` | `cargo dev bench --fast` after the applicable syntax/checker gate |
| Frozen indexed fixtures and lexical shadowing | `target/debug/xsht test --jobs 1 tests/xsh/frontend-indexed.xsh` | The same native module plus `cargo test --test integration runtime::frontend_indexed:: -- --test-threads=1` for producer lifecycle fixtures |
| Runtime controller/worker allocation accounting | `cargo test -p xsh --lib runtime_stats::tests` and `cargo build -p xsh --bin xsh-runtime-stats` | `xsh-runtime-stats --json REPORT SCRIPT [-- ARGS...]` with matching output fingerprint, scoped worker attribution, and paired release host RSS |
| `FullBuilder::build_compact`, `FullVerifier::verify`, or executable IR | targeted `cargo test -p xsh --lib runtime::eval::indexed::full::tests::` | `cargo test -p xsh runtime::eval::indexed::full::tests --lib --features native-tests` |
| Explicit execution frames | targeted `cargo test --test integration runtime::stack_depth -- --test-threads=1` | `cargo test -p xsh runner::tests --lib --features native-tests` plus the runtime gate |
| Production executable runtime | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test -p xsh --test integration runtime:: --features native-tests -- --skip runtime::coverage:: --skip runtime::examples::example_corpus_is_formatted --skip runtime::examples::example_corpus_lints_without_warnings --test-threads=1` plus `cargo test -p xsh --test integration runtime::coverage::xsh_native_tests --features native-tests -- --exact` and `cargo dev bench --fast` |
| Syscall diagnostics | benchmark smoke test on the host | `cargo dev bench --syscalls` on Linux/Docker |
| LLVM IR size | `tools/llvm-lines-repeat-offenders.xsh` over an existing capture | fresh `cargo llvm-lines` capture plus the applicable behavior/benchmark gate |
| API registry/reference/examples | see `API Gate` below | same |
| Broad cross-cutting work | closest targeted tests | relevant filtered package tests; unfiltered `cargo test` is owner-run |
| Ambient filesystem authority policy | `cargo test --test ambient_fs_policy` | relevant filtered tests; unfiltered `cargo test --tests` is owner-run |

The PTY fixture keeps a master descriptor open while it spawns `xshi` and
other runtime tests may spawn children in parallel. Its master and duplicated
slave descriptors must have close-on-exec set at creation;
`runtime::interactive::xshi_pty_master_does_not_survive_exec` checks the
inherited-descriptor boundary in a child process.

As of 2026-09-25, `cargo dev bench --fast` stops while compiling the sibling
`../../rustybench` crate: its `allocator_api` use lacks the feature gate on the
pinned nightly toolchain. The command does not reach XSH's benchmark cases.
Until that sibling build is repaired, record paired workload samples and the
applicable behavior gates directly; a failed benchmark invocation is not a
performance pass.

## Native XSH Test Rule

Language behavior **must** be specified in the native XSH corpus, normally in
`tests/xsh/stdlib/` or the nearest `tests/xsh/*.xsh` module. Do not embed new
XSH source strings in Rust tests merely to exercise language behavior.

Rust integration tests are reserved for boundaries that native XSH cannot own:
fixture servers, exact process/byte lifecycles, PTYs, privileges, and platform
behavior. A Rust-owned boundary harness must invoke the relevant disk-backed
native XSH test and provide only its fixture inputs (for example URLs,
certificate paths, or temporary roots). Keep the assertions about the language
contract in XSH. Any exception requires an adjacent comment explaining why a
disk-backed native test cannot express the behavior.

## Executed Test Accounting

`docs/TEST-EXECUTION.json` records the tests that actually ran, along with each
skip reason and test ID. `tools/test-execution-report.py` checks that parsed
case lines agree with each completed gate's summary before writing the JSON.
The 2026-09-24 snapshot covers the full configured native `xsht test --jobs 1`
suite and the focused Rust `runtime::interactive` gate on macOS ARM64 and the
pinned Linux ARM64 musl image. It does not claim Rust suite-wide coverage.

The source inventory snapshot had 23 PTY `#[ignore]` attributes in
`tests/runtime/interactive.rs`, one intentionally ignored cold-start diagnostic
in `src/stdlib.rs`, and 35 `test.skip` call sites in tracked XSH tests. The
commented-out stress probe in `tests/runtime/os.rs` is not a registered test.
The current PTY suite has five active lifecycle cases and 17 opt-in cases;
the snapshot predates that change. Each opt-in case names its host requirement
at its `#[ignore]`. The cold-start probe is a manual measurement. Native
skips are conditional on platform, installed paths, and network fixtures, so
the JSON records observed counts separately for each host.

For the Linux native gate, bind the container-built
`target/aarch64-unknown-linux-musl/debug/xsh` over `/work/target/debug/xsh`.
The `dev/tests/test-lifecycle.xsh` fake tools use that path as a shebang;
without this bind, the shared macOS target directory supplies a Mach-O binary
and six test cases fail before reaching their assertions. Use the pinned
`Dockerfile.test` image for this gate.
The image does not install `make`; the Makefile facade test in
`dev/tests/test-lifecycle.xsh` reports an explicit skip there and runs on hosts
with `make` available.

## API Gate

```sh
cargo build -p xsh -p xshi -p xsht --bin xsh --bin xshi --bin xsht
cargo metadata --no-deps --format-version 1
cargo test --test integration libxsh_api
cargo test -p xsh-registry --lib
cargo test -p xsh --lib modules::signature
cargo test -p xsht --test api
target/debug/xsht api
target/debug/xsht api summary --format jsonl
target/debug/xsht check docs/snippets/api
cargo dev check
git diff --check
```

Run the relevant language or runtime test gate when the API contract or an
example exposes behavior that changed outside the registry and renderer. The
snippet directory check scans only that explicit directory, even when the
repository config has additional `include` roots.

## XSH Corpus Gate

Use the runnable-corpus integration test after changing core applets, native
tests, showcases, tools, benchmark scripts, or repository automation scripts.
It checks formatting and linting without rewriting files; intentional parser,
formatter, and runtime fixtures under `tests/fixtures/` are excluded.
Documentation fragments under `docs/snippets/` are excluded as well because
they may contain illustrative placeholders rather than complete programs.
This is an owner-run gate under the agent workflow rule above.

```sh
cargo test --test integration runtime::coverage::runnable_xsh_corpus_is_formatted_and_lints_without_warnings
```

## Runtime Test Modules

Language assertions and dry-run module contracts live in the nearest
`tests/xsh/` module. Rust runtime tests retain CLI, PTY, host fixtures,
process and signal lifecycles, raw bytes, allocation accounting, and small
stack boundaries.

| Area | File |
|---|---|
| collection aliasing and allocation traffic | `tests/xsh/stdlib/methods.xsh`, `tests/xsh/stdlib/map.xsh`, `tests/runtime/collections.rs` |
| coverage, lint, grep-adjacent tooling | `tests/runtime/coverage.rs` |
| frontend indexed fixtures | `tests/xsh/frontend-indexed.xsh`, `tests/runtime/frontend_indexed.rs` |
| `fs.walk`/`fs.files` options and walk value consumption | `tests/xsh/stdlib/fs.xsh` |
| cataloged examples | `tests/runtime/examples.rs` |
| `core/pstree.xsh` process-tree output | `core/tests/test-pstree.xsh`, `tests/runtime/unix.rs` |
| interactive behavior | `tests/runtime/interactive.rs` |
| Linux-specific behavior | `tests/xsh/stdlib/linux.xsh`, `tests/runtime/linux.rs` |
| standard modules | `tests/xsh/stdlib/module.xsh`, `tests/runtime/modules.rs` |
| embedded standard-module linkage and copied checker/runner binaries | `tests/stdlib_port.rs`, `tests/runtime/run.rs::copied_products_check_and_run_script_backed_calls_in_static_and_loaded_modules` |
| OS-facing runtime behavior | `tests/xsh/stdlib/unix.xsh`, `tests/runtime/os.rs`, `tests/runtime/unix.rs`, `tests/runtime/linux.rs` |
| `run_capture`, `spawn_managed`, and process execution | `tests/xsh/run.xsh`, `tests/xsh/stdlib/process.xsh`, `tests/runtime/process.rs`, `tests/runtime/run.rs` |
| retry blocks | `tests/xsh/retry.xsh` |
| stack depth and explicit lowered frames | `tests/runtime/stack_depth.rs` |
| structured stream behavior | `tests/xsh/stdlib/streams.xsh` |
| stream argv and signal process boundaries | `tests/runtime/streams.rs` |

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

- `cargo dev lint`, `cargo fmt`, `cargo clippy`, `xsht fmt`, and `xsht lint`
  are owner-run commands for agent work, including their fix variants.
- The `dist` profile is reserved for release packaging, not local agent
  verification.
- Benchmark commands intentionally use release code generation.
