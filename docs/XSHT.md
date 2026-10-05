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
| `xsht lint [--fix] [--only RULE,...] [--runless] [--deny-notes] [FILE...]`, `xsht lint --list` | non-fatal quality diagnostics and conservative autofixes; the code catalog | `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/lint.rs` |
| `xsht test [FILTER]` | discover and run native `test NAME { ... }` declarations | `crates/xsht/src/xsht/test.rs` |
| `xsht api [QUERY...]` | query language and standard-library metadata | `crates/xsht/src/api.rs`, `crates/xsht/src/cli/api.rs` |
| `xsht trace SCRIPT` | run with structured tracing (text, jsonl, flamegraph, syscall totals) | `crates/xsht/src/trace.rs`, `crates/xsht/src/cli/trace.rs` |
| `xsht grep` / `xsht refactor` | AST-pattern search and span-based rewrite | `crates/xsht/src/grep.rs`, `crates/xsht/src/cli/refactor.rs` |
| `xsht ast SCRIPT` | parser debug output | `crates/xsht/src/cli/syntax_tree.rs` |
| `xsht highlight SCRIPT` | syntax highlighting runs as JSON Lines, from the lexer (`src/syntax/highlight.rs`) | `src/syntax/highlight.rs`, `crates/xsht/src/cli/highlight.rs` |
| `xsht desugar SCRIPT` | the script with every sugar statement replaced by its expansion | `crates/xsht/src/format.rs` (`Formatter::desugar_source`), `crates/xsht/src/cli/desugar.rs` |
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

`xsht-config.ini` is resolved per file from the nearest ancestor of the file's
absolute location; the current directory's config controls no-argument
discovery. Relative paths resolve from the config's directory. A file with no
config above it takes tool settings from the current directory's config or
the defaults, and has no project module roots. An invalid file is a command
error.

| Key | Meaning |
|---|---|
| `include` | extra roots for no-argument discovery |
| `exclude` | glob patterns removed from discovery for path-oriented commands; an explicit directory uses its nearest config's `exclude` |
| `module_path` | module search roots (default `.`, the config's directory), searched after file-relative lookup and `XSH_MODULE_PATH`; `xsh` and `xshi` read this one key for the entry script through the same `project_module_roots` (`src/project.rs`); `xsht test` also passes the roots to `module.load` and appends them to children's `XSH_MODULE_PATH` |
| `test_roots` | directories `xsht test` searches |
| `[format] line-width` | formatter width target (default 120) |
| `[format] exclude` | glob patterns, matched from the discovery root, that `xsht fmt` skips during discovery; files named explicitly are still formatted |
| `[check] annotate` | default `--annotate` policy |
| `[lint] prefer-inferred-pure-returns` | opt-in removal of private pure return annotations the checker can infer |
| `[lint] prefer-inferred-private-effects` | `false` turns off `lint.prefer-inferred-private-effects`, which is on by default |
| `[lint] prefer-env-string`, `prefer-item-shorthand`, `prefer-tempdir-scope` | each rule is on by default; `false` disables its corpus-migration suggestion |
| `[lint] prefer-inferred-variants`, `prefer-positional-constructors` | opt-in `lint.prefer-inferred-variant` (drop a variant qualifier the expected type selects, or in a pattern the matched value's type; a match arm head keeps its qualifier) and `lint.prefer-positional-constructor` (pass in-order constructor fields positionally). Naming either rule in `--only` also turns it on |
| `[lint] prefer-inferred-proc-returns` | opt-in `lint.prefer-inferred-proc-return`: a private proc drops its return annotation when a second check of the file without it is clean and leaves every checked type, statement position, and effect of the file unchanged. Exports, `main`, tests, and recursive procs keep theirs, as does a body that needs the annotation as an expected type (`Err(.Variant(...))`, `.require()`, an empty collection). Off when `[check] annotate` writes returns |
| `[lint] prefer-implicit-messages` | opt-in `lint.prefer-implicit-message`: a variant declared `V(message: Str)` drops its payload and takes the message positionally. Calls that name `message:` are fixed first, in every file, and then the declaration of a private family. An exported declaration is reported without a fix: a variant without a payload takes no named argument, so a `V(message: ...)` call in an importing file, which the lint of the module does not see, would stop checking (`check.error-constructor`); nothing else an importer can write changes. A private one-variant family that `lint.prefer-fail` rewrites is left to that rule. `--only lint.prefer-implicit-message` also turns the rule on |
| `[lint] prefer-rel-path` | opt-in note `lint.prefer-rel-path`: the `path` argument of an `FsRoot` method whose type is a plain `Path` is reported, because it may be absolute or climb out with `..` and then fails only when the root resolves it. A literal without interpolation is left alone. No fix: the `RelPath` type comes from the binding's annotation, the parameter and its callers, or a `.require(RelPath)?`. `--only lint.prefer-rel-path` also turns it on |
| `[lint] prefer-text-pattern` | opt-in note `lint.prefer-text-pattern`, which has no fix: it reports a `split` whose pieces are read as `parts[0]`, `parts[1]`, and a `starts_with` test in a statement that also slices at the prefix's length, where an f-string pattern names the pieces. A pattern matches the whole text and its last hole takes the rest, so the rewrite is the author's; `--only lint.prefer-text-pattern` also runs it |
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
`--fix` formats the rewritten file. A fix is declined when a comment lies inside
its span; the formatter keeps a comment with the match arm it precedes, so a
later round finds it in the same place and declines again.
A file with a check error is not linted, because its checked facts are
incomplete; a check warning is reported beside the file's lint findings.

A code's table severity is `error`, `warning`, `note`, or `mixed`. A lint that
has no safe fix is a `note`: advice that is printed, counted apart in the
closing `xsht lint: N findings, M notes` line (printed only when a note was
reported), and selected by `--only`, but that leaves the exit status at 0.
`lint_diagnostics_status` gives a file's status and never counts a note;
`lint_files_timed` counts the printed notes and, under `--deny-notes`, turns
a run that would exit 0 into status 1. The repository gates agree with the
exit status: `cargo dev check` runs `xsht lint` and reads its status, and
`lint_performance` requires status 0 and no finding other than a note. A
note rule is on by default unless it reports too many sites to read
(`lint.prefer-rel-path`, `lint.prefer-text-pattern`, each behind its `[lint]`
key or `--only`). The notes that are always on:

| Code | Reports |
|---|---|
| `lint.prefer-non-empty-argv` | a spliced command vector (`run @argv`, in any run form and pipeline segment) whose type is a plain `List[T]`, which may be empty and then fails only when it runs. The `NonEmpty[T]` type comes from the binding's annotation, the parameter and its callers, or a `.require(NonEmpty[T])?` |
| `lint.prefer-typed-callable` | a `Proc` or `Pure` parameter of a private top-level function, with its callable type, when every call in the module passes a top-level function and those functions have one signature. A function whose name is also used as a value, or that is called with a splice or spread, is skipped. The body's `.call(...)` becomes a direct call |
| `lint.prefer-within` | a block whose `run` forms all carry the same `--timeout`; one `within` scope states the limit once but bounds the commands together |
| `lint.prefer-env-path-list` | a search-path environment value formatted as a `:`-separated string; the `List[Path]` rewrite is shown and never applied |
A fix round is accepted when the rewritten file has no check diagnostic the
file did not have before, whether or not `--only` selects its code. SIGINT or
SIGTERM is observed between files and between fix rounds; fixed files are
written only after every file is done, so an interrupted run writes none.

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
`lint.prefer-guard` reports a postfix guard only when the guarded statement fits
88 columns, because the formatter has no readable multiline layout for a longer
one; a longer guard stays an `if` block.
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
an onboarding guide whose script is itself checked by `tests/xsh/api-tool.xsh`.
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

## Desugar

`xsht desugar SCRIPT` prints the program the checker and the runtime read:
each sugar statement (`docs/DESIGN.md`) is replaced by the core statements its
expansion builds, and everything else is printed as `xsht fmt` prints it. It
is the formatter with one switch, so a form needs no code of its own here: a
`SugarForm` added in `src/syntax/parser/sugar.rs` is expanded the day it
parses.

- The script is parsed, not checked. The output checks when the script does,
  with the same diagnostics apart from positions, and runs the same.
- Comments stay on the statement they lead. A trailing comment stays on the
  statement's line when the expansion fits on one line and moves to the
  guarded statement otherwise.
- A local that an expansion binds under a name no identifier can spell is
  printed under a fresh name, `STEM_N`, that the script spells nowhere.
- A statement after `# fmt: skip`, or an expression with a comment inside,
  is copied from the source by `xsht fmt`; when it holds sugar, `desugar`
  prints it instead, formatted.
- One rule is not in the output: the failure block of `guard cond else` must
  leave the enclosing continuation, and the `if` printed for it may fall
  through. A script that checks loses nothing.
- The command refuses to print (exit 1, a diagnostic on stderr) when the
  script does not parse, or when what it would print does not parse back to
  exactly the expansion. The second is a bug in the command.

`make docs` shows a snippet's expansion in the SPEC with
`{{.spec.NAME.desugared}}`, which runs this command, so a documented expansion
is the implemented one. `the_desugared_corpus_checks_and_tests_like_the_corpus`
(`crates/xsht/tests/desugar.rs`) desugars every native test file that holds
sugar and requires the same check diagnostics and test results.

## Source representations

The AST (arena) is semantic: checkers, lints, lowering, and structural analysis
use it. The CST is source-faithful: tokens, comments, whitespace, delimiters,
and exact spans. Tools decide a change on the AST and express it as a CST-range
edit validated after application. `xsht refactor` replacements operate on
captured spans and do not format; run `xsht fmt` afterwards.
