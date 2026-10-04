# TODO

## Reduce `.display()` drudgery without lossy Path→Str conversion

`Path` holds native bytes, and `.display()` is its lossy conversion to UTF-8
text. Automatic Path→Str conversion is rejected because it would lose bytes
silently. A survey of this repo plus `../packages` found ~1,160 lines that call
`.display()`, and most of them do not need a lossy conversion at all:

| Share | Use | Possible fix |
|---|---|---|
| ~1/3 | inside interpolation (`f"..."`, `fp"..."`, command words, `print`) | done: `lint.redundant-path-display` and `lint.path-constructor` autofixes, applied to all three repos |
| ~80 | comparison with a literal, e.g. `p.display() == "/repo/x"` | let a string literal take the `Path` type when the other side of `==` or `!=` is a `Path`, so you write `p == "/repo/x"`. This is the same rule as typed bindings and parameters, and it is lossless. |
| dozens | OS byte sinks: `process.command_argv(exe.display(), ...)`, `bytes.from_text(p.display())` | accept `Path` in argv, env, and process APIs, which carry OS bytes anyway; add a lossless `p.bytes()` |
| ~30 | text queries: `.display().starts_with/ends_with/split/replace` | Path methods: component-wise `starts_with`/`ends_with` and `ext`/`stem`/`name` coverage, so path logic stays on paths |
| rest | real text boundaries (JSON, `Str` params, string building) | keep `.display()`: this is the one explicit point where bytes can be lost |

Prior art:
- Rust has the same `.display()`, but `AsRef<Path>`/`AsRef<OsStr>` APIs mean
  callers rarely convert.
- Python's `os.PathLike` makes nearly every API accept `Path`, while f-strings
  convert silently (lossy).
- Go and Nushell treat paths as strings and accept the lossiness.

The lesson is to make sinks accept `Path` rather than add sugar. A shorter
alias (`.text`, `str(p)`) would rename the chore without removing it.

Open decisions:
- which of the rows above to adopt;
- whether literal-to-Path conversion also applies to `in`, `match` patterns,
  and map keys.

Each adopted row changes the language contract and goes into `docs/SPEC.md`
first.

## Small leftovers from the typing campaign

- Overlapping diagnostic codes that may merge:
  - `lex.invalid-string` and `parse.invalid-string`
  - `lex.invalid-escape` and `parse.invalid-string-escape`
  - `compact.statement-count` and `runtime.compact-statement-count`
  - `compact.indexed-driver` and `runtime.indexed-driver`
- Two codes have per-site severity: `check.non-exhaustive-match` and
  `check.reveal-type`.
- Traceback gap: an `Err` that is handled and replaced inside one `?`
  operand can reuse the old traceback, e.g. `(f() ?? Err(x))?`.
- `cargo dev test linux --ci` passes the host target (`aarch64-apple-darwin`)
  into the container, so it works only on a Linux host. Use
  `cargo dev test linux` on macOS. The Linux run also shares `target/` with
  the host and overwrites `target/release` with Linux binaries.
- Cargo's `unused_dependencies` lint flags `mimalloc` in xsht and xshi.
  This is a false positive: the binaries use it, the libraries do not.

## Literal braces in interpolating strings that generate code

Recipes and proofs generate C, XSH, and config text with `f"""..."""`, and
every literal brace in that text must be doubled (`{{`, `}}`); the Laputa
corpus has ~320 such escapes. The generated code then no longer reads as
itself. Raw strings (`r"..."`) do not help: they decode no escapes but do not
interpolate either.

Rejected so far:
- `$`-interpolated block strings (`${expr}`): `$` is already overloaded by
  command words.
- Swift-style hash delimiters (`f#"""...#{expr}..."""#`): too noisy.

Open decision: a delimiter or prefix that makes `{` literal while still
allowing interpolation, or none (keep `{{` and use `template.render` for
large generated files).

## Accepted ergonomics backlog

Each item was surveyed against this repo and `../laputa` (counts are rough
grep counts). Each needs SPEC wording, a lint with a behavior-preserving
autofix where one exists, and tests.

- **Subcommand `cli` entries.** `cli main repo check(repo: Path = default_repo()) { ... }`:
  several `cli` entries named by a subcommand path, each with generated help
  and option parsing. Replaces the `var parsed = Placeholder(...)` +
  `match cli.parse(...) { Ok(v) => parsed = v  Err(e) => return Err(e) }`
  dance (11 times in Laputa's `pm/cli.xsh`, 850 lines).
- **Infer private proc return types**, as private proc effects already are.
  Exports keep their signatures as the contract. About 2,259
  `-> Result[...]` annotations in Laputa; the lint drops one only when the
  inferred type is identical.
- **A `match` that only re-propagates is `?`.** `match f() { Ok(_) => {}
  Err(e) => return Err(e) }` → `f()?` (about 62 Laputa sites). One site
  (`pm/execute.xsh`) says `?` once escaped a `par-map` worker before the
  caller's cleanup ran: confirm whether that runtime bug still exists and fix
  it before the lint.
- **`tempdir NAME at PATH { ... }`**: a scratch directory at a fixed path,
  removed if present, created, and removed on exit. Replaces
  `fs.remove(t, missing_ok: true)?` / `fs.mkdir(t)?` /
  `defer fs.remove(t, missing_ok: true)?` (about 35 full triples, 59
  remove-then-mkdir pairs in Laputa). Extends the `tempdir` block.
- **Path lists as environment values.** A `List[Path]` in an env overlay
  joins with the platform separator, losslessly:
  `PATH: [fp"{root}/usr/bin", ...env.PATH]` instead of
  `f"{root}/usr/bin:{env.get("PATH") ?? ""}"`, whose empty fallback leaves a
  trailing `:` (about 25 Laputa sites).
- **`Path.write_lines(lines)`**: newline-terminated lines, replacing
  `fs.write(p, lines.join("\n") + "\n")` (about 46 sites); pairs with
  `collect { ... }`.
- **One spelling per filesystem operation.** `fs.write(p, x)` and
  `p.write(x)` (likewise `exists`, `read_text`, `remove`, `mkdir`) are two
  names for one concept: about 1,250 `fs.op(path, ...)` calls against
  about 4,000 method calls. Keep the Path method ("methods first"), keep
  `fs.*` only where there is no single path receiver, and lint + autofix.
- **`empty` as a natural-language predicate.** `return nested when ! empty`
  style sugar for emptiness tests instead of `.len() == 0` / `> 0`
  (about 615 sites). Open design point: what `empty` refers to (the subject
  named in the statement, e.g. `when nested is empty` /
  `when nested not empty`); it must desugar trivially to `.len() == 0`.
- **`test.expect(ctx, src, status: N, stderr: [...])`** for script tests. The
  status is a required argument (0 for success), stderr/stdout fragments are
  optional, and any mismatch reports the full output. Replaces
  `run_script` + `assert output.status == N, output.stderr` + repeated
  `assert "..." in output.stderr, output.stderr` (1,013 `run_script` calls,
  about 587 status and 567 stderr asserts here).

## Accepted ergonomics backlog, second batch

- **One spelling for statement-position propagation; fewer `?` overall.** A
  statement-position `Result[Unit]` already propagates (SPEC 8.1), so
  `fs.mkdir(tmp)?` as a statement spells it twice (about 6,400 such lines
  across this repo and Laputa). Drop the redundant `?` with a lint and
  autofix. Broader goal: audit where else `?` is ceremony rather than
  information.
- **`fail MESSAGE` statement**, desugaring to `return Err(error.failure(msg))`.
  Replaces the 124 one-variant `error X = Failed(message: Str)` families that
  exist only to have something to return. Typed families stay for errors
  callers match on.
- **Multi-line error families and implicit messages.** Families cannot span
  lines today (Laputa's `PmError` is one ~1,100-character line). Use the
  `enum`-style brace form, one variant per line, and let a variant without a
  payload take its message positionally (`PmError.Usage("...")`): every error
  already has `.message`, so 273 `(message: Str)` payloads restate it.
- **String patterns in f-string syntax**: `if let f"{key}={value}" = line`,
  `match line { f"#define {name} {body}" => ... }`. Stay as close to Python
  as possible: the hole grammar is Python's format-field grammar read in
  reverse (as the `parse` package does), so typed holes such as `{n:d}`
  follow Python's spec letters. Holes match leftmost-shortest, the last takes
  the rest; a non-match is just a non-matching arm. Targets ~40 split-then-index
  sites, ~33 regex captures, and many `starts_with` + slice pairs.
- **`wait until COND within DURATION`**, failing loudly with `Timeout`
  instead of hand-written polling loops that silently fall through (~37
  `time.sleep` loops). Include exponential backoff in the same sugar, e.g.
  `wait until ready()? within 30s every 100ms` and a backoff form such as
  `... backoff 100ms..5s` (doubling, capped); `retry` should accept the same
  backoff spelling instead of a hand-written delay list. Open: exact spelling
  and whether jitter is the default.
- **Optionals instead of `""` sentinels.** Lint `?? ""` (~475 Laputa sites)
  whose binding is later compared with `""`, and suggest optional binding:
  collapsing "unset" into "empty" makes the two indistinguishable.

## Accepted ergonomics backlog, third batch

- **Conditions propagate `Result[Bool]`.** In a fallible function,
  `if ! fs.exists(p) { ... }` propagates the error, like statement position
  (about 880 condition lines carry `)?` today).
- **Value-position `run.*` forms propagate.** `let v = run.text git describe`
  propagates; `try run.text ...` captures. Removes the detached trailing ` ?`
  that reads like an argv word (361 Laputa lines).
- **`Set[T]` with `{"a", "b"}` literals**, `in`, `|`, `&`, `-`, `.add`.
  Replaces `Map[Bool]` sets and their meaningless `true` values (about 500
  `Map[Bool]`/`set.empty()` sites).
- **Negative single-element indexes.** Slices already count negative bounds
  from the end, but `xs[-1]` fails at run time with `index-out-of-range`
  (verified) and there is no `.last()`. Make `xs[-1]` index from the end;
  lint + autofix `xs[xs.len() - N]` (about 71 sites). A literal negative
  index on a too-short literal list should be a check error.
- **Filter loops become comprehensions.** Rejected: a `for x in xs if c`
  statement form. Instead `lint.prefer-list-comp` must also rewrite loops whose
  first statement is `continue unless c` / `continue when c` (83 sites) when
  the body only accumulates.
- **Backslash continuation for command words.** `run muon setup \` continues
  argv on the next line (a trailing `\` is a lexer error today, so the syntax
  is free). 51 Laputa command lines exceed 120 characters.
- **`xsh` reads the project module path like `xsht`.** `xsht` resolves
  `use pm.proof` from the nearest `xsht-config.ini` `module_path`; `xsh` only
  honors `XSH_MODULE_PATH`, so code can pass `xsht check` and fail to load at
  run time. Laputa sets the variable by hand in its Makefile and reads it in
  11 modules.
- **`Path.glob(pattern)` / `Path.rglob(pattern)`** returning `Path`s, no
  implicit expansion anywhere else (about 23 walk-then-filter sites).
- **Size literals** `KiB MiB GiB KB MB GB` producing `UInt` bytes, like
  Duration literals (about 79 `64 * 1024 * 1024`-style sites).

Rejected this round: `/` as path join (keep `fp"..."`).
