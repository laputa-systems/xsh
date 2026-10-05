# XSHT Tooling

`xsht` is the tooling frontend for XSH source: checking, formatting, linting,
native tests, API queries, tracing, structural search, and refactoring. Script
execution stays in `xsh`; `xsht` evaluates only when a command (test, trace)
requires it. `crates/xsht/src/commands.rs` declares every command's options
once: the argument parser matches only those declarations, and `xsht help`,
`xsht help COMMAND`, and the usage lines are generated from them, so they are
the authoritative option reference.

## Commands

| Command | Does | Owner |
|---|---|---|
| `xsht check [PATH...]` | parse, load modules, type-check; `--annotate[=POLICY]` writes inferred annotations; `--summary` counts diagnostics by code | `crates/xsht/src/cli/check.rs` |
| `xsht fmt [--check] [FILE...]` | format through the checked program (`docs/XSHT-FMT.md`) | `crates/xsht/src/cli/fmt.rs` |
| `xsht lint [--fix] [--only RULE,...] [--runless] [FILE...]`, `xsht lint --list` | non-fatal quality diagnostics and conservative autofixes; the code catalog | `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/lint.rs` |
| `xsht test [FILTER]` | discover and run native `test NAME { ... }` declarations | `crates/xsht/src/xsht/test.rs` |
| `xsht api [QUERY...]` | query language and standard-library metadata | `crates/xsht/src/api.rs`, `crates/xsht/src/cli/api.rs` |
| `xsht trace SCRIPT` | run with structured tracing (text, jsonl, flamegraph, syscall totals) | `crates/xsht/src/trace.rs`, `crates/xsht/src/cli/trace.rs` |
| `xsht grep` / `xsht refactor` | AST-pattern search and span-based rewrite | `crates/xsht/src/grep.rs`, `crates/xsht/src/cli/refactor.rs` |
| `xsht ast SCRIPT` | parser debug output | `crates/xsht/src/cli/syntax_tree.rs` |
| `xsht highlight SCRIPT` | syntax highlighting runs as JSON Lines, from the lexer (`src/syntax/highlight.rs`) | `src/syntax/highlight.rs`, `crates/xsht/src/cli/highlight.rs` |
| `xsht grammar [--format ebnf\|json]` | the language's productions as EBNF, or the JSON reference that `make docs` renders | `src/syntax/grammar/reference.rs`, `crates/xsht/src/app.rs` |

Dispatch starts in `xsht::app::main` (`crates/xsht/src/app.rs`); every command
returns a `CliOutput` (`crates/xsht/src/cli/mod.rs`). Commands consume the
shared checked-program bundle from `src/loader.rs` through the `xsh::frontend`
façade rather than building parallel parser/checker pipelines. Shared parse and
check diagnostics are deduplicated per command, so an imported module's error
is reported once.

`xsht check` and `xsht lint` end their stderr with one timing line whenever
they processed a file, for example
`xsht check: 301 files in 4.75s (thread time by stage: discover 0.03s, load 7.16s, check 30.90s, lower 6.06s)`.
A stage sums the time of every worker thread that ran it, so stages can add up
to more than the wall-clock total; a consumer that compares stderr drops that
last line (`StageTimings`, `crates/xsht/src/cli/timing.rs`).

## Configuration

`xsht-config.ini` is resolved per file from the nearest ancestor; the current
directory's config controls no-argument discovery. Relative paths resolve from
the config's directory. A missing file means defaults; an invalid file is a
command error.

| Key | Meaning |
|---|---|
| `include` | extra roots for no-argument discovery |
| `exclude` | glob patterns removed from discovery for path-oriented commands; an explicit directory uses its nearest config's `exclude` |
| `module_path` | module search roots (default `.`), searched after file-relative lookup and `XSH_MODULE_PATH`; `xsh` and `xshi` read this one key for the entry script through the same `project_module_roots` (`src/project.rs`); `xsht test` also passes the roots to `module.load` and appends them to children's `XSH_MODULE_PATH` |
| `test_roots` | directories `xsht test` searches |
| `[format] line-width` | formatter width target (default 120) |
| `[format] exclude` | glob patterns, matched from the discovery root, that `xsht fmt` skips during discovery; files named explicitly are still formatted |
| `[check] annotate` | default `--annotate` policy |
| `[lint] prefer-inferred-pure-returns`, `prefer-inferred-private-effects` | opt-in removal of annotations the checker can infer |
| `[lint] prefer-inferred-variants`, `prefer-positional-constructors` | opt-in `lint.prefer-inferred-variant` (drop a variant qualifier the expected type selects) and `lint.prefer-positional-constructor` (pass in-order constructor fields positionally) |
| `[lint] prefer-implicit-messages` | opt-in `lint.prefer-implicit-message`: a variant declared `V(message: Str)` drops its payload and takes the message positionally. Calls that name `message:` are fixed first; the declaration is fixed only when the family is not exported, because calls in importing files are not visible |
| `[lint] runless-except` | commands allowed under `--runless` |
| `[dead-code] exclude` | files exempt from `lint.dead-code` and `lint.unused-callable` |
| `[coverage] exclude` | files removed from the `xsht test --cov` denominator only |

## Lint

Lints analyze the checked program and its published facts; they never re-derive
types or effects from syntax. Every diagnostic code, from any stage, is a
variant of `xsh::diagnostic::DiagnosticCode`, declared once with its stable
name, family, default severity, one-line summary, and whether a non-lint
code's fix is safe for `xsht lint --fix` (`fixable`). `--only` accepts every lint code plus the fixable codes of other
stages (`DiagnosticCode::lint_selectable`), and
`xsht lint --list [--format text|jsonl]` prints that catalog, which
`make docs` renders into `docs/reference/lints.md`. `--only` restricts both
reports and fixes; an unknown code is a usage error that suggests the nearest
selectable code. A scoped fix splices its
exact edits with no formatting pass (`apply_cst_fixes`), while unrestricted
`--fix` formats the rewritten file.

Lint selects true entry roots during directory discovery (files with no inbound
import; one deterministic root per import cycle; explicitly named files always),
lints imported modules only through those roots, and checks each reachable
bundle once. `lint.dead-code` and `lint.unused-callable` are proof-oriented
reachability warnings over that bundle and never auto-delete. Workers are
bounded (at most four) and run on `FRONTEND_WORKER_STACK_BYTES` stacks.

A safe fix:

- is a non-overlapping source replacement over an original span, applied
  through the CST (`apply_cst_guarded_edits`);
- skips spans containing comments unless the rule handles them;
- leaves the file parsing, resolving, formatting, and with no new checker
  diagnostics (existing diagnostics may remain);
- converges: rounds repeat until no fix applies, and the final round's
  diagnostics are the ones reported;
- keeps exactly the parentheses the parser needs where it lands: replacements
  are built with conservative grouping, and `minimize_fix_grouping` removes
  every pair `check.redundant-parens` would reject.

When a rule cannot preserve nearby comments, it reports without a fix. Lint
diagnostics are invariant under formatting; a rule that measures source shape
measures the formatter's spelling. Shapes the formatter owns are not lint rules.
`check.redundant-parens` is the one diagnostic formatting removes: it never
blocks `fmt` or hides lint diagnostics, and formatted output has none
(`lint_format_invariance`). `check.mixed-logical` still blocks `fmt`, which
never chooses a grouping silently.

`lint.error-fallback-block` retains multiline error expressions inside the lazy
handler block when replacing an identity-success `Result` match.

## Native tests

`xsht test` discovers explicit `test NAME { ... }` declarations under
`test_roots`, prepares each file through the same verified indexed program as
`xsh`, and reports lowering failures as failed tests with the `xsht check`
diagnostic. `FILTER` matches a path prefix or a test-name substring; `--exact`
takes `PATH::TEST_NAME`. `--list`, `--fail-fast`, `--keep-temp`, and
`--nocapture` behave as named. Child output is captured per test and shown on
failure.

Without `--jobs`, half the logical CPUs run concurrently (1 to 8). Each
`test.run_script`, `test.run_xsh`, and `test.run_xsht_trace` child leads a
runner-owned process group; on SIGINT/SIGTERM the runner sends SIGTERM to those
groups, waits one second, sends SIGKILL, and exits `128 + signal`.

Each test has a time limit: `--timeout DURATION` (an XSH duration literal such
as `30s` or `5m`; default `120s`; `0` or `none` disables it), or the limit the
test sets with `test.timeout(ctx, limit)`, which wins unless the run disabled
timeouts. A test that overruns is canceled through its evaluator: its first
checkpoint raises a `canceled` error so its defers run, and its own child
process groups receive SIGTERM and then SIGKILL. A test still running five
seconds later is aborted without cleanup; after one more second its thread is
abandoned. The test is reported as `TIMEOUT` with `TIMEOUT after LIMIT`, counts
as a failure, and the run continues (`--fail-fast` stops it). Covered by
`cli::test_runner_times_out_hung_tests_and_stops_their_descendants`.

`--cov` prints source coverage, `--api` adds standard-API hit data, and
`--cov-json FILE` writes the machine-readable report (`docs/TESTING.md`).

## API queries

`xsht api` is the first-contact language reference. With no selector it prints
an onboarding guide whose script is itself checked by `crates/xsht/tests/api.rs`.
Selectors are `summary`, `module:NAME`, `api:MODULE.FUNCTION`,
`method:RECEIVER[.METHOD]`, `record:NAME`, `language:ID` (exact or prefix), and
`search:TERMS`; batches preserve order. Options: `--format text|jsonl`,
`--details basic|full`, `--strict`, `--query-file PATH`, `--stdin`.

All content comes from the registry: module and method docs in
`crates/xsh-registry/src/signature/`, language items in
`crates/xsh-registry/src/reference.rs`, and examples in `docs/snippets/api/`.
Items carry a purpose, an optional contract (only constraints that prevent a
wrong program), derived effects, signatures, tags, and an example; they never
expose Rust names, implementation paths, or tests. The registry rejects missing
or unknown documentation.

## Source representations

The AST (arena) is semantic: checkers, lints, lowering, and structural analysis
use it. The CST is source-faithful: tokens, comments, whitespace, delimiters,
and exact spans. Tools decide a change on the AST and express it as a CST-range
edit validated after application. `xsht refactor` replacements operate on
captured spans and do not format; run `xsht fmt` afterwards.
