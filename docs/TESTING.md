# Testing

Choose the narrowest useful command first, then run the broader gate for the
area touched. This file owns the gate commands; architecture and invariants are
in `docs/ARCHITECTURE.md`.

## Native tests first

Language behavior is specified in the native XSH corpus: a `test NAME { ... }`
declaration in the nearest `tests/xsh/*.xsh` module, `tests/xsh/stdlib/*.xsh`
for standard modules, `core/tests/test-*.xsh` for applets,
`dev/tests/test-*.xsh` for repository automation, and
`showcase/tests/test-*.xsh` for showcase programs. Use `test.run_script`,
`test.run_xsh`, `test.run_xsht_trace`, temporary resources, and mocks to reach
process and CLI behavior from XSH.

Rust tests are reserved for boundaries native XSH cannot own: fixture servers,
exact process or byte lifecycles, PTYs, privileges, platform behavior,
allocation accounting, and small-stack limits. A Rust harness for such a
boundary supplies fixture inputs and invokes the disk-backed native test;
assertions about the language stay in XSH. Embedding XSH source in Rust needs an
adjacent comment explaining why a native test cannot express it.

| Suite | Location |
|---|---|
| native XSH | `tests/xsh`, `core/tests`, `dev/tests`, `showcase/tests` (the `test_roots` in `xsht-config.ini`) |
| root integration | `tests/integration.rs` aggregates `tests/syntax.rs`, `tests/sema.rs`, `tests/runtime/`, `tests/cli.rs`, and the rest |
| separate root targets | `tests/ambient_fs_policy.rs`, `tests/symbol_plateau.rs`, `tests/linux_priv.rs` (`linux-priv-tests` feature) |
| tooling | `crates/xsht/tests/` (`integration`, `api`, `profile_parity` targets) |
| interactive | `crates/xshi/tests/`, `tests/runtime/interactive.rs`, `tests/runtime/interactive/parity/` |
| fixtures | `tests/fixtures/{syntax,sema,runtime,fmt,frontend-indexed}` |

## Finding tests

- `xsht test --list FILTER` lists matching native tests. `FILTER` is a path
  prefix (`tests/xsh/stdlib`) or a test-name substring (`map_iteration`);
  `--exact PATH::TEST_NAME` selects one test.
- Native test files are named for the feature (`tests/xsh/retry.xsh`) or the
  module (`tests/xsh/stdlib/json.xsh`); applet, automation, and showcase
  tests are named `test-` plus the program name in the sibling `tests/`
  directory (`core/tests/test-pstree.xsh`).
- `xsht grep` and ordinary text search over `tests/` find existing coverage for a
  symbol or diagnostic code. Rust test names are filterable substrings, for
  example `cargo test --test integration runtime::process::`.

## Gates

Run native suites on release binaries; they are several times faster than
debug and `xsht` runs the sibling `xsh` from its own directory:

```sh
cargo build --release -p xsh --bins -p xsht --bin xsht
target/release/xsht test tests/xsh/retry.xsh            # one file
target/release/xsht test --exact tests/xsh/retry.xsh::test_retry_repeats_until_attempt_succeeds
target/release/xsht test                                 # full native suite
```

Use debug builds for Rust gates and compile checks, and build only the package
you need (`cargo build -p xsht --bin xsht`, not a bare workspace build).

| Change | Narrow | Broader |
|---|---|---|
| parser, CST, formatter | `cargo test --test integration syntax::NAME` | `cargo test --test integration syntax::`; `tests/xsh/formatter.xsh` |
| checker | `cargo test --test integration sema::NAME` | `cargo test --test integration sema::` |
| lowering, verifier | `cargo test -p xsh --lib runtime::eval::indexed::full::tests::NAME` | `cargo test -p xsh --lib runtime::eval` |
| runtime | native module, then `cargo test --test integration runtime::NAME` | `cargo test --test integration runtime:: -- --skip runtime::coverage:: --skip runtime::examples::` |
| frames, stack depth | `cargo test --test integration runtime::stack_depth` | runtime gate |
| lint, tooling | `cargo test -p xsht --test integration lint::NAME` | `cargo test -p xsht --test integration` |
| API, registry, docs | `cargo test -p xsht --test api` | API gate below |
| standard modules | `target/release/xsht test tests/xsh/stdlib/NAME.xsh` | `target/release/xsht test tests/xsh/stdlib` |
| retained frontend memory | `cargo test -p xsh --lib frontend_stats::tests` | `cargo run --bin xsh-frontend-stats -- --json tests/fixtures/frontend-indexed` |

API gate: `cargo test --test integration libxsh_api`,
`cargo test -p xsh-registry`, `cargo test -p xsh --lib modules::signature`,
`cargo test -p xsht --test api`, and `target/release/xsht check docs/snippets/api`.

Repository gates (owner-run unless the task asks for them):

| Command | Runs |
|---|---|
| `cargo dev check` | product build, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `xsht check`, `xsht fmt --check`, `xsht lint`, the runnable-corpus test, `git diff --check` |
| `make check` (`cargo dev check lint`) | release `xsht lint` on the repository with no diagnostics within a 15 s budget (`crates/xsht/tests/lint_performance.rs`) |
| `make test` (`cargo dev test`) | debug `cargo test` |
| `cargo dev test xsh` | the native suite through `cargo run -p xsht` |
| `make fuzz` | the fuzz targets in `crates/xsh-fuzz` |
| `make docs` / `make docs-check` | regenerate generated docs / fail if they are stale (`make check` also fails on stale generated docs) |

`cargo test -p xsht --test integration lint_format_invariance::` checks that
lint diagnostics are identical before and after formatting on the repository
corpus, layout perturbations of it, and `../packages` (or
`XSH_PACKAGE_CORPUS`) when present; it formats only temporary copies.

## Linux

All Linux building and testing runs in the image defined by `Dockerfile.test`
(`xsh-test`), target `aarch64-unknown-linux-musl`, with the flags in
`dev/targets.xsh::docker_test_env`. Run `cargo dev test linux` (privileged,
`make test-linux`) or `cargo dev test linux --ci`. CI (`.github/workflows/`)
runs the same image. A build outside that image is not evidence about Linux.
The driver passes `--init` so orphaned stopped jobs are reaped. For a native
gate inside the image, bind the container-built
`target/aarch64-unknown-linux-musl/debug/xsh` over `target/debug/xsh`, since
`dev/tests/test-lifecycle.xsh` uses that path as a shebang.

## Interactive parity

`tests/runtime/interactive/parity/` replays scenarios against `xshi` in an
isolated `HOME` and compares transcripts with goldens recorded from `ish`
(`tests/fixtures/interactive-parity/<os>/`). The gate needs no `ish` install.
`XSHI_PARITY_ISH_BIN=/path/to/ish` also runs `ish` and requires
ish == golden == xshi; adding `XSHI_PARITY_RECORD=1` rewrites goldens;
`XSHI_PARITY_FULL=1` prints whole transcripts on mismatch. Scenarios pin locale,
`XSHI_PROFILE_PATH=/dev/null`, and `XSHI_HOSTNAME=sentry`; record `ish` where
its host name is also six characters (`docker run --hostname sentry`).
Behavior `ish` lacks is tested in `tests/runtime/interactive.rs`.

## What not to run

- Formatters and autofixers: `cargo fmt`, `cargo clippy --fix`,
  `cargo dev lint --fix` (`make lint`), `xsht fmt`, `xsht lint --fix`. They
  rewrite unrelated files; formatting is the owner's responsibility. Check-only
  forms are fine.
- Unfiltered `cargo test` in agent work: `runtime::coverage` and two
  `runtime::examples` cases launch `xsht fmt`/`xsht lint`. Use the filtered
  runtime gate.
- The `dist` profile, bare `cargo build --release`, and more than one full
  native suite at a time. Never leave test processes running.

## Flakes and skips

A flaky test is a bug: find the race or the unstable assertion rather than
retrying. Assert on rendered fragments (`exited with status 127`), never bare
numbers, because nested-run stderr embeds PIDs and timestamps. Native skips use
`test.skip(reason)` and must be conditional on platform, installed tools,
privilege, or fixture availability. Rust tests that return early for a missing
capability count as passed, so report whether a privileged case actually ran
(`--nocapture` shows the reason).

## Coverage

`cargo dev coverage [--backend native|docker]` (`make cov`) is the source of
truth for combined Rust LLVM and XSH API coverage; on ARM hosts the Docker
backend uses the pinned image. For XSH source coverage, run `xsht test --cov`;
add `--api` for API hits and `--cov-json FILE` for the machine-readable report.
The line denominator counts parsed executable statements in every configured
file, including files no test loads; `proc entries` reports only whether each
callable was entered. Exclude source families with `[coverage] exclude`.
Prefer tests that prove workflows and host contracts over branch-only tests;
dangerous platform operations without an isolation harness may stay uncovered.

## Benchmarking

Benchmarks use release code generation; run them serially with paired
settings, and treat single-run latency changes as inconclusive.

- `cargo dev bench` runs the rustybench `xshi` suite
  (`crates/xshi/benches/bench.rs`, `benchmark` feature); `--fast` is an
  allocation signal, not a latency measurement. Baselines are machine-local
  and ignored.
- `cargo dev bench --syscalls` and
  `xsht trace --syscalls --trace-top-syscalls 10 SCRIPT` detect unexpected
  subprocesses or kernel work.
- `bench/stdlib-port/run.py` and `bench/stdlib-port/tooling.py` measure
  end-to-end `xsh` and `xsht` latency; `bench/stdlib-port/README.md` records
  dispositions.
- `xsh-runtime-stats --json REPORT SCRIPT` reports construction, controller,
  and `par-map` worker allocation; pair memory claims with a host RSS check.
