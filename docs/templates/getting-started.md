# Getting started with XSH

This page gets you from an empty file to a checked, working XSH script, and
shows the facts that trip up newcomers most often. Every example passes
`xsht check` and prints what is shown. The [tour](user-tour.md) goes deeper on
each topic, and [`docs/SPEC.md`](SPEC.md) is the language contract.

## Discover, check, run

XSH work is a short loop: look up what you need with `xsht api`, check the
whole program with `xsht check`, then run it with `xsh`.

```xsh
{{.guide.first_script.source}}
```

```text
{{.guide.first_script.output}}
```

```bash
xsht api search:sort                 # discover: which stages and methods sort?
xsht api language:stream.sort-by     # read one item's signature and contract
xsht api api:json.decode             # a module function
xsht check hosts.xsh                 # check: parse, resolve and type-check, run nothing
xsh hosts.xsh                        # run: checks again, then executes
xsh deploy.xsh -- web-2 --force      # arguments after `--` go to the script
```

`xsht check` exits 2 on a check error and runs nothing; a runtime failure exits
3 with a traceback. Read the diagnostic code and its `help:` line: most
first-draft mistakes are named precisely. This line, for example,

```xsh
{{.guide.inline_record.source}}
```

fails with `parse.expected-ident`, because a record type used inside
`List[...]` must have a name: write `type Host = {name: Str, port: Int}` and
then `List[Host]`.

## Values

```xsh
{{.guide.values.source}}
```

```text
{{.guide.values.output}}
```

- `let` is immutable, `var` reassignable, `const` fixed at check time. Types
  are inferred; annotate empty collections and optionals.
- Nothing converts implicitly: `"12" as Int` converts or fails, and
  `text.parse_int() ?? 0` falls back.
- `f"..."` interpolates and plain `"..."` never does. Lists and records cannot
  appear inside `f"..."`; join them or `json.encode` them. Strings use double
  quotes only.
- `print` takes a literal, an f-string, `$name`, or a parenthesized expression,
  never a bare name.
- Negation is `!`, as in `if ! ready { ... }`; `not` appears only in `not in`.
- A binding cannot reuse a standard module's name: `path`, `map`, `set`,
  `time`, `user`, `group`, `json`, `fs` and the other modules
  `xsht api summary` lists are taken.
- Parentheses that change nothing are a check error
  (`check.redundant-parens`): write `total += size + 1`, not
  `total += (size + 1)`.
- `Str` has `byte_len()` and `count_chars()`; lists have `len()`. Count with
  `for i in range(n)`: `for i in 0..n` does not parse, while slices such as
  `xs[1..3]` do.
- Closed choices are enums written with a leading dot where the type is
  known: `archive.create(dest, root, entries, .Tar, collision: .Replace)`.

The negation rule catches shell and Python habits alike; this fails with
`parse.expected-expression`:

```xsh
{{.guide.not_negation.source}}
```

## Results and errors

Anything that can fail returns a `Result`. `?` propagates the error, `??`
recovers with a fallback, and `match` handles each case. An `error` declaration
gives your failures names that callers match on:

```xsh
{{.guide.results.source}}
```

```text
{{.guide.results.output}}
```

The catch-all arm of a `match` is `else =>`. Host queries return `Result`
too, predicates included: write `if file.exists()? { ... }` or
`dir.is_dir() ?? false`. `return` belongs to functions; a script ends early
with `exit STATUS`.

## Processes

There is no shell underneath. Each value is exactly one argument, nothing is
split or globbed, and a list is spliced only where you write `@`. The `run`
form you choose decides what a nonzero exit means:

```xsh
{{.guide.processes.source}}
```

```text
{{.guide.processes.output}}
```

`run.text` fails on a nonzero exit; `run.capture --text` keeps the status, stdout
and stderr as data and fails only when the program cannot start; `try` turns
either failure into a `Result`. Options such as `--timeout` and `--accept` come
before the command. `cd (dir) { ... }`, `env ({KEY: "value"}) { ... }` and
`tempdir dir { ... }` are scopes that end with their block; a `cd` or `env`
block used as a value yields a `Result` of its last expression. Bind a run
result with `let` before using it in a larger expression, and prefer a
standard-library operation to an external tool when one exists, since tool
options differ between implementations such as GNU and BusyBox.

## Data pipelines

`|>` passes a list or stream through stages and yields a list. A stage block
reads its item as `.`; name the parameter when you nest blocks.

```xsh
{{.guide.pipelines.source}}
```

```text
{{.guide.pipelines.output}}
```

Decoded JSON is `Any` until `.require(Type)` validates it, as in the first
example. `par-map(jobs: N)` runs at most `N` blocks at once and keeps input
order; stage options are always named, so `par-map(3)` is rejected.

## Cleanup, atomic writes, and retries

```xsh
{{.guide.cleanup.source}}
```

```text
{{.guide.cleanup.output}}
```

`defer` runs when its block exits, failures included. `write_atomic` replaces a
file by renaming a finished sibling over it. `retry [delays] on (pattern)`
retries only the errors the pattern matches and yields a `Result`.

## Command-line tools

A `cli main` signature is the argument parser, the usage text, and the type
conversion. This script accepts `[--keep N] [--dry-run] DIR` and answers a bad
`--keep` with usage text and exit status 2 before its body runs:

```xsh
{{.guide.cli.source}}
```

## Files and directories

`fs.children(dir)?` lists one directory, hidden entries included.
`fs.walk(root)?` walks a tree without following symbolic links, but by default
it skips hidden entries and obeys `.gitignore` files: pass
`gitignore: false, hidden: true` for a complete inventory. Entry paths start
with the root you passed; `entry.path.relative_to(root)` makes them relative.

## Next

- [Agent context](agent-context.md): the compact guide for agents that write
  XSH, measured on the code mode experiment in `bench/codemode/`.
- [The tour](user-tour.md): every topic above in depth, with a larger program
  and a project layout with tests.
- [`docs/SPEC.md`](SPEC.md) and `xsht api language:core`: the full rules.
