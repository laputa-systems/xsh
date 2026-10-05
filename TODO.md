# TODO

The backlog of the hardening and ergonomics campaign, grouped into workstreams
that one agent can own. `campaign.md` is the workflow. Items inside a
workstream are in the order to do them; an item's ID (`PROP-3`) is stable, and
finished items are deleted.

Each item was surveyed against this repo and `../laputa` (counts are rough
grep counts). Each language change needs SPEC wording first, tests, and a
migration lint with a behavior-preserving autofix wherever the old form has a
mechanically equivalent replacement.

The hardening items are biased toward making invalid states fail during
checking or at a single explicit validation boundary instead of surviving as
`Any`, broad `Error`, aliased handles, structurally compatible records, or
unchecked host data.

## Decisions

Closed on 2026-10-04. Each is written on its item below.

| Item | Decision |
|---|---|
| `PROP-8`, `PROP-9`, `MATCH-8` | One propagation rule: a failure leaves a function only through a form visible at the site. See `PROP`. |
| `PATH-5`, `PATH-6` | Adopt every row of the `.display()` table. A string literal takes the `Path` type wherever the expected type is `Path`: `==`, `!=`, `in`, `match` patterns, and map keys. |
| `PATH-11` | The defaults flip only after a lint has made today's defaults explicit at every call that relies on them. |
| `PATH-12` | The type is named `RelPath`. |
| `PATH-13`, `MATCH-10` | Methods, not phrases: `p.is_dir()` and `xs.is_empty()`. `is` keeps one meaning, the type test. |
| `ERR-4` | `Err(.Variant(...))` requires a declared family-typed return. There is no rule that guesses a family. |
| `MATCH-2` | Ordinary and Str-backed enums now. An exactly known error family follows once `ERR-5` has merged. |
| `MATCH-7` | Only a literal negative index counts from the end. A computed negative index still fails. |
| `TYPE-1`, `TYPE-2`, `MOD-5` | The spellings written on the items: `Union[A, B]`, `proc(...) [effects] -> T`, `exact module { ... }`. |
| `TYPE-4`, `TYPE-5`, `PATH-12` | One checker mechanism for validated types, built with `TYPE-4`. |
| `TYPE-6` | `nominal type`. Identity only; field privacy is not part of it. |
| `SCOPE-9` | Last, and narrow: use after a consuming operation in straight-line code of one scope. |

Still open; the item is not assigned until it is closed here:

| Item | Open point |
|---|---|
| `SCOPE-5` | The context-manager protocol, and how `with x = m { }` differs from `with ... else`, which always has its `else`. |
| `SCOPE-8` | Exact spelling; whether jitter is the default. |
| `SCOPE-9` | How a parameter distinguishes borrowing a handle from taking it. |
| `TYPE-5` | Range syntax and inclusive-bound spelling. |
| `TYPE-7` | Spelling of the local dynamic-traversal escape hatch. |
| literal braces | No accepted design (see "Open, without an accepted design"). |

## `PROP`: propagation and failure flow

What `?` means, where a failure may leave without one, and the statements
that produce failures. `PROP-1` to `PROP-4` change no language rule.

The propagation rule the later items implement, replacing the list in SPEC 1
("Results, not exceptions"): a failure leaves a function only through a form
that is visible at the site.

- The `?` operator.
- The keywords `run`, `assert`, `fail`, and `as`. A `run` form fails on a
  failed command unless `try` or `run.status` captures it; `as` fails on a
  failed conversion.
- A `Result` in a control position, where it cannot be a value: a
  statement-position `Result[Unit]` and a `Result[Bool]` condition.

Nothing else propagates. A `Result` in a binding, an argument, an operand, or
a return is data and needs `?`, whatever type the context expects. Where the
rule makes a `?` redundant, a lint removes it, so each site has one spelling.

- `PROP-1` **One spelling for statement-position propagation; fewer `?` overall.** A
  statement-position `Result[Unit]` already propagates (SPEC 8.1), so
  `fs.mkdir(tmp)?` as a statement spells it twice (about 6,400 such lines
  across this repo and Laputa). Drop the redundant `?` with a lint and
  autofix. Broader goal: audit where else `?` is ceremony rather than
  information.
- `PROP-2` **Lint the redundant `}?` on statement scopes.** Statement scopes
  already propagate (SPEC 8.1).
- `PROP-3` **A `match` that only re-propagates is `?`.** `match f() { Ok(_) => {}
  Err(e) => return Err(e) }` → `f()?` (about 62 Laputa sites). One site
  (`pm/execute.xsh`) says `?` once escaped a `par-map` worker before the
  caller's cleanup ran: confirm whether that runtime bug still exists and fix
  it before the lint.
- `PROP-4` Traceback gap: an `Err` that is handled and replaced inside one `?`
  operand can reuse the old traceback, e.g. `(f() ?? Err(x))?`.
- `PROP-5` **`fail MESSAGE` statement**, desugaring to `return Err(error.failure(msg))`.
  Replaces the 124 one-variant `error X = Failed(message: Str)` families that
  exist only to have something to return. Typed families stay for errors
  callers match on.
- `PROP-6` **`fail ... because ERR`**: `fail .RemoteFetch(f"fetching {url}") because problem`
  keeps the replaced error as the cause. Laputa uses `cause:` zero times.
  Needs `PROP-5`; the `.Variant` spelling needs `ERR-4`.
- `PROP-7` **Postfix `when`/`unless` on any simple statement**, and
  `guard cond else fail "..."` without braces:
  `print f"copying {src}" when verbose`, `fs.remove(tmp) when tmp.exists()`.
  Needs `PROP-5`.
- `PROP-8` **Conditions propagate `Result[Bool]`.** In a fallible function,
  `if ! fs.exists(p) { ... }` propagates the error, like statement position
  (about 880 condition lines carry `)?` today).
  A condition is a control position under the propagation rule. Last in the
  workstream, with `PROP-9`.
- `PROP-9` **Value-position `run.*` forms propagate.** `let v = run.text git describe`
  propagates; `try run.text ...` captures. Removes the detached trailing ` ?`
  that reads like an argv word (361 Laputa lines).
  `run` is the visible form under the propagation rule. Migrate in two steps:
  first a lint writes `try` on every value-position run form that is captured
  today, then the bare form changes meaning and the trailing `?` is removed.

## `ERR`: error families and variants

How error families are declared, constructed, matched, and named in
signatures.

- `ERR-3` `.Name` in match patterns is not implemented; patterns stay qualified.
- `ERR-4` `Err(.Variant(...))` needs a family-typed return. Laputa's ~652
  `Err(Family.Variant(...))` sites mostly return `Result[T]` (plain `Error`),
  which names no family, so they cannot use it. Decide between family-typed
  returns in Laputa and a different rule; "the one visible family with this
  variant" would be a guess and covers only ~266 sites.
  Decided: the inferred form requires a declared family-typed return, and
  there is no guessing rule. Laputa gains family-typed returns where `ERR-5`
  narrows a signature; other sites keep the qualified spelling.
- `ERR-5` **Public Result signatures must spell their error type.** Keep
  `Result[T]` as low-ceremony private code, but reject it on exported procs,
  module-contract entries, and other declared API boundaries: write
  `Result[Config, ConfigError]` when callers can rely on a family, or
  `Result[Config, Error]` when broad failure is intentionally part of the
  contract. This mirrors the existing rule that private returns/effects may be
  inferred while exports state their contract. It prevents an API from
  silently widening its failure surface as implementation calls change, and
  makes broad `Error` visible in review. A lint can add `, Error` without
  changing behavior, after which modules may narrow families deliberately.
  Pairs with `MOD-7`; settle `ERR-4` first so the lint adds the right type.
- `ERR-6` The inferred-variant and positional-constructor lints are opt-in
  (`[lint] prefer-inferred-variants`, `prefer-positional-constructors`)
  because the repository lint gate requires zero diagnostics; enable them by
  default after migrating the corpus (347 + 32 sites here, 13 + 7 in Laputa),
  then delete the settings.
  Runs as a migration once `ERR-3` and `ERR-4` have merged.
- `ERR-7` Error-constructor argument binding is decided by lowering a second
  time instead of consumed from a checker fact, and error constructors lack
  two record-constructor rules: a positional argument after a named one is
  accepted, and there is no same-type overlap rule. Publish the binding as a
  checker fact and apply the record rules.
- `ERR-8` `lint.prefer-implicit-message` is opt-in
  (`[lint] prefer-implicit-messages`) because exported families get a note
  and no fix: 184 message-only variants here (14 exported), 79 in Laputa (43
  exported). Migrate both corpora, enable it by default, and delete the
  setting.

## `PATH`: paths and the filesystem API

Mostly registry and standard-module work with little new syntax.

### Reduce `.display()` drudgery without lossy Path→Str conversion

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

### Items

- `PATH-5` **OS byte sinks accept `Path`** (table row 3): argv, env, and
  process APIs take a `Path` without `.display()`, and `p.bytes()` is the
  lossless byte view. `TYPE-1` gives the argv word type its name.
- `PATH-6` **A string literal takes the `Path` type wherever the expected type
  is `Path`** (table row 2): typed bindings, parameters, `==` and `!=`
  against a `Path`, `in` against a collection of paths, a literal pattern in
  a `match` on a `Path`, and a key of a `Path`-keyed map. One rule, with a
  lint for `p.display() == "literal"`. Today `let q: Path = "a"` and `f("a")`
  are rejected although SPEC 4.4 allows them, and a `Str` literal for a
  registry `Path` parameter passes the check and fails at run time.
- `PATH-7` **Path lists as environment values.** A `List[Path]` in an env overlay
  joins with the platform separator, losslessly:
  `PATH: [fp"{root}/usr/bin", ...env.PATH]` instead of
  `f"{root}/usr/bin:{env.get("PATH") ?? ""}"`, whose empty fallback leaves a
  trailing `:` (about 25 Laputa sites).
- `PATH-8` **`fs.write(p, x, mode: M)` creates the file with its final mode**,
  merging `write` + `chmod` pairs by autofix.
- `PATH-9` **Argument labels that read as a sentence**: `link.symlink(to: target)`,
  `s.replace("#undef ", with: "")`, `src.copy(to: dest)`. Ends argument-order
  bugs in symlink (~220 symlink, ~840 replace, ~160 copy calls).
- `PATH-10` **One spelling per filesystem operation.** `fs.write(p, x)` and
  `p.write(x)` (likewise `exists`, `read_text`, `remove`, `mkdir`) are two
  names for one concept: about 1,250 `fs.op(path, ...)` calls against
  about 4,000 method calls. Keep the Path method ("methods first"), keep
  `fs.*` only where there is no single path receiver, and lint + autofix.
- `PATH-11` **`fs.remove` and `fs.mkdir` default to `missing_ok: true` and
  `parents: true`**, the overwhelmingly common choice (~390 sites each).
  Rejected: a declarative `ensure p is absent` form (too magical).
  Changes what existing calls do. Needs `PATH-10`, and a lint that first makes
  today's defaults explicit at every call that relies on them.
- `PATH-12` **A confined relative-path type for root-relative operations.** Add
  `RelPath` (name open) for native-byte paths proven not to be absolute and
  not to escape their logical root. `file.strip_prefix(root)?` can return it
  directly, and `FsRoot`/archive/rootfs APIs should prefer it over arbitrary
  `Path`: `proc install(root: FsRoot, rel: RelPath) ...`. Expected-type path
  literals may construct one when statically valid. Keep the guarantee lexical
  and exact; it does not by itself solve symlink traversal, which remains the
  responsibility of the rooted filesystem capability. This targets the
  repeated strip-prefix-then-operate pattern throughout Laputa package
  manifests, root composition, image construction, and archive handling.
  `MATCH-8` defines the conversion boundary it validates at. An instance of
  the validated-type mechanism of `TYPE-4`; needs it.
- `PATH-13` **Path kind predicates as methods**: `out.is_dir()`,
  `out.is_file()`, `out.is_symlink()`, each a `Result[Bool]` beside
  `exists()`, with a lint for the kind comparisons they replace (~950
  `exists` calls, ~390 kind comparisons). In a condition they need no `?`
  once `PROP-8` has merged; the methods themselves do not need it.
- `PATH-14` **`Path.components() -> List[Path]`**, with a lint for
  `.display().split("/")` (8 sites). About 20 `.display()` prefix and suffix
  tests remain that are text tests, not component tests (`"lib/"`, `".a"`);
  they need a manual rewrite or stay as `.display()`.
- `PATH-15` `Path.lines`, `bytes_lines`, and `touch_from` are missing from
  the checker's method effect table, so they need no `fs` effect. Fix it and
  derive a test from the registry that every method's declared effects are
  enforced.
- `PATH-16` `lint.prefer-file-lines` never fires: `x.read_text()?.lines()`
  parses as a guarded hop.

Rejected this round: `/` as path join (keep `fp"..."`).

## `SCOPE`: scoped blocks, time, and resources

Block forms that own a resource or a deadline, and the ownership rules behind
them. Most are sugar over `defer`. `repeat N times` is merged; it runs
`range(N)`, so a negative count counts down instead of running zero times.

- `SCOPE-1` **`tempdir NAME at PATH { ... }`**: a scratch directory at a fixed path,
  removed if present, created, and removed on exit. Replaces
  `fs.remove(t, missing_ok: true)?` / `fs.mkdir(t)?` /
  `defer fs.remove(t, missing_ok: true)?` (about 35 full triples, 59
  remove-then-mkdir pairs in Laputa). Extends the `tempdir` block.
- `SCOPE-3` **`collect { ... }` blocks**: the block's `yield`s (including
  `yield x when c` and yields inside loops) append to a List that is its
  value; lint + autofix for local lists built only by `xs = xs.push(..)` /
  `xs += [..]` (~2,858 `x = x.push(` lines in Laputa).
- `SCOPE-4` **`errdefer { ... }`**: cleanup that runs only when the scope leaves with
  an error. Spelled with `defer` in its name so every deferred action is
  found by one search (see `docs/DESIGN.md`).
- `SCOPE-5` **Context-managed resources use `with`, not a new `using` keyword.**
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
- `SCOPE-6` **`atomically replace DEST as TMP { ... }` for files produced by external
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
- `SCOPE-7` **`within DURATION { ... }`**: a timeout scope over any block, failing with
  `Timeout`; the same word as `wait until ... within` (~68 timeout sites).
- `SCOPE-8` **`wait until COND within DURATION`**, failing loudly with `Timeout`
  instead of hand-written polling loops that silently fall through (~37
  `time.sleep` loops). Include exponential backoff in the same sugar, e.g.
  `wait until ready()? within 30s every 100ms` and a backoff form such as
  `... backoff 100ms..5s` (doubling, capped); `retry` should accept the same
  backoff spelling instead of a hand-written delay list. Open: exact spelling
  and whether jitter is the default.
  Shares `Timeout` and the word `within` with `SCOPE-7`.
- `SCOPE-9` **Affine checking for runtime-owned handles.** Turn "already consumed" from a
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
  Needs `SCOPE-5`. The most flow-sensitive item in the backlog; last. Start
  with use after a consuming operation in straight-line code of one scope.

Deferred for further design: `atomically at DIR { ... }` (stage writes in a
hidden sibling directory, rename into place on success, remove on failure).

## `MATCH`: matching, binding, and conversion

Patterns, exhaustiveness, conditional binding, indexing, and explicit
conversion.

- `MATCH-1` **`else =>`** as the catch-all match arm (~505 `_ =>` arms); `_` keeps its
  meaning inside patterns.
- `MATCH-2` **Statement matches over closed types are exhaustive errors, not warnings.**
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
  Needs `MATCH-1` and its migration, so the autofix can add `else => {}`.
  Decided: enums now; an exactly known error family once `ERR-5` has merged.
- `MATCH-3` Two codes have per-site severity: `check.non-exhaustive-match` and
  `check.reveal-type`.
  `MATCH-2` removes the `check.non-exhaustive-match` case.
- `MATCH-4` **Optional binding in `guard let` / `if let` / `while let`.** Extend the
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
- `MATCH-5` **Optionals instead of `""` sentinels.** Lint `?? ""` (~475 Laputa sites)
  whose binding is later compared with `""`, and suggest optional binding:
  collapsing "unset" into "empty" makes the two indistinguishable.
- `MATCH-6` **`for i, x in xs`** with an index binding; lint + autofix counter
  `while i < xs.len()` loops into it or into a slice.
- `MATCH-7` **Negative single-element indexes.** Slices already count negative bounds
  from the end, but `xs[-1]` fails at run time with `index-out-of-range`
  (verified) and there is no `.last()`. Make `xs[-1]` index from the end;
  lint + autofix `xs[xs.len() - N]` (about 71 sites). A literal negative
  index on a too-short literal list should be a check error.
  Decided: only a literal negative index counts from the end. A computed
  index that turns out negative still fails with `index-out-of-range`, so an
  off-by-one cannot read the last element silently.
- `MATCH-8` **`text as Int`** parsing (also `as UInt`, `as Path`, ...), propagating a
  failed parse (~284 `.parse_int()` sites).
- `MATCH-9` **String patterns in f-string syntax**: `if let f"{key}={value}" = line`,
  `match line { f"#define {name} {body}" => ... }`. Stay as close to Python
  as possible: the hole grammar is Python's format-field grammar read in
  reverse (as the `parse` package does), so typed holes such as `{n:d}`
  follow Python's spec letters. Holes match leftmost-shortest, the last takes
  the rest; a non-match is just a non-matching arm. Targets ~40 split-then-index
  sites, ~33 regex captures, and many `starts_with` + slice pairs.
- `MATCH-10` **`.is_empty()` on `Str`, `Bytes`, `List`, `Map`, and `Set`**, with
  a lint and autofix for `.len() == 0`, `.len() > 0`, and `.len() != 0`
  (about 615 sites).

## `TYPE`: type-level hardening

New type forms. Each keeps a runtime failure from being the first place an
invalid value is noticed. `TYPE-7` depends on the others and goes last.

- `TYPE-1` **Closed finite union types instead of `Any` at heterogeneous typed
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
- `TYPE-2` **Typed first-class callables that retain signatures and effects.** Add a
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
- `TYPE-3` **`Set[T]` with `{"a", "b"}` literals**, `in`, `|`, `&`, `-`, `.add`.
  Replaces `Map[Bool]` sets and their meaningless `true` values (about 500
  `Map[Bool]`/`set.empty()` sites).
- `TYPE-4` **`NonEmpty[T]` for collections whose first element is part of the
  contract.** A command argv is the canonical case: it is not merely a
  `List[Arg]`; it must contain an executable. Non-empty literals satisfy the
  type at check time, while converting an arbitrary list validates once.
  `first()` (and equivalent guaranteed operations) are total on
  `NonEmpty[T]`. Preserve the guarantee only through operations that
  obviously do so (for example `map` and appending); ordinary filters,
  slices, and removals return `List[T]` unless revalidated. This pairs with
  `run @argv` so an empty command can eventually become a type error rather
  than a runtime setup failure.
  `CMD-3` uses it to reject an empty command at check time.
  Build it as the first instance of one checker mechanism for validated
  types: a base type, a validation that runs once at an explicit conversion,
  literals checked statically, and a short list of operations that preserve
  the guarantee, every other operation returning the base type. `TYPE-5` and
  `PATH-12` are further instances and must not add a second mechanism.
- `TYPE-5` **Bounded scalar types, deliberately not general refinement types.** Support
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
  `MATCH-8` defines the conversion boundary it validates at. An instance of
  the validated-type mechanism of `TYPE-4`; needs it.
- `TYPE-6` **Nominal record construction as an opt-in alternative to structural
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
  Decided: `nominal type`, identity only.
- `TYPE-7` **Make `Any` opaque by default; dynamic traversal becomes an explicit
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
  Needs `TYPE-1` and `TYPE-2`.

## `MOD`: effects, modules, and inference

Module loading and contracts, effect bounds, and what private code may leave
to inference.

- `MOD-7` **Infer private proc return types**, as private proc effects already are.
  Exports keep their signatures as the contract. About 2,259
  `-> Result[...]` annotations in Laputa; the lint drops one only when the
  inferred type is identical.
  Pairs with `ERR-5`: exports spell their contract, private procs infer it.
- `MOD-8` A top-level `let x: Contract = static_module` fails lowering with
  `top_level_boundary_blocker`; the same line inside a proc works.

## `CMD`: commands, lexer, CLI, and tooling

Command words, literals, script entry points, the test API, and repository
tooling. The items are independent of each other.

- `CMD-3` **Let `@argv` provide the command head.** A spliced argv in target position
  is one complete command vector: `run @argv`, `spawn run @argv`, and
  `process.command { run @argv }` use element zero as both the executable
  target and `argv[0]`. An empty list fails explicitly (and can become a
  check-time impossibility once `NonEmpty[T]` below exists). This removes
  `process.run(process.command_argv(argv[0], argv))` and similar reconstruction
  while keeping `process.command_argv(target, argv)` for the unusual case
  where executable selection intentionally differs from `argv[0]`.
  `process.command_argv` appears in about 49 Laputa XSH files, including about
  13 files with the direct run-of-command-argv shape.
- `CMD-4` **`exit N` replaces `abort(N)`**, and `abort` is removed (~131 sites): the
  exit is deliberate, which "abort" contradicts.
- `CMD-5` **Subcommand `cli` entries.** `cli main repo check(repo: Path = default_repo()) { ... }`:
  several `cli` entries named by a subcommand path, each with generated help
  and option parsing. Replaces the `var parsed = Placeholder(...)` +
  `match cli.parse(...) { Ok(v) => parsed = v  Err(e) => return Err(e) }`
  dance (11 times in Laputa's `pm/cli.xsh`, 850 lines).
- `CMD-6` **`test.expect(ctx, src, status: N, stderr: [...])`** for script tests. The
  status is a required argument (0 for success), stderr/stdout fragments are
  optional, and any mismatch reports the full output. Replaces
  `run_script` + `assert output.status == N, output.stderr` + repeated
  `assert "..." in output.stderr, output.stderr` (1,013 `run_script` calls,
  about 587 status and 567 stderr asserts here).
- `CMD-7` `cargo dev test linux --ci` passes the host target (`aarch64-apple-darwin`)
  into the container, so it works only on a Linux host. Use
  `cargo dev test linux` on macOS. The Linux run also shares `target/` with
  the host and overwrites `target/release` with Linux binaries.
- `CMD-8` Cargo's `unused_dependencies` lint flags `mimalloc` in xsht and xshi.
  This is a false positive: the binaries use it, the libraries do not.
- `CMD-14` A size literal inside a `const` list or record infers `Int` where
  `let` infers `UInt`. `xsht fmt` moves a trailing comment on a command
  statement to the next line. `scan_number_end` appends a duration suffix to
  a float's end.
- `CMD-11` **`xsht desugar FILE`** prints a program with every sugar form
  replaced by its expansion into core forms: the formatter printing a sugar
  node's expansion instead of its operands. The output is valid XSH that
  checks: a hidden local is printed under a fresh legal name. Three uses
  ship with it. A reader, or an agent writing XSH, can ask what a form
  means. The docs generator shows each sugar form's expansion in the SPEC by
  running the command on the snippet, so the documented expansion is the
  implemented one. And a differential test over the native test corpus and
  the fuzzer requires a program and its desugared form to check identically
  and print the same output. It does not show where a control-position
  `Result` propagates; that is a typing rule, not sugar.
  Needs the sugar expansion mechanism, and `CMD-12` so the output is not
  partial.
- `CMD-13` The tour snippet `docs/snippets/tour/29-env-set.xsh` runs
  `printenv STAGE RETRIES`, which prints both values only with GNU
  `printenv`; the BSD one prints the first. `make docs` therefore writes a
  different `docs/user-tour.md` on a macOS host without GNU coreutils first
  on `PATH`, although generation is meant to be host-independent.
- `CMD-12` **Move postfix `when`/`unless` and `guard ... else` onto the sugar
  expansion mechanism.** They are sugar by `docs/DESIGN.md` but are still
  first-class statements that the checker and lowering handle by hand
  (`GuardedStmt`, `BooleanGuard`, `Guard`). Each becomes a sugar form whose
  expansion is the `if` it stands for, with the same narrowing and the same
  diagnostics, and the hand-written checker and lowering paths are deleted
  in the same change. Until then `xsht desugar` names the forms it leaves
  as written.

## `LINT`: lint accuracy and migrations

Fixes to existing lints. The corpus migrations themselves are not items: the
integrator runs them after a merge (`campaign.md`). A lint visits only the
module it is linting, through the linter's traversal; a scan of an arena
table costs files times the whole workspace.

- `LINT-2` **Filter loops become comprehensions.** Rejected: a `for x in xs if c`
  statement form. Instead `lint.prefer-list-comp` must also rewrite loops whose
  first statement is `continue unless c` / `continue when c` (83 sites) when
  the body only accumulates.
- `LINT-3` `lint.prefer-list-element-assignment` still limits its argument
  to identifiers and literals, the rule that made
  `lint.prefer-list-compound-assignment` miss nine sites in ten.
  `x.extend(a).extend(b)` chains have no single `+=` spelling and stay
  unflagged.
- `LINT-4` A `lint.prefer-guard` fix inside a single-statement match-arm
  block is not format-stable: `xsht fmt` collapses
  `0 => { return .. when c }` into `0 => return .. when c`.
- `LINT-5` `xsht lint --fix` re-parses the whole file for each fix that
  contains `(` (`minimize_fix_grouping`), which made a 1,057-site migration
  take 39 s of CPU.
- `LINT-6` SPEC does not say whether `x += e` reads `x` before it evaluates
  `e`. The implementation does; a lint relies on it.

## Laputa migrations pending

Lints already applied to this repository and not yet to Laputa, with the
sites each finds there: `lint.redundant-use-alias` 126,
`lint.prefer-list-compound-assignment` 392, `lint.prefer-read-lines` 21,
`lint.prefer-size-literal` 17 (6 with a fix), `lint.prefer-write-lines` 5
(2 with a fix), `lint.path-text-query` 4, `lint.prefer-env-string` (not
counted). `lint.prefer-repeat` finds none. `guest/qemu-dwl-foot-proof.xsh:15`
passes `Failed(phase, message)` positionally; since `ERR-1` each argument
fills its own field, where before the two were swapped.

## Open, without an accepted design

### Literal braces in interpolating strings that generate code

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

## Deferred and rejected

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
- Path and emptiness phrases (`if out is a directory`, `if p exists`,
  `when xs is empty`): `is` would gain a second meaning beside the type
  test, and a fallible filesystem call would read as a pure predicate. The
  methods `is_dir()`, `exists()`, and `is_empty()` are as short and can be
  searched for.
- Computed negative indexes: an off-by-one would read the last element
  instead of failing. Only a literal negative index counts from the end.
- A propagation rule based on the expected type (a `Result[T]` propagating
  wherever a `T` is expected): it would make every binding and argument a
  hidden exit.

From the hardening pass:

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
