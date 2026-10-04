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

## Accepted ergonomics backlog, fourth batch (natural-English forms)

- **Path predicates in conditions**: `if out is a directory`, `is a file`,
  `is a symlink`, `if gcc_s exists` / `if ! gcc_s exists`. No `is missing`:
  negation stays `!`. Desugar to `exists()` and kind checks; propagate under
  the accepted conditions rule (~950 `exists` calls, ~390 kind comparisons).
- **`within DURATION { ... }`**: a timeout scope over any block, failing with
  `Timeout`; the same word as `wait until ... within` (~68 timeout sites).
- **`fail ... because ERR`**: `fail .RemoteFetch(f"fetching {url}") because problem`
  keeps the replaced error as the cause. Laputa uses `cause:` zero times.
- **`errdefer { ... }`**: cleanup that runs only when the scope leaves with
  an error. Spelled with `defer` in its name so every deferred action is
  found by one search (see `docs/DESIGN.md`).
- **`repeat N times { ... }`** (~45 `for _ in range(n)` sites).
- **`text as Int`** parsing (also `as UInt`, `as Path`, ...), propagating a
  failed parse (~284 `.parse_int()` sites).
- **Argument labels that read as a sentence**: `link.symlink(to: target)`,
  `s.replace("#undef ", with: "")`, `src.copy(to: dest)`. Ends argument-order
  bugs in symlink (~220 symlink, ~840 replace, ~160 copy calls).
- **`fs.remove` and `fs.mkdir` default to `missing_ok: true` and
  `parents: true`**, the overwhelmingly common choice (~390 sites each).
  Rejected: a declarative `ensure p is absent` form (too magical).
- **`else =>`** as the catch-all match arm (~505 `_ =>` arms); `_` keeps its
  meaning inside patterns.
- **`exit N` replaces `abort(N)`**, and `abort` is removed (~131 sites): the
  exit is deliberate, which "abort" contradicts.
- **Postfix `when`/`unless` on any simple statement**, and
  `guard cond else fail "..."` without braces:
  `print f"copying {src}" when verbose`, `fs.remove(tmp) when tmp exists`.

Deferred for further design: `atomically at DIR { ... }` (stage writes in a
hidden sibling directory, rename into place on success, remove on failure).

Rejected, with reasons:
- `not` as unary negation: too mechanical, and `!` is readable.
- Infix text predicates (`starts with`, `ends with`, `matches`): too
  mechanical; the new words would not be reusable elsewhere, so the
  `starts_with`/`ends_with`/regex methods stay the API.
- `pkg with field: value` record updates: too esoteric; keep `{...pkg, field: v}`.
- Quantifier phrases (`any x in xs where c`, `every`, `count`): confusing.
- `first x in xs where c else ...`: a stretch; the explicit loop reads well.
- String test names (`test "a sentence" { }`): strings are bad identifiers.
- `for x in xs, N at a time`: too magical, and `par-map` must stay greppable.
- English collection phrases (`sorted by`, `grouped by`): `sort-by` and the
  other stage names must stay greppable.
- `$`-interpolated or hash-delimited (`f#"""..."""#`) code-generation strings
  (recorded above).
- `/` as path join: keep `fp"..."`.

## Target-typed variants: open follow-ups

- The inferred-variant and positional-constructor lints are opt-in
  (`[lint] prefer-inferred-variants`, `prefer-positional-constructors`)
  because the repository lint gate requires zero diagnostics; enable them by
  default after migrating the corpus (347 + 32 sites here, 13 + 7 in Laputa),
  then delete the settings.
- `Err(.Variant(...))` needs a family-typed return. Laputa's ~652
  `Err(Family.Variant(...))` sites mostly return `Result[T]` (plain `Error`),
  which names no family, so they cannot use it. Decide between family-typed
  returns in Laputa and a different rule; "the one visible family with this
  variant" would be a guess and covers only ~266 sites.
- `.Name` in match patterns is not implemented; patterns stay qualified.


## Accepted hardening backlog

These are biased toward making invalid states fail during checking or at a
single explicit validation boundary instead of surviving as `Any`, broad
`Error`, aliased handles, structurally compatible records, or unchecked host
data. As with the ergonomics batches, each language change needs SPEC wording,
tests, and a migration lint/autofix wherever the old form has a mechanically
equivalent replacement.

- **Optional binding in `guard let` / `if let` / `while let`.** Extend the
  existing conditional-binding family from `Result[T]` to `T?`: for example,
  `guard let executor = context.executor else { return Err(...) }` binds an
  `Executor` on the continuation instead of requiring `executor == null`
  followed by a type-restating binding. The subject evaluates once; a null
  optional takes the failure branch and has no error value, so an optional
  form cannot bind the existing `|failure|` handler parameter. Result binding
  keeps its current `Ok`/error semantics. Laputa has null-test sites across
  roughly 30 files (`== null` in 17, `!= null` in 14 by code search), so
  lint the test-plus-restating-binding shape when the rewrite is local and
  behavior-preserving.

- **Context-managed resources use `with`, not a new `using` keyword.**
  `with root = fs.open_root(path)? { check(root)? }` acquires a value that
  implements a small context-manager protocol, binds the value returned by its
  enter operation, and guarantees the exit operation on every scope exit.
  Multiple managers enter left-to-right and exit right-to-left. This should
  cover `FsRoot`, tempdir handles, locks, and future host resources that need
  explicit fallible cleanup; Laputa currently has the `defer ... .close()`
  shape in about 14 XSH files, plus explicit `fs.lock` / `fs.unlock` pairs.
  Reuse the existing `with` concept rather than introduce a second scoped
  resource word. Open design point: XSH already has `with ... else` for
  grouping fallible bindings, so specify the context-manager protocol and the
  syntactic distinction without type-directed surprises. A narrow
  checker-known `ContextManager[T]` protocol is preferable to adding a
  general trait system only for this feature. Unlike Python, cleanup must not
  silently suppress the body's error; replacement/causal errors stay explicit.

- **Let `@argv` provide the command head.** A spliced argv in target position
  is one complete command vector: `run @argv`, `spawn run @argv`, and
  `process.command { run @argv }` use element zero as both the executable
  target and `argv[0]`. An empty list fails explicitly (and can become a
  check-time impossibility once `NonEmpty[T]` below exists). This removes
  `process.run(process.command_argv(argv[0], argv))` and similar reconstruction
  while keeping `process.command_argv(target, argv)` for the unusual case
  where executable selection intentionally differs from `argv[0]`.
  `process.command_argv` appears in about 49 Laputa XSH files, including about
  13 files with the direct run-of-command-argv shape.

- **`atomically replace DEST as TMP { ... }` for files produced by external
  work.** The block receives a hidden sibling path, guarantees its cleanup on
  failure, and renames it over `DEST` only after the body succeeds:
  `atomically replace image as tmp { run docker save --output $tmp ... }`.
  This is the file counterpart to `write_atomic` for producers that cannot
  write through XSH itself (Docker, archive creation, compilers, copies, image
  builders). Laputa has `fs.rename` in about 15 XSH files and several
  remove/defer/copy-or-build/rename publication sequences. Define "atomic" here
  as visibility by same-directory rename; crash durability remains explicit
  through `fsync`, rather than hiding a durability policy in the sugar. Keep
  the broader `atomically at DIR { ... }` directory transaction deferred
  separately.

- **Closed finite union types instead of `Any` at heterogeneous typed
  boundaries.** Add a greppable type constructor such as
  `Union[Str, Path]`; do not overload `|`, which already has language
  meanings. The immediate Laputa case is `MakeTask.argv: List[Any]`, whose
  real domain is a small set of argv word types. A value fits a union only if
  it fits a listed member; narrowing uses existing `is`/pattern machinery,
  and inference never invents a union merely because branches disagree.
  Start conservatively: exact member signatures, no arbitrary union
  simplification, and no implicit widening to `Any`. Code search finds
  `List[Any]` in only a handful of Laputa files, making this a tractable
  hardening migration with high value at process/data boundaries.

- **Local negative effect bounds.** Inside an effectful proc,
  `without net { build_from_staged_sources()? }` subtracts `net` from the
  effects the checker permits in that lexical region. Nested restrictions
  compose, and diagnostics show the call chain that violates the local bound,
  just as proc effect clauses do. This is a static claim only: it does not
  sandbox a spawned child or pretend XSH can prove what an external executable
  does. The point is to make phase invariants such as "the PM is offline after
  resolution" executable checker contracts even when the enclosing orchestration
  proc legitimately has broad effects. Laputa has broad combinations such as
  `[fs, process, env, error]` across many modules, while individual phases
  often intend substantially narrower capabilities.

- **`Path.read_lines()` as the eager inverse-shaped partner to
  `Path.write_lines(lines)`.** It returns `Result[List[Str]]` with exactly
  the UTF-8 and line semantics of `path.read_text()?.lines()`; keep it eager
  initially so an autofix does not move file-I/O or decoding failure from call
  time into iteration. Laputa has the read-text-plus-lines shape in roughly 16
  XSH files. This keeps the text boundary explicit on `Path` and does not
  introduce implicit file opening through ordinary iteration.

- **Statement matches over closed types are exhaustive errors, not warnings.**
  Today value-position matches must be exhaustive while a statement match over
  an enum can merely warn when a variant is missing. Make a non-exhaustive
  statement match on a known finite nominal type a check error, so adding
  `Fetch` to `enum Action { Build, Reuse, Fetch }` breaks an old
  `match action { Build => ... Reuse => ... }` at check time. A deliberate
  forward-compatible catch-all is written explicitly with the accepted
  `else =>` arm. Start with ordinary and Str-backed enums; decide separately
  whether an exactly known error family or other finite builtins should receive
  the same rule. This should also eliminate the current per-site severity oddity
  around `check.non-exhaustive-match` for the closed-type case.

- **Affine checking for runtime-owned handles.** Turn "already consumed" from a
  runtime `ProcessError.Unknown` class into a checker error where ownership is
  statically visible: after `wait child?`, a later `child.cancel()?` is
  invalid; likewise a consumed `NetJob`, closed `FsRoot`, and other
  single-owner host handles. Keep this deliberately smaller than Rust:
  no general references or lifetime calculus, only checker-known runtime-owned
  types, moves, and consuming operations. Direct aliasing should move ownership
  rather than create two independently usable names. Open design point:
  function parameters need one simple distinction between borrowing a handle
  for the call and taking ownership; choose that from corpus needs rather than
  importing a full borrow syntax. Lexical auto-cancellation/cleanup remains the
  fallback for a live owner that leaves scope.

- **A confined relative-path type for root-relative operations.** Add
  `RelPath` (name open) for native-byte paths proven not to be absolute and
  not to escape their logical root. `file.strip_prefix(root)?` can return it
  directly, and `FsRoot`/archive/rootfs APIs should prefer it over arbitrary
  `Path`: `proc install(root: FsRoot, rel: RelPath) ...`. Expected-type path
  literals may construct one when statically valid. Keep the guarantee lexical
  and exact; it does not by itself solve symlink traversal, which remains the
  responsibility of the rooted filesystem capability. This targets the
  repeated strip-prefix-then-operate pattern throughout Laputa package
  manifests, root composition, image construction, and archive handling.

- **Bounded scalar types, deliberately not general refinement types.** Support
  finite constant bounds for a small set of scalar domains, e.g.
  `type Port = Int range 1..65535`, `ExitCode`, sizes, modes, and similar
  values. An in-range literal is checked statically; converting/parsing a
  runtime value validates the bound at that explicit boundary. Do not permit
  arbitrary predicates, user code, dependent constraints, or SMT-style proof
  obligations: the feature must remain a decidable local range check. Be
  conservative under arithmetic—if the checker cannot prove an operation
  preserves the bound, produce the base scalar (or require explicit
  revalidation) rather than grow a range-analysis language. Exact range syntax
  and inclusive-bound spelling are still open.

- **`NonEmpty[T]` for collections whose first element is part of the
  contract.** A command argv is the canonical case: it is not merely a
  `List[Arg]`; it must contain an executable. Non-empty literals satisfy the
  type at check time, while converting an arbitrary list validates once.
  `first()` (and equivalent guaranteed operations) are total on
  `NonEmpty[T]`. Preserve the guarantee only through operations that
  obviously do so (for example `map` and appending); ordinary filters,
  slices, and removals return `List[T]` unless revalidated. This pairs with
  `run @argv` so an empty command can eventually become a type error rather
  than a runtime setup failure.

- **Public Result signatures must spell their error type.** Keep
  `Result[T]` as low-ceremony private code, but reject it on exported procs,
  module-contract entries, and other declared API boundaries: write
  `Result[Config, ConfigError]` when callers can rely on a family, or
  `Result[Config, Error]` when broad failure is intentionally part of the
  contract. This mirrors the existing rule that private returns/effects may be
  inferred while exports state their contract. It prevents an API from
  silently widening its failure surface as implementation calls change, and
  makes broad `Error` visible in review. A lint can add `, Error` without
  changing behavior, after which modules may narrow families deliberately.

- **Exact module contracts for capability minimization.** Existing
  `type Plugin = module { ... }` contracts allow additional exports. Add an
  opt-in closed form such as `exact module { ... }` whose
  `.require(Contract)` rejects an unexpected export as well as a missing or
  mismatched one. Optional members remain optional; "exact" means the module's
  exported capability surface is no larger than the contract. This is useful
  for service modules, recipe/plugin interfaces, and other dynamically loaded
  code where capability growth should require an explicit contract change.
  Diagnostics must name every extra export clearly. This is intentionally
  separate from record/JSON schema exactness, which is deferred below.

- **Make `Any` opaque by default; dynamic traversal becomes an explicit
  escape hatch.** Today `Any` may be navigated with fields, indexes, method
  calls, and iteration, propagating `Any` and deferring shape errors to
  runtime. Tighten the default so dynamic data must be validated with
  `.require(T)` or narrowed by a type pattern before concrete traversal.
  Preserve a deliberately dynamic mode for genuinely schema-less code, but
  make it local and greppable (exact spelling open, e.g. a `dynamic value {
  ... }` scope) rather than silently inheriting dynamic semantics from the
  type. Equality, validation, and re-encoding can remain available without the
  escape hatch. Laputa uses `Any` in a modest number of XSH files, and the
  typed-callable and union items in this batch remove two important reasons to
  reach for it.

- **Typed first-class callables that retain signatures and effects.** Add a
  callable type form such as
  `type Builder = proc(root: Path) [fs, process, error] -> Result[Unit]`.
  Conditional selection, records, parameters, and returns can then carry a
  checked callable without degrading to dynamic `Proc.call(...) ->
  Result[Any]`: `let build: Builder = if debug { debug_build } else {
  release_build }; build(root)?`. Start with conservative assignability:
  parameter labels/types and result shape must match, while the concrete
  callable's effects may be a subset of the declared bound. Keep bare
  `Proc`/`Pure` as the explicit dynamic escape hatch. This closes a major
  type-and-effect erasure boundary without introducing futures, callbacks as a
  runtime model, or general higher-kinded typing.

- **Nominal record construction as an opt-in alternative to structural
  schemas.** Add a form (spelling open: `nominal type` or `opaque type`)
  whose values cannot be forged merely by having the same fields:
  `consume_package({id, version})` does not satisfy a nominal `Package`;
  callers must use its constructor or another API that returns the nominal
  type. Keep ordinary `type T = { ... }` width-compatible and structural.
  Do not conflate nominal identity with representation privacy: if an
  `opaque` spelling also hides fields outside the declaring module, specify
  that as an additional module-visibility rule rather than an accidental side
  effect. The goal is to encode invariants and domain identity where structural
  records are too permissive, without making every record nominal.

### Deferred and rejected from the hardening pass

- **Record/JSON schema exactness is deferred.** Do not add `exact { ... }`
  yet. Exactness may belong at the dynamic validation operation instead
  (`json.require(..., exact: true)`, a strict `.require` mode, or a related
  schema API) so one schema can deliberately accept, strip, preserve, or reject
  unknown fields at different boundaries. Review Zod's TypeScript model
  (strict/passthrough/catchall-style choices) and similar schema systems before
  choosing how XSH composes exactness with width-compatible records, defaults,
  JSON decoding/encoding, and future schema evolution.

- **`assert let PATTERN = VALUE` is rejected.** Although it could combine a
  shape assertion with binding, the syntax makes assertion, pattern matching,
  and declaration read ambiguously at a glance. Keep ordinary `assert`,
  explicit patterns, and existing conditional binding as separate concepts.

- **Nominal scalar `type X wraps Str` is rejected for now.** Do not add a
  second scalar-wrapper mechanism while the accepted `text as T` conversion
  work is still defining explicit typed conversion boundaries. Revisit only if
  real corpus bugs remain that `as`, enums, bounded scalars, or nominal
  records cannot express cleanly.

## Accepted and specified, not started (first batch)

- **`collect { ... }` blocks**: the block's `yield`s (including
  `yield x when c` and yields inside loops) append to a List that is its
  value; lint + autofix for local lists built only by `xs = xs.push(..)` /
  `xs += [..]` (~2,858 `x = x.push(` lines in Laputa).

- **Lint the redundant `}?` on statement scopes** and make `fs.write(p, x, mode: M)`
  create the file with its final mode, merging `write` + `chmod` pairs by
  autofix. Statement scopes already propagate (SPEC 8.1).
- **Dotted `use a.b` binds `b`**; lint + autofix the redundant `as b`.
- **`for i, x in xs`** with an index binding; lint + autofix counter
  `while i < xs.len()` loops into it or into a slice.
- **Existing lints under-report on Laputa**: `x = x.push(...)` appears
  ~2,858 times but `prefer-list-compound-assignment` flags ~37; three-line
  `if c { return Err(..) }` guards ~445 vs 96 flagged. Find why and fix.

## Defects found while migrating Laputa

- Positional error-constructor arguments bind to payload fields in
  alphabetical order, not declaration order: with
  `Conflict(path: Path, owner: Str)`, `Conflict(p"x", "o")` is a type error,
  and two same-typed fields would silently swap. Laputa's
  `Failed(kind, message)` works only because its names are alphabetical.
  Fixing this changes behavior.

- `module.load(p)?.require(C)?.build()` fails to check ("unknown method
  `build` on Record") while binding the required module with `let` first
  works.
- A failed `.require(Contract)` on a module reports "schema check failed at
  $: expected Module, found Module", naming neither the missing or
  mismatched export nor the reason, and missing exports are not
  distinguishable from signature mismatches by facet.
- `xsht fmt` is not idempotent on Laputa's `packages/terminfo/proof.xsh`
  (a map comprehension with an `if`); `syntax::formatter_is_idempotent_on_laputa_corpus`
  fails on the host.
