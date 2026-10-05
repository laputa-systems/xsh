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

Closed. Each is written on its item below.

| Item | Decision |
|---|---|
| `PROP-8`, `PROP-9`, `MATCH-8` | One propagation rule: a failure leaves a function only through a form visible at the site. See `PROP`. |
| `PATH-11` | The defaults flip only after a lint has made today's defaults explicit at every call that relies on them. |
| `PATH-12` | The type is named `RelPath`. |
| `PATH-13`, `MATCH-10` | Methods, not phrases: `p.is_dir()` and `xs.is_empty()`. `is` keeps one meaning, the type test. |
| `ERR-4` | `Err(.Variant(...))` requires a declared family-typed return. There is no rule that guesses a family. |
| `MATCH-2` | Ordinary and Str-backed enums now, as a direct change of severity with no autofix. An exactly known error family follows once `ERR-5` has merged. |
| `MATCH-7` | Only a literal negative index counts from the end. A computed negative index still fails. |
| `TYPE-2` | The spelling written on the item: `proc(...) [effects] -> T`. |
| `TYPE-4`, `TYPE-5`, `PATH-12` | One checker mechanism for validated types, built with `TYPE-4`. |
| `TYPE-6` | `nominal type`. Identity only; field privacy is not part of it. |
| `SCOPE-9` | Last, and narrow: use after a consuming operation in straight-line code of one scope. |
| `CMD-4` | `exit N` is a core statement. `abort`, with its `force` argument, is removed; nothing replaces `force`. |

Still open; the item is not assigned until it is closed here:

| Item | Open point |
|---|---|
| `SCOPE-3` | `collect { }` needs an expression-level sugar form whose body's `yield`s are reinterpreted. The mechanism is statement-level. Decide whether to build that, or make `collect` a core expression. |
| `SCOPE-5` | The context-manager protocol, and how `with x = m { }` differs from `with ... else`, which always has its `else`. |
| `SCOPE-8` | Exact spelling; whether jitter is the default. |
| `SCOPE-9` | How a parameter distinguishes borrowing a handle from taking it. |
| `SCOPE-10` | Whether a deliberate `exit N` runs `errdefer` actions. |
| `TYPE-5` | Range syntax and inclusive-bound spelling. |
| `TYPE-7` | Spelling of the local dynamic-traversal escape hatch. |
| literal braces | No accepted design (see "Open, without an accepted design"). |

## `PROP`: propagation and failure flow

What `?` means, where a failure may leave without one, and the statements
that produce failures. The lints for a redundant statement `?`, a redundant
`}?`, and a re-propagating `match` are merged; applying them is the first
migration in `campaign.md`.

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
- `PROP-10` `let v = f()?` reports the enclosing call site as the failing
  span. A failing `defer` in a `Result` proc surfaces as
  `err[runtime.error]`, not as an `Err`. A replaced error with the same kind
  and message as the one it replaces still reuses the old traceback.
- `PROP-11` Other `?` that is ceremony, by rough count over both corpora:
  `defer f()?` 667 and plain `run cmd ?` 272 behave the same without it;
  13 Laputa `Ok(v) => x = v` matches could be `x = f()?`.

## `ERR`: error families and variants

How error families are declared, constructed, matched, and named in
signatures.

- `ERR-4` `Err(.Variant(...))` needs a family-typed return. Laputa's ~652
  `Err(Family.Variant(...))` sites mostly return `Result[T]` (plain `Error`),
  which names no family, so they cannot use it. Decide between family-typed
  returns in Laputa and a different rule; "the one visible family with this
  variant" would be a guess and covers only ~266 sites.
  Decided: the inferred form requires a declared family-typed return, and
  there is no guessing rule. Laputa gains family-typed returns where `ERR-5`
  narrows a signature; other sites keep the qualified spelling.
- `ERR-5` **Public Result signatures must spell their error type.** The
  warning and its autofix (`, Error`) are merged and both corpora are
  migrated. The check error is prepared on the branch `held/err-check-errors`
  together with the error for an ambiguous positional error-constructor
  argument; it needs a rebase. A module loaded with `module.load` is not
  checked at run time. Pairs with `MOD-7`.
- `ERR-6` The inferred-variant and positional-constructor lints are opt-in
  (`[lint] prefer-inferred-variants`, `prefer-positional-constructors`)
  because the repository lint gate requires zero diagnostics; enable them by
  default after migrating the corpus (347 + 32 sites here, 13 + 7 in Laputa),
  then delete the settings.
  Runs as a migration once `ERR-3` and `ERR-4` have merged.
- `ERR-8` `lint.prefer-implicit-message` is opt-in
  (`[lint] prefer-implicit-messages`) because exported families get a note
  and no fix: 184 message-only variants here (14 exported), 79 in Laputa (43
  exported). Migrate both corpora, enable it by default, and delete the
  setting.
- `ERR-9` Qualified imported-enum arm heads (`k.File => ...`) do not count
  toward exhaustiveness. The `record.require` migration fix is offered only
  for a call in the root program, so a module reached only through its
  importer keeps the site until it is linted directly.

## `PATH`: paths and the filesystem API

Mostly registry and standard-module work with little new syntax.

`Path` holds native bytes and `.display()` is its lossy conversion to text.
The work that made most `.display()` calls unnecessary is merged: sinks take
a `Path`, a string literal is a `Path` wherever one is expected, and path
queries are `Path` methods. `.display()` stays at real text boundaries (JSON,
`Str` parameters, string building), the one explicit point where bytes can
be lost. Automatic Path→Str conversion and a shorter alias (`.text`,
`str(p)`) stay rejected.

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
- `PATH-17` Finish the byte sinks: `process.which`, `linux.mount` source,
  `linux.modinfo`, `applet.su_session`, and `test.run_script` args still
  take text; argv lists built separately can now be
  `List[Union[Str, Path]]` (list parameters need an added overload, because
  `List[Str]` is not `List[Union[Str, Path]]`). `env.PATH` `append`,
  `prepend`, and `in` still reject a string literal, against the one
  literal rule. Where overloads disagree (`hash.sha256`, `fs.executable`) a
  literal stays `Str`.

Rejected this round: `/` as path join (keep `fp"..."`).

## `SCOPE`: scoped blocks, time, and resources

Block forms that own a resource or a deadline, and the ownership rules behind
them. Most are sugar over `defer`. `repeat N times` is merged; it runs
`range(N)`, so a negative count counts down instead of running zero times.

- `SCOPE-3` **`collect { ... }` blocks**: the block's `yield`s (including
  `yield x when c` and yields inside loops) append to a List that is its
  value; lint + autofix for local lists built only by `xs = xs.push(..)` /
  `xs += [..]` (~2,858 `x = x.push(` lines in Laputa).
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
- `SCOPE-10` `errdefer` runs on `exit N` and on cancellation, as it did on
  `abort(N)`. Decide whether a deliberate exit should run error-only
  cleanup.
- `SCOPE-11` A bare block that is the tail of a `Result[T]` function rejects
  a `Result[T]` tail; the same holds for a `tempdir` body there. The grammar
  accepts `for x in xs |> sort-by { |c| ... }` with the block as the loop
  body while the parser gives it to the stage. Unconfirmed: a module's
  top-level defers may not run when its top level fails.

Deferred for further design: `atomically at DIR { ... }` (stage writes in a
hidden sibling directory, rename into place on success, remove on failure).

## `MATCH`: matching, binding, and conversion

Patterns, exhaustiveness, conditional binding, indexing, and explicit
conversion.

- `MATCH-2` **Statement matches over closed types are exhaustive errors, not
  warnings.** A non-exhaustive statement match on an ordinary or Str-backed
  enum becomes a check error, so adding a variant breaks an old match at
  check time; a deliberate catch-all is `else =>`. Change the severity
  directly: the warning finds no site in either corpus, and an autofix that
  appends `else => {}` would not preserve behavior, because an unmatched
  statement match fails today with `match-no-arm`. An exactly known error
  family follows once `ERR-5` has merged.
- `MATCH-3` Two codes have per-site severity: `check.non-exhaustive-match` and
  `check.reveal-type`.
  `MATCH-2` removes the `check.non-exhaustive-match` case.
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
- `MATCH-11` A Result `guard let` whose block falls through passes the
  check. A `Result[T]?` call propagates `Err` at a plain `let`. `x == null`
  narrowing is lost in the final `else` after an `else if`.

## `TYPE`: type-level hardening

New type forms. Each keeps a runtime failure from being the first place an
invalid value is noticed. `TYPE-7` depends on the others and goes last.

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
- `TYPE-8` Lowering types a narrowed slot by its declared union, so it
  accepts any method some member has. Publish narrowed types as checker
  facts and have lowering consume them.

## `MOD`: effects, modules, and inference

Module loading and contracts, effect bounds, and what private code may leave
to inference.

- `MOD-9` `lint.prefer-inferred-proc-return` is opt-in
  (`[lint] prefer-inferred-proc-returns`): about 162 provable sites here.
  Decide whether it becomes a default, then migrate. A failed
  `.require(Contract)` raises a plain `Error` with facets; a built-in
  `ContractError` family would let user code match it by name.

## `CMD`: commands, lexer, CLI, and tooling

Command words, literals, script entry points, the test API, and repository
tooling. The items are independent of each other.

- `CMD-4` **`exit N` replaces `abort(N)`.** The `exit` statement and
  `lint.prefer-exit` are merged. The removal of `abort` and of its `force`
  argument is prepared on the branch `held/cmd4-abort-removal`; it merges
  after both corpora are migrated with the lint, and it hand-edits the
  script strings and fixtures the lint cannot reach.
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
- `CMD-15` After `abort` is removed nothing populates the checker fact
  `terminating_call_spans`; its plumbing through the checker output, lint
  options, and flow analysis (about 30 sites) can go.
  `dev/tests/test-targets.xsh::test_dev_main_target_override_reaches_context`
  overflows the stack on a debug binary. Lint flow no longer folds
  `guard true` and `guard false`.
- `CMD-16` Measure retained frontend memory for guarded statements, which
  now take several arena rows where they took one
  (`xsh-frontend-stats`).

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
- `LINT-7` `xsht lint --fix --only CODE` rejects every fix round in a file
  that has any unselected check warning. `xsht lint --fix` ignored SIGTERM
  during one long run.

## Laputa

Migrated through `lint.prefer-tempdir`, one commit per rule, with its
`xsht check` clean. Sites without an autofix remain: `lint.prefer-tempdir`
17, `lint.prefer-size-literal` 9, `lint.prefer-match-else` 7,
`lint.prefer-write-lines` 5. `packages/flex/files/flex.xsh` and
`packages/bison/files/bison.xsh` are checksummed local sources and
`tests/pm/fixtures/plans/basic-aarch64.json` records proof hashes, so a
migration that rewrites them must update the `sha256` in their
`PKGBUILD.xsh` and regenerate the fixture. `pm/execute.xsh` carries a comment
that `?` once escaped a `par-map` worker; it does not reproduce
(`tests/xsh/par-map-worker-propagation.xsh`), so the site can migrate and
the comment can go.

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
