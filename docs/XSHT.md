# XSHT Tooling

`xsht` is the toolchain for checking, formatting, linting, testing, querying the
API, tracing, searching, and refactoring XSH source. Scripts run with `xsh`;
`xsht` executes them only for `test` and `trace`. User-facing command options
live in `crates/xsht/src/commands.rs`; `xsht help` and `xsht help COMMAND` are
the flag reference.

## Commands

| Command | Does | Owner |
|---|---|---|
| `xsht check [PATH...]` | parse, load modules, type-check; `--annotate[=POLICY]` writes inferred annotations; `--summary` counts diagnostics by code | `crates/xsht/src/cli/check.rs` |
| `xsht fmt [--check] [FILE...]` | format through the checked program (`docs/XSHT-FMT.md`) | `crates/xsht/src/cli/fmt.rs` |
| `xsht lint [--fix] [--only RULE,...] [--runless] [--deny-notes] [FILE...]`, `xsht lint --list` | non-fatal quality diagnostics and conservative autofixes; the code catalog | `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/lint.rs` |
| `xsht test [FILTER]` | discover and run native `test NAME { ... }` declarations | `crates/xsht/src/xsht/test.rs` |
| `xsht api [QUERY...]` | query language and standard-library metadata | `crates/xsht/src/api.rs`, `crates/xsht/src/cli/api.rs` |
| `xsht trace SCRIPT [ARGS...]` | run with structured tracing (text, jsonl, flamegraph, syscall totals) | `crates/xsht/src/trace.rs`, `crates/xsht/src/cli/trace.rs` |
| `xsht grep PATTERN [FILE...]` / `xsht refactor PATTERN REPLACEMENT [FILE...]` | AST-pattern search and span-based rewrite | `crates/xsht/src/grep.rs`, `crates/xsht/src/cli/refactor.rs` |
| `xsht ast SCRIPT` | parser debug output | `crates/xsht/src/cli/syntax_tree.rs` |
| `xsht highlight SCRIPT` | syntax highlighting runs as JSON Lines, from the lexer (`src/syntax/highlight.rs`) | `src/syntax/highlight.rs`, `crates/xsht/src/cli/highlight.rs` |
| `xsht desugar SCRIPT` | the script with every sugar statement replaced by its expansion | `crates/xsht/src/format.rs` (`Formatter::desugar_source`), `crates/xsht/src/cli/desugar.rs` |
| `xsht grammar [--format ebnf\|json]` | the language's productions as EBNF or JSON | `src/syntax/grammar/reference.rs`, `crates/xsht/src/app.rs` |

## Configuration

The nearest `xsht-config.ini` above a file governs its per-file settings,
regardless of the command's working directory. Lookup uses the file's absolute
lexical path and does not follow symbolic links. With no config, a file gets
the defaults and has no project module roots. Relative paths and exclusion
patterns resolve from the config directory; an invalid config fails the
command.

During directory discovery, `exclude` applies from each config between the
file and the walked root. `[format] exclude` adds another exclusion for
`xsht fmt`. A nested project's config does not cancel an exclusion from an
outer project, but starting discovery inside the nested project does not apply
configs above its root. Files named directly on the command line bypass
discovery exclusions.

`include`, `test_roots`, and `[coverage] exclude` describe the project from
which the command runs, so they come only from the current directory's config.
`include` adds roots to no-argument source discovery; `xsht test` searches
`tests/` by default or the configured `test_roots`. Coverage exclusions remove
files from the source-coverage denominator of `--cov` and `--cov-json` reports.

| Key | Meaning |
|---|---|
| `include` | extra roots for no-argument discovery, read from the current directory's config |
| `exclude` | glob patterns, relative to the config, of files that discovery skips, nested projects included; a file named on the command line is always processed |
| `module_path` | module roots searched after file-relative lookup and `XSH_MODULE_PATH`; defaults to `.` relative to the config directory. `xsh` and `xsht check`, `fmt`, `lint`, `ast`, and `test` use the nearest config for each loaded script; a file without one has no project roots |
| `test_roots` | directories `xsht test` searches, read from the current directory's config (default `tests/`) |
| `[format] line-width` | formatter width target (default 120) |
| `[format] exclude` | glob patterns, relative to the config, that `xsht fmt` skips during discovery; files named explicitly are still formatted |
| `[check] annotate` | annotation classes for `xsht check --annotate` when the flag has no explicit policy |
| `[lint] prefer-inferred-pure-returns`, `prefer-inferred-proc-returns`, `prefer-set`, `prefer-text-pattern`, `prefer-rel-path` | opt-in rules; inferred-return rules are disabled when `[check] annotate` includes returns |
| `[lint] prefer-inferred-private-effects` | on by default; set to `false` to disable |
| `[lint] prefer-env-string`, `prefer-item-shorthand`, `prefer-tempdir-scope` | on by default; set each to `false` to disable |
| `[lint] runless-except` | command names allowed by `xsht lint --runless` |
| `[lint.RULE] exclude` | glob patterns exempting files from one lint rule; the section name must be a lint code, such as `[lint.prefer-inferred-variant]` |
| (no key) | `lint.prefer-fail`, `lint.prefer-implicit-message`, and `lint.redundant-close` are on by default; use `[lint.RULE] exclude` to exempt files |
| `[dead-code] exclude` | files exempt from `lint.dead-code` and `lint.unused-callable` |
| `[coverage] exclude` | files removed from the source coverage denominator, read from the current directory's config |

## Lint

`xsht lint` analyzes checked facts for each reachable program. Files with check
errors are skipped; check warnings appear with lint findings. `--only` limits
reports and fixes to selected lint codes or other fixable diagnostic codes.
Selecting a rule overrides its `[lint]` setting, except inferred-return rules
stay off when `[check] annotate` includes returns; `[lint.CODE] exclude` still
applies.
Lint diagnostics are invariant under formatting.

`--fix` applies guarded source edits in rounds. A fix is declined if it would
add a checker diagnostic or drop a comment. Without `--only`, rewritten files
are formatted; a scoped run applies its edits without a formatting pass. Notes
do not fail a run unless `--deny-notes` is set. `xsht lint --list
[--format text|jsonl]` prints the catalog used by the generated
[lint reference](reference/lints.md).

## Native tests

`xsht test` discovers `test NAME { ... }` under `test_roots` (default
`tests/`) and prepares each test with its file's module roots. `FILTER` matches
a path prefix or test-name substring; `--exact` uses `PATH::TEST_NAME`.
`--list`, `--fail-fast`, `--keep-temp`, `--nocapture`, and `-j`/`--jobs` control
selection and execution. The default timeout is 120 seconds; `--timeout` sets
another XSH duration, while `0` or `none` disables it. `--cov` reports source
coverage, `--api` adds API hits and requires `--cov`, and `--cov-json FILE`
writes the report as JSON.

## API queries

With no selector, `xsht api` prints a getting-started guide. Use `summary`,
`module:NAME`, `api:MODULE.FUNCTION`, `method:RECEIVER[.METHOD]`,
`record:NAME`, `language:ID` (exact or prefix), or `search:TERMS` to query the
registry. Batches preserve order. Options are `--format text|jsonl`,
`--details basic|full`,
`--strict`, `--query-file PATH`, and `--stdin`.

## Desugar

`xsht desugar SCRIPT` prints formatted source with each sugar statement
replaced by its core expansion. It parses without checking or modifying the
input; the output checks and runs as the input does. The command fails if the
input does not parse or if the expanded output does not parse back to the same
program.

## Generated references

`make docs` refreshes `docs/reference/cli.md` from `xsht --help` and
`xsht help COMMAND`, `docs/reference/lints.md` from `xsht lint --list`, and
`docs/reference/stdlib.md` from `xsht api`. It also renders
`docs/reference/grammar.md` from `xsht grammar --format json`. In
`docs/templates/SPEC.md`, a `{{.spec.NAME.desugared}}` placeholder obtains its
snippet expansion from `xsht desugar`. These references are generated; update
their templates, snippets, command, diagnostic, API, or grammar sources, then
run `make docs`.

## Source representations

`xsht grep` matches semantic AST structure. `xsht refactor` rewrites captured
spans in the original source and does not format; run `xsht fmt` afterwards.
See `docs/ARCHITECTURE.md` for the AST and CST invariants.
