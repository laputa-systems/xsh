# Chapter 3: Tooling And Development Loop

This chapter is about the loop you use while writing XSH, not about memorizing
every tool flag.

By the end, you should know how to take a small script from "I wrote some
source" to "I trust what it will run": check it before effects happen, format
it, ask for review-style lint feedback, run it with arguments, test it, and
trace it when the behavior is surprising.

## The Everyday Surfaces

XSH has three command-line surfaces because they do different jobs:

- `xsh` runs scripts.
- `xshi` starts the interactive prompt.
- `xsht` owns the development tools: check, format, lint, test, trace, docs,
  AST inspection, search, and refactoring.

That split matters. A script runner should stay focused on execution. Tooling
should be available before execution, while you still have a chance to catch a
bad path, a missing import, or a mistaken type without touching the host.

## Start With A Script

Run a script with `xsh`:

```sh
xsh examples/hello.xsh
```

Script arguments go after `--`:

```sh
xsh examples/args.xsh -- one two
```

The separator is not decoration. `xsh` has its own options, and everything
after `--` belongs to the script as `args: List[Str]`.

The interactive prompt is `xshi`. It accepts some compatibility commands while
you are exploring, but ordinary `.xsh` files should use XSH forms such as
`print`, typed filesystem APIs, and explicit `run`.

## Check Before Effects

Use `xsht check` before running a script whose effects matter:

```sh
xsht check examples/hello.xsh
```

This catches syntax errors, missing modules, name errors, type errors, invalid
effect boundaries, and compact-runtime lowerability failures without starting
processes or changing files.

`xsht check` accepts files or directories. With no path, it checks all `.xsh`
files under the current directory, plus configured `include` files or
directories from `xsht-config.ini`, skipping paths matched by `exclude`. It exits
with status `0` on success and `2` for source, parse, resolve, check, or
compact lowerability failures.

When a type is not what you expect, temporarily add `reveal_type(expr)`.
`xsht check` prints the inferred type as a note. Normal `xsh` execution rejects
scripts that still contain `reveal_type`, so it stays a development aid.

Use strict checking when a script crosses dynamic data boundaries:

```sh
xsht check --strict tool.xsh
```

Strict mode warns when `Any` from JSON, records, or other dynamic boundaries is
used as a concrete type without `value.require(Schema)?`.

## Format And Lint For Review

Formatting keeps source shape boring:

```sh
xsht fmt tool.xsh
xsht fmt --check tool.xsh
```

`xsht fmt --check` is the CI form. It exits with status `1` when a file needs
formatting and status `2` when the source could not be read or parsed.
By default the formatter targets 120 columns. Set `[format] line-width = N` in
the nearest `xsht-config.ini` to choose a different positive integer target.

Linting is review feedback:

```sh
xsht lint tool.xsh
xsht lint --fix tool.xsh
xsht fmt tool.xsh
```

Lint warnings are not type errors, but they catch patterns that make scripts
harder to maintain. Some rules can be fixed automatically. For example,
`lint.unsorted-imports` sorts contiguous import blocks, and effect annotation
linting can insert or update inferred proc effects. `lint.redundant-tail-return-binding`
rewrites a final binding followed by `return name` into the equivalent implicit
tail expression. `lint.redundant-ok-tail` removes final `return Ok(value)`
ceremony when a plain tail value is enough, and
`lint.redundant-newline-triple-string` rewrites a one-newline triple string to
`"\n"`. If a fix would require moving commented code or changing declaration
order, the linter reports the problem and leaves the edit to you.

Use `xsht check --annotate` when you want high-signal inferred annotations
written into source. It refuses to write if parsing or checking reports
diagnostics, then formats the result before saving.

## Test The Behavior

Use native tests when helper logic, temp files, mocks, or expected failures are
part of the script's contract:

```sh
xsht test
xsht test dns
xsht test --list
```

Test files live under `tests/**/*.xsh` and `showcase/tests/**/*.xsh`.
Individual tests are top-level `proc test_*` functions returning
`Result[Unit]`. Cataloged examples are integration checks and stay opt-in:

```sh
xsht test --examples
xsht test --all
```

The testing chapter builds this up with temp resources and host mocks. For now,
the important habit is simple: once a script has reusable logic, give that
logic a native test instead of relying only on manual runs.

## Trace Surprising Runs

When a checked script still does the wrong thing, trace execution:

```sh
xsht trace tool.xsh -- sample args
```

Tracing belongs to `xsht` because it is a development command, but the trace is
collected from real script execution. Start with the summary. If the wrong
operation ran, rerun with `--raw` and inspect the runtime events. The tracing
chapter shows how to read process argv, cwd, env overlays, stream stages, and
propagated failures.

## Search And Refactor

Plain text search is still useful, but XSH also has structural search:

```sh
xsht grep 'regex.compile(PAT)' showcase/
xsht grep 'MAP.get(KEY, FALLBACK)' showcase/ examples/
```

Uppercase-only names in the pattern are metavariables. `xsht refactor` applies
the same idea as a rewrite:

```sh
xsht refactor 'MAP.get(KEY, FALLBACK)' 'MAP.get(KEY, FALLBACK)' --dry-run examples/
```

Use `--dry-run` first. Without it, files are rewritten in place. Run
`xsht fmt` afterward. With no file arguments, `xsht grep` and `xsht refactor`
search all `.xsh` files under the current directory, plus configured `include`
files or directories from `xsht-config.ini`.

`xsht ast` prints parser debug output. Most script authors will not need it,
but it is useful when developing XSH itself or reducing a formatter/parser
issue.

## Docs Tooling

This guide is generated from `docs-src/`:

```sh
xsht docs build
xsht docs check
```

Repository maintenance usually goes through the combined docs gate:

```sh
make docs
```

That command checks Rust formatting, regenerates markdown and HTML docs, checks
generated drift, runs docs tests, and runs the example runtime corpus.

## A Good Default Loop

For an ordinary script, use this loop:

```sh
xsht check tool.xsh
xsht lint tool.xsh
xsht fmt tool.xsh
xsh tool.xsh -- sample args
```

When the script grows reusable logic, add `xsht test`. When the run is
surprising, add `xsht trace`.

Common trap: do not type `xsh check ...`. `xsh` runs scripts. Tooling lives
under `xsht`.

The next chapter turns from tooling to the first major runtime boundary:
external processes, captures, status values, cwd, and environment state.
