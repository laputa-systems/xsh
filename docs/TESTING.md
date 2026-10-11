# Testing

Choose the narrowest useful command first, then run the broader gate for the
area touched. This file owns the gate commands; architecture and invariants are
in `docs/ARCHITECTURE.md`.

The consolidation batch also measures production source and structural ratchets
with `dev/consolidation/metrics.xsh`. It writes the current report and every
baseline rise to `.work/consolidation/current.json` and `rises.json`. Temporary
ceilings are explicit in `dev/consolidation/envelope.json`, with approval, reason
and removal condition for each metric. Missing or unknown limits fail. Final
closure rejects that envelope, uses the unchanged starting baseline, and requires
independently reviewed semantic evidence as well as mechanical counts.

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
| tooling | `crates/xsht/tests/` (`integration`, `profile_parity` targets) |
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
  example `cargo test --profile verification --target "$TARGET" --test integration runtime::process::`.

## Gates

Run process and native suites with the optimized `verification` profile. It
uses no LTO and 64 codegen units to shorten rebuilds. Debug `--lib` tests retain
checks compiled only under debug assertions; release is reserved for final
verification and performance measurements. `xsht` runs its sibling `xsh`.
Linux development uses the native musl target inside `Dockerfile.test`; the
final Linux gate uses `aarch64-unknown-linux-musl`. Set `TARGET` for direct
commands, or use `cargo dev gate` to select target-correct commands:

```sh
export TARGET=x86_64-unknown-linux-musl
cargo build --profile verification --target "$TARGET" -p xsh --bins -p xsht --bin xsht
target/$TARGET/verification/xsht test tests/xsh/retry.xsh            # one file
target/$TARGET/verification/xsht test --exact tests/xsh/retry.xsh::test_retry_repeats_until_attempt_succeeds
target/$TARGET/verification/xsht test                                 # full native suite
```

Process tests accept only verification and release binaries. Every Rust test target that spawns `xsh`,
`xsht`, `xshi`, `xsh-fuzz`, or `xsh-test-helper` resolves it through
`test_bin!` (`tests/test_binary.rs`), which fails with the verification command
when Cargo built the test, and so the binary, in an unsupported profile. Run those
targets with `cargo test --profile verification --target "$TARGET"`; Cargo builds their binaries in
`target/$TARGET/verification`. Unit tests (`--lib`) spawn nothing and run in debug, which
also compiles the checks that exist only under debug assertions; the root
package's unit-test target currently also crashes LLVM (unbounded
`ScalarEvolution` recursion) when built with `--release`, so name the root
integration targets with `--test` instead of a bare `cargo test --profile verification --target "$TARGET"`. Use
debug builds for compile checks, and build only the package you need
(`cargo build --profile verification --target "$TARGET" -p xsht --bin xsht`, not a bare workspace build).

On a musl host (Alpine) `rustc` allocates through musl's allocator and a
thin-LTO build spends most of its time in the kernel. `cargo dev` therefore
preloads jemalloc into its build steps there: when `rustc -vV` reports a
`-musl` host and `/usr/lib/libjemalloc.so.2` exists, each step that only
compiles runs with it in `LD_PRELOAD`, and each `cargo test` step first
builds its executables with `--no-run` under the preload (stage `NAME-build`)
and then runs them without it, so no test or spawned binary inherits the
library. `XSH_DEV_BUILD_PRELOAD` names another library, and set to the empty
string disables the preload; a missing library means none and is not an
error. Steps that run what they build (`cargo run`, `cargo bench`) are never
preloaded, and neither is a `cargo` you type yourself: prefix a manual build
with `LD_PRELOAD=/usr/lib/libjemalloc.so.2`, and never a test run. The policy
is `dev/cargo_steps.xsh`.

| Change | Narrow | Broader |
|---|---|---|
| parser, CST, formatter | `cargo test --profile verification --target "$TARGET" --test integration syntax::NAME` | `cargo test --profile verification --target "$TARGET" --test integration syntax::`; `tests/xsh/formatter.xsh` |
| grammar (`src/syntax/grammar.rs`) | `cargo test --profile verification --target "$TARGET" --test integration syntax::grammar::` | `cargo test --profile verification --target "$TARGET" -p xsh-fuzz --test soundness generated_programs_are_grammar_sentences`; `make docs-check` |
| checker | `target/$TARGET/verification/xsht test tests/xsh/checker-NAME.xsh` (accepted and rejected programs by topic: `commands`, `modules`, `records`, `results`, `signal-hooks`, `streams`) or the feature's own `tests/xsh/NAME.xsh`; `cargo test --profile verification --target "$TARGET" --test integration sema::NAME` for checker facts with no command-line surface (full-versus-compact fact comparisons, expression types, interactive mode, fix hints) | `target/$TARGET/verification/xsht test tests/xsh`; `cargo test --profile verification --target "$TARGET" --test integration sema::` |
| sugar forms, `xsht desugar` | `cargo test -p xsht --lib sugar_expansion_tests`; `cargo test -p xsht --lib desugar_tests` | `cargo test --profile verification --target "$TARGET" -p xsht --test integration desugar::` (desugars every native test file that holds sugar, in a copy of the workspace, and requires the same check diagnostics and test results) |
| lowering, verifier | `cargo test -p xsh --lib runtime::eval::indexed::full::tests::NAME` | `cargo test -p xsh --lib runtime::eval` |
| lowering vs checker types | `cargo test -p xsh --lib corpus_lowering_agrees_with_checked_types` (lowers the corpus and embedded stdlib; debug builds report any disagreement with the checker) | `tests/xsh/lowering-coverage.xsh`; full native suite |
| runtime | native module, then `cargo test --profile verification --target "$TARGET" --test integration runtime::NAME` | `cargo test --profile verification --target "$TARGET" --test integration runtime:: -- --skip runtime::coverage:: --skip runtime::examples::` |
| frames, stack depth | `cargo test --profile verification --target "$TARGET" --test integration runtime::stack_depth` | runtime gate |
| lint, tooling | `target/$TARGET/verification/xsht test tests/xsh/tooling-NAME.xsh` (the `check`, `fmt`, `lint`, and `test` command lines, the lint migration fixes, and help); `cargo test --profile verification --target "$TARGET" -p xsht --test integration lint::NAME` | `cargo test --profile verification --target "$TARGET" -p xsht` |
| API, registry, docs | `target/$TARGET/verification/xsht test tests/xsh/api-tool.xsh`; `cargo test --profile verification --target "$TARGET" -p xsht --test integration api::` | API gate below |
| standard modules | `target/$TARGET/verification/xsht test tests/xsh/stdlib/NAME.xsh` | `target/$TARGET/verification/xsht test tests/xsh/stdlib` |
| retained frontend memory | `cargo test -p xsh --lib frontend_stats::tests` | `target/$TARGET/verification/xsht frontend-stats --json tests/fixtures/frontend-indexed` |

`cargo dev gate PATH...` selects the gate families owned by the changed paths,
deduplicates them, and runs focused native modules plus the corresponding hard
checks. With no paths it uses tracked and untracked Git changes. Unmapped paths
fail before verification starts. `cargo dev gate --batch` runs the complete
process, debug, tooling, fixed-seed soundness, native and docs
sequence once. Neither command invokes source-tree formatters or autofixers.
Public Linux gate commands enter `Dockerfile.test`; hidden internal gate
operations run the same plan inside the container without another Docker call.
Changed fixtures select their compiler owner rather than running as native test
modules. Embedded catalog changes also run the debug catalog preparation test.
An unset `DIST_PROFILE` selects verification for tests; an explicit value must
be `verification` or `release`, independently of the packaging default.

API gate: `cargo test --profile verification --target "$TARGET" --test integration libxsh_api`,
`cargo test -p xsh-registry`, `cargo test -p xsh --lib modules::signature`,
`cargo test --profile verification --target "$TARGET" -p xsht --test integration api::`,
`target/$TARGET/verification/xsht test tests/xsh/api-tool.xsh`, and
`target/$TARGET/verification/xsht check docs/snippets/api`.

Repository gates (owner-run unless the task asks for them):

Compatibility sources under `dev/compat/**/*.xsh` are excluded from automatic
check and lint while that work is in progress. Consolidation lane and batch
gates omit the compatibility ratchets; the tools and their explicit tests stay
available for resuming that work. The integrator owns the corresponding
`xsht-config.ini` exclusion.

Automatic root syntax gates skip only
`syntax::formatter_is_idempotent_on_laputa_corpus`; automatic xsht integration
gates skip only
`lint_format_invariance::formatting_preserves_lints_on_laputa_corpus` for the
sibling policy. Both rewrite or repeatedly analyze the complete sibling
repository. Their declarations remain available for explicit invocation;
migrated sibling files receive focused check/lint from the integrator. Repository
corpus checks and the existing repository lint budget remain active.

The compatibility ratchets are `python3 dev/compat/check_ignored_options.py`
(no new or grown discard bucket in `core/*.xsh`),
`python3 dev/compat/check_kernel_reads.py` (no `/proc` or `/sys` literal in a
top-level applet: kernel state is read through one typed domain API per ABI),
`python3 dev/compat/check_exclusions.py` (Gate 3 exclusions are exact test IDs with
a category and a reason), and
`python3 dev/compat/parity.py --check` (the committed parity manifest matches
the repository and the denominator pinned in `dev/compat/upstream.lock.json`;
offline unless `UUTILS_ROOT` is set). Harness usage is in
`dev/compat/README.md`.

| Command | Runs |
|---|---|
| `cargo dev check` | release product build, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, release `xsht check`, `xsht fmt --check`, and `xsht lint`, `check-docs` (with release binaries), `git diff --check` |
| `make check` (`cargo dev check lint`) | release `xsht lint` on the repository with no diagnostics within a 15 s budget (`crates/xsht/tests/lint_performance.rs`), then `check-docs` with release binaries |
| `make test` (`cargo dev test`) | filtered root process targets, debug `--lib`, `xsht` integration, fixed-seed soundness, then one full native suite |
| `cargo dev test xsh` | build verification products for the selected target, then one full native suite |
| `make fuzz` | `xsh-fuzz all` for 120 s on release (`FUZZ_DURATION` overrides); not part of `make check` |
| `make docs` (`cargo dev docs`) | build release `xsh` and `xsht`, then regenerate every file rendered from `docs/templates/`, and `docs/user-tour.html` |
| `make docs-check` (`cargo dev docs check`) | the same render into memory, failing with the list of stale files; then `xsht check` on each snippet in `docs/snippets/spec` and `docs/snippets/tour`, and `xsht test` in `docs/snippets/tour/project` |

Generated docs: `docs/SPEC.md`, `docs/user-tour.md`, and `docs/reference/*.md`
are rendered by `dev/docs.xsh` from `docs/templates/`; edit the template or the
snippet in `docs/snippets/spec/` or `docs/snippets/tour/`, run `make docs`, and
commit both. A snippet that is a fragment wraps the shown lines in
`# begin example` / `# end example` inside a program that checks; an example
that must fail lives in `docs/snippets/spec/rejected/` with a `# error: CODE`
comment on each failing line. A template that shows
`{{.spec.NAME.output}}` or `{{.tour.NAME.output}}` makes `make docs` run that
snippet; other output blocks are literal template text. The repository's
`xsht check`, `xsht lint`, and `xsht fmt --check` cover the snippets outside
`rejected/` (excluded in `xsht-config.ini`). A region drops the blank lines at
its edges, so a blank line the formatter puts before `# end example` never
shows.
`dev/tests/test-docs.xsh` covers the generator.

`docs/user-tour.html` is the tour as one self-contained page. It is not a
template: `dev/tour_html.xsh` renders it from the rendered `docs/user-tour.md`
(`make docs` writes it and `make docs-check` fails when it is stale), coloring
XSH code with `xsht highlight`. The renderer accepts only the Markdown subset
the tour uses and fails on anything else, so a new construct in the tour needs
renderer support in the same change. `dev/tests/test-tour-html.xsh` covers it.

`cargo test --profile verification --target "$TARGET" -p xsht --test integration lint_format_invariance::`
checks that lint diagnostics are identical before and after formatting on the
repository corpus, layout perturbations of it, and the Laputa monorepo `../laputa` (or
`XSH_LAPUTA_CORPUS`) when present; it formats only temporary copies. The
repository baseline is linted once per run and shared, each baseline lint
overlaps the rewritten copy's, and perturbation spreads files over a bounded
thread pool; the largest file (`dev/system_report_check.xsh`) sets its floor
of about 16 s.

## Grammar proofs

`syntax::grammar::` (in `tests/grammar.rs`) holds the productions of
`src/syntax/grammar.rs` and the parser to the same language, in both
directions, on every run:

- **Generation.** Sentences generated from the productions at fixed seeds and
  depths, plus a few per rule that take the shortest way to that rule, must
  parse with no diagnostics. A candidate that misses a lookahead or lexes into
  different tokens is not a sentence and is filtered by the recognizer; at
  least 80% of candidates must be sentences, and every rule must appear in
  one.
- **Recognition.** An Earley recognizer over the same productions must accept
  the token stream of every repository `.xsh` file that parses without
  diagnostics, and
  `xsh-fuzz --test soundness generated_programs_are_grammar_sentences` checks
  every fuzz-generated program at the soundness seed set.

`grammar_tokens` prepares the token stream as the parser reads it (comments
dropped, continuation lines joined), and sources the parser rejects must be
rejected too. The whole module runs in about a second. When a
generated sentence fails to parse, either the production is wrong or the
parser has a bug; `xsht grammar` prints the productions.

## Soundness fuzzing

Well-typed programs must not go wrong. `cargo test --profile verification --target "$TARGET" -p xsh-fuzz
--test soundness` checks this at a fixed seed set. Generated programs must check, run
without a runtime or internal error, and print exactly what the reference
evaluator (`xsh_fuzz::eval`) predicts. Registry probes the checker accepts must
run without internal errors, and mutants of generated and corpus programs must
get ordinary diagnostics with valid spans. A failure prints a minimized
program.

`make fuzz` explores fresh seeds, starting each campaign from the clock's
nanoseconds so consecutive campaigns do not replay overlapping seed ranges.
Generated programs run under
`proc fuzz_main() []`, so the checker proves them free of host effects, and
each runs in a child with a cleared environment, a temp directory, a timeout,
and an output cap. Memory is bounded: at most `--jobs` workers (default half
the CPUs, at most 4) each handle 500 seeds and exit. A worker above 384 MiB or
a program above 256 MiB of sampled physical footprint is killed and reported
(macOS enforces no data rlimit). Failures are written to
`target/fuzz/<sha256>/failures/<seed>.xsh` by `make fuzz`, where `<sha256>`
hashes the tested `xsh-fuzz` executable (including its linked XSH frontend,
runtime, and tooling). Repeated runs of the same binary share reproducers;
changed binaries get separate directories, preserving earlier evidence.
Direct `xsh-fuzz` campaigns default to `target/fuzz/failures` and accept
`--out DIR`; `xsh-fuzz reduce FILE` minimizes a frontend failure.

## Linux

All Linux building and testing runs in the image defined by `Dockerfile.test`
(`xsh-test`). Development uses the host-native musl target; final verification
uses `aarch64-unknown-linux-musl`, with the flags in
`dev/targets.xsh::docker_test_env`. Run `cargo dev test linux` (privileged,
`make test-linux`) or `cargo dev test linux --ci`. CI (`.github/workflows/`)
runs the same image. A build outside that image is not evidence about Linux.
The driver passes `--init` so orphaned stopped jobs are reaped. For a native
gate inside the image, run the container-built
`target/aarch64-unknown-linux-musl/release/xsht` for final verification.
The image includes Python 3 for native tests that run `dev/compat/stage.py`,
util-linux `setpriv` for tests that drop to an unprivileged user, and a `compat`
account with UID/GID 1000 for those runs. Bash, make, Perl, autotools, the
uutils GNU preparation tools, and ACL/capability/crypto headers support the
GNU compatibility suite and runner.

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
- Unfiltered `cargo test --profile verification --target "$TARGET"` in agent work: `runtime::coverage` runs
  the whole native suite (`xsh_native_tests`), and two `runtime::examples`
  cases launch `xsht fmt`/`xsht lint`. Use the filtered runtime gate. The
  native tooling tests (`tests/xsh/tooling-*.xsh`) run `xsht fmt` and
  `xsht lint --fix` only on files in their own temp directories.
- Debug `cargo test` of a target that spawns binaries: each such test fails
  with the `cargo test --profile verification --target "$TARGET"` instruction instead of running.
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
A hung native test fails as `TIMEOUT` after `xsht test --timeout` (default
120 s); a test that legitimately runs longer calls `test.timeout(ctx, limit)`.

## Coverage

`cargo dev coverage [--backend native|docker]` (`make cov`) is the source of
truth for combined Rust LLVM and XSH API coverage; automatic execution uses the pinned Docker image. The native backend is for
callers already inside that image. Coverage runs the same process, debug,
tooling, fixed-seed soundness, privilege, and native oracles as the complete
gate, with the selected target and profile. Compiler preload stays confined to
build steps; test processes never inherit it. Executable discovery includes
the target profile's nested Cargo output directories. For XSH source coverage, run `xsht test --cov`;
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
- `xsht runtime-stats --json REPORT SCRIPT` reports construction, controller,
  and `par-map` worker allocation; pair memory claims with a host RSS check.
  It and `xsht frontend-stats` are not listed in `xsht --help`.
