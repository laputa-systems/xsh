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
| `ERR-4` | `Err(.Variant(...))` requires a declared family-typed return. There is no rule that guesses a family. |
| `TYPE-5`, `TYPE-6`, `PATH-12` | Instances of the one checker mechanism for validated types (`docs/ARCHITECTURE.md`, "Adding a validated type"); `.require(T)` is the conversion. |
| `TYPE-6` | `nominal type`. Identity only; field privacy is not part of it. |
| `SCOPE-9` | Last, and narrow: use after a consuming operation in straight-line code of one scope. |

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
| `PROP-7` | Whether a command statement takes a postfix guard (`when` is an argv word in `print "x" when verbose` today); which `if c { stmt }` sites a lint rewrites; whether `assert`, `defer`, and bindings count as simple statements; `guard c else fail "m"` against `fail "m" unless c`, which expand alike. Proposed: expression statements, assignments, and `print`/`eprint` only, with no lint for an existing `if`. |
| `PATH-9` | A parameter cannot say "label required later", so the rename needs either a label rule in the registry that one lint is driven from, or a plain rename now. Also: the eventual error code; whether `Regex.replace`, `Path.rename`, and `Path.hardlink` are included; whether the symlink fix, which swaps evaluation order, is limited to effect-free operands. Proposed: the label rule. |
| `PATH-18` | A mount that refuses `statvfs`: skip it, as `df` does, or list it without statistics. |
| `SCOPE-12` | Which rule both `tempdir` forms follow. A: `at` becomes a second head of the core scope, following `cd` and `env` (proposed; no corpus site is expected to change). B: setup failures propagate and the value is the body's tail, which splits `tempdir` from `cd` and `env` and needs the answer to `SCOPE-3`. `SCOPE-11`'s tail-block fix waits on it. |
| `MATCH-5` | The lint has no behavior-preserving fix and `xsht lint` fails on any diagnostic: add a note severity that does not fail the gate (proposed), make the rule opt-in, or leave it on and fail Laputa's lint. |
| `TYPE-3` | The set literal. `{a, b}` with bare names is already a record literal. A: braces are a set only where `Set[T]` is expected or an element is not a bare name, and the empty set is `set.empty()` (proposed). B: no literal, `set.of(a, b)`. |
| `CMD-5` | Whether defaults may be computed (the item's example is, SPEC 3.2 requires constants; proposed: allow, evaluated after parsing and shown unevaluated in help); whether a module may declare `cli` entries; whether a bare `cli main(...)` may sit beside subcommand entries (proposed: no); how a path word is spelled (proposed: snake to kebab, as options). |
| literal braces | No accepted design (see "Open, without an accepted design"). |

## `PROP`: propagation and failure flow

What `?` means, where a failure may leave without one, and the statements
that produce failures. Merged and applied to both corpora: `fail MESSAGE`
and `fail ... because CAUSE`, a `Result[Bool]` condition that propagates,
`try run...`, and the lints that remove each redundant `?`.

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

- `PROP-7` **Postfix `when`/`unless` on any simple statement**, and
  `guard cond else fail "..."` without braces:
  `print f"copying {src}" when verbose`, `fs.remove(tmp) when tmp.exists()`.
  Needs `PROP-5`.
- `PROP-9` **Value-position `run.*` forms propagate**, second step.
  `try run.text ...` captures and `lint.explicit-run-capture` has written
  `try` on every run form whose failure is kept as a value, in both corpora.
  What remains: the bare form changes meaning (`let v = run.text git describe`
  propagates) and the trailing `?` is removed by a lint (361 Laputa lines).
  Plain `run` in value position yields a `Status` and is not part of it.
  `try { run.text x }` is a nested `Result` today and changes meaning with
  the bare form, so the lint must rewrite it first.
- `PROP-10` `let v = f()?` reports the enclosing call site as the failing
  span. A failing `defer` in a `Result` proc surfaces as
  `err[runtime.error]`, not as an `Err`. A replaced error with the same kind
  and message as the one it replaces still reuses the old traceback.
- `PROP-11` Other `?` that is ceremony, by rough count over both corpora:
  `defer f()?` 667 and plain `run cmd ?` 272 behave the same without it;
  13 Laputa `Ok(v) => x = v` matches could be `x = f()?`.
- `PROP-12` `lint.prefer-fail` changes what an uncaught failure reports,
  from `Family.Variant` to `validation`, and it reads only the linted file:
  a test in another file that names the kind goes stale
  (`tests/xsh/system-report.xsh` did). A file-header comment directly above
  a family blocks nothing now, but a comment inside or beside the
  declaration still leaves the emptied family for a hand edit.

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
- `ERR-10` A public signature must spell its `Result` error type, and an
  ambiguous positional error-constructor argument is rejected; both are
  check errors now. A module loaded with `module.load` is not held to the
  first rule at run time.

## `PATH`: paths and the filesystem API

Mostly registry and standard-module work with little new syntax.

`Path` holds native bytes and `.display()` is its lossy conversion to text.
The work that made most `.display()` calls unnecessary is merged: sinks take
a `Path`, a string literal is a `Path` wherever one is expected, and path
queries are `Path` methods. `.display()` stays at real text boundaries (JSON,
`Str` parameters, string building), the one explicit point where bytes can
be lost. Automatic Path→Str conversion and a shorter alias (`.text`,
`str(p)`) stay rejected.

- `PATH-9` **Argument labels that read as a sentence**: `link.symlink(to: target)`,
  `s.replace("#undef ", with: "")`, `src.copy(to: dest)`. Ends argument-order
  bugs in symlink (~220 symlink, ~840 replace, ~160 copy calls).
- `PATH-10` **One spelling per filesystem operation**, second step.
  `lint.prefer-path-method` has rewritten `fs.op(path, ...)` to
  `path.op(...)` in both corpora for `chmod`, `copy`, `executable`,
  `exists`, `metadata`, `mkdir`, `read_text`, `remove`, `rename`, `write`,
  and `write_atomic`. What remains: remove those `fs.*` functions, keeping
  `fs.*` only where there is no single path receiver. A site with a comment
  beside the path, a literal with an escape, or an `f"..."` operand has no
  fix.
- `PATH-11` **`remove` defaults to `missing_ok: true`**, second step.
  `mkdir` already defaults to `parents: true` in both spellings, so only
  `remove` changes. `lint.explicit-missing-ok` (opt-in,
  `[lint] explicit-missing-ok`) writes `missing_ok: false` where a `remove`
  relies on today's default: 9 sites here, 14 in Laputa, not yet applied.
  What remains: apply it in both corpora, flip the default, add a
  `remove`/`missing_ok: true` row to `lint.redundant-default`, and delete
  the opt-in lint and its setting.
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
- `PATH-17` Finish the byte sinks that need the Linux container or a
  decision: `linux.mount` source, `linux.modinfo`, and `applet.su_session`
  (which of its three text parameters takes a `Path`) still take text.
  `test.run_script` and `test.expect` take `args: List[Union[Str, Path]]`
  as one signature, not the added overloads first planned, because three
  overloads made an omitted or `[]` argument ambiguous.
- `PATH-18` `fs.mounts()`, `fs.mount_for`, and `linux.disk_usage()` fail
  as a whole with `Permission denied` when `statvfs` is refused on one mount
  point. On a Linux host that runs Docker, an unprivileged user is refused on
  `/run/docker/netns/*` and on the overlay roots, so every call fails
  (`tests/xsh/stdlib/fs.xsh::test_fs_tree_metadata_install_and_locking`,
  `runtime::linux::linux_real_read_only_surfaces_work_in_container`). `df`
  skips such a mount. Decide between skipping it and listing it without
  statistics; a mount the caller named must still fail.
- `PATH-19` `lint.prefer-env-path-list` has 18 Laputa sites and no
  automatic fix, because the list drops the trailing empty entry of an unset
  variable, keeps a value that is not UTF-8, and fails on a directory that
  contains `:`. Each site needs a look. `x?.kind` after a call parses as a
  null-safe field, not as `?` and then a field.

Rejected this round: `/` as path join (keep `fp"..."`).

## `SCOPE`: scoped blocks, time, and resources

Block forms that own a resource or a deadline, and the ownership rules behind
them. Most are sugar over `defer`. `repeat N times` is merged; it runs
`range(N)`, so a negative count counts down instead of running zero times.
`atomically replace DEST as NAME { ... }` and `within DURATION { ... }` are
merged. `within` takes a duration literal or a dotted name, rejects `yield`
in its body, never interrupts a deferred action, and returns `Ok` for a body
that reaches its end, however late.

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
- `SCOPE-12` There are two `tempdir` forms that were built apart: the scope
  `tempdir NAME { ... }`, a core form over a fresh directory that returns a
  `Result`, and the sugar `tempdir NAME at PATH { ... }` over a fixed path,
  a statement whose failures propagate. Make them read as one feature: the
  same position and value rules, one SPEC section, and one lint family
  (`lint.prefer-tempdir-scope`, `lint.prefer-tempdir`).
- `SCOPE-13` `within` and `par-map`: an item failure or an early `return`
  still lets the items already running finish; only a deadline, a
  cancellation, or shutdown stops them. The fused `par-map` reduce stage has
  no native test of its own, and only the network case tests an operation
  that turns an interruption into an `Err` value. `fs.temp_sibling` names an
  unused path by chance, not by a lock. `xsht fmt` prints
  `(spawn run sh -c f"...") ?`, which the grammar does not recognize.

Deferred for further design: `atomically at DIR { ... }` (stage writes in a
hidden sibling directory, rename into place on success, remove on failure).

## `MATCH`: matching, binding, and conversion

Patterns, exhaustiveness, conditional binding, indexing, and explicit
conversion.

- `MATCH-5` **Optionals instead of `""` sentinels.** Lint `?? ""` (~475 Laputa sites)
  whose binding is later compared with `""`, and suggest optional binding:
  collapsing "unset" into "empty" makes the two indistinguishable.
- `MATCH-8` **`text as Int`** parsing (also `as UInt`, `as Path`, ...), propagating a
  failed parse (~284 `.parse_int()` sites).
- `MATCH-9` **String patterns in f-string syntax**: `if let f"{key}={value}" = line`,
  `match line { f"#define {name} {body}" => ... }`. Stay as close to Python
  as possible: the hole grammar is Python's format-field grammar read in
  reverse (as the `parse` package does), so typed holes such as `{n:d}`
  follow Python's spec letters. Holes match leftmost-shortest, the last takes
  the rest; a non-match is just a non-matching arm. Targets ~40 split-then-index
  sites, ~33 regex captures, and many `starts_with` + slice pairs.
- `MATCH-12` `Ok(...)` and `.Variant(...)` are rejected where
  `Result[T, E]?` is expected. An `if let` that lists every variant of an
  enum is `check.irrefutable-pattern-condition`, while one over an error
  family is not. `ExprIndexFromEnd` in the explicit-frame executor has no
  test that is known to reach it. The two older tests in
  `tests/xsh/lint-fix-selection.xsh` still say "warning" for
  `check.non-exhaustive-match`.

## `TYPE`: type-level hardening

New type forms. Each keeps a runtime failure from being the first place an
invalid value is noticed. `TYPE-7` depends on the others and goes last.

- `TYPE-3` **`Set[T]` with `{"a", "b"}` literals**, `in`, `|`, `&`, `-`, `.add`.
  Replaces `Map[Bool]` sets and their meaningless `true` values (about 500
  `Map[Bool]`/`set.empty()` sites).
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
- `TYPE-9` Left from typed callables and `NonEmpty`: a typed call whose
  callee is a local, a field, or a call result is tied to its type only by a
  kind check at run time; `m.handler(x)` on a module's exported callable
  value has no committed test; `lint.prefer-typed-callable` covers
  parameters, not record fields; a stage descriptor such as `map(scale)`
  still needs a function name. `Proc.call` on a proc with a non-`Result`
  return is typed `Result[Any]` but yields the raw value. `lint.unused-type`
  reports a type named only inside `Union[...]`. A mistyped `const` is
  reported twice. Unverified: `lowered_str_byte_op` picks `StrByteAt` by
  method name alone, which may mislower `Bytes.byte_at`.

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
tooling. The items are independent of each other. `test.expect` is merged
and applied here; it returns the script's output record, so a call that
needs nothing more is written `let _ = test.expect(...)?` at 87 sites.
Returning `Result[Unit]` instead would remove that, at the cost of the 384
sites that go on to read the record.

- `CMD-5` **Subcommand `cli` entries.** `cli main repo check(repo: Path = default_repo()) { ... }`:
  several `cli` entries named by a subcommand path, each with generated help
  and option parsing. Replaces the `var parsed = Placeholder(...)` +
  `match cli.parse(...) { Ok(v) => parsed = v  Err(e) => return Err(e) }`
  dance (11 times in Laputa's `pm/cli.xsh`, 850 lines).
- `CMD-7` `cargo dev test linux --ci` passes the host target (`aarch64-apple-darwin`)
  into the container, so it works only on a Linux host. Use
  `cargo dev test linux` on macOS. The Linux run also shares `target/` with
  the host and overwrites `target/release` with Linux binaries.
- `CMD-8` Cargo's `unused_dependencies` lint flags `mimalloc` in xsht and xshi.
  This is a false positive: the binaries use it, the libraries do not.
- `CMD-15` Nothing populates the checker fact `terminating_call_spans`
  since `abort` was removed; its plumbing through the checker output, lint
  options, and flow analysis (about 30 sites) can go. Lint flow no longer
  folds `guard true` and `guard false`.
  `lower_named_spread_call` clones the whole program and its bodies for
  each call.
- `CMD-16` Measure retained frontend memory for guarded statements, which
  now take several arena rows where they took one
  (`xsh-frontend-stats`).
- `CMD-17` `abort` is removed. Its diagnostic offers `exit N`, but
  `xsht lint --fix` does not apply that fix, so old code needs a manual
  rewrite.

## `LINT`: lint accuracy and migrations

Fixes to existing lints. The corpus migrations themselves are not items: the
integrator runs them after a merge (`campaign.md`). A lint visits only the
module it is linting, through the linter's traversal; a scan of an arena
table costs files times the whole workspace.

- `LINT-9` What still costs time in `xsht lint --fix` is the check of each
  file's import graph after every round (`ITER-6`). Laputa has 139 files
  that `xsht fmt --check` rejects and has never been formatted as a whole.
  `x += e` evaluates `e` first and reads `x` when the update commits; SPEC 7
  now says so, where this file once said the opposite.
- `LINT-10` Flaky under load, passing alone:
  `lint_performance::repository_lint_is_clean_within_wall_budget`,
  `desugar::the_desugared_corpus_checks_and_tests_like_the_corpus` (its
  traced system-report test), and
  `showcase/tests/test-px.xsh::test_px_default_search_matches_executable_substrings`,
  which once failed to spawn a file it had just written with
  `Text file busy`.

## Laputa

Laputa (`../laputa`) is migrated through every campaign lint and through
`lint.prefer-inferred-private-effects`, `lint.prefer-item-shorthand`, and
`lint.prefer-tempdir-scope`, one commit per rule, with its `xsht check` clean.
Sites without an autofix remain, among them `lint.prefer-tempdir` 17,
`lint.prefer-size-literal` 9, `lint.prefer-tempdir-scope` 8,
`lint.prefer-match-else` 7, and `lint.prefer-write-lines` 5; its older lint
findings were never part of the campaign.

A lint migration that rewrites a checksummed local source must update the
`sha256` its `PKGBUILD.xsh` declares (`packages/flex`, `packages/bison`,
`packages/ca-certificates` so far) and regenerate
`tests/pm/fixtures/plans/basic-aarch64.json`, which records proof hashes.
`pm/execute.xsh` carries a comment that `?` once escaped a `par-map` worker;
it does not reproduce (`tests/xsh/par-map-worker-propagation.xsh`), so the
comment can go.

## `ITER`: iteration speed

Gates and builds are the slowest part of every change, for a person and for
a lane. Measured on a ten-core M1 Pro with nothing else running: a release
rebuild of `xsh` and `xsht` after a source change takes about two minutes;
the `xsht` integration target runs for about 105 s and the debug `xsh --lib`
tests for about 40 s, each after its own build; the whole gate sequence is
15 minutes or more, and 25 to 40 with lanes building. Nothing in this
workstream is designed yet.

- `ITER-1` Measure first: `cargo build --timings` for the release build and
  each test target, and the slowest native tests and Rust tests by wall
  time. Record the numbers in `docs/TESTING.md` so a regression is visible.
- `ITER-2` A faster build for running tests. The release profile uses thin
  LTO at `opt-level = 3`, and tests refuse debug binaries
  (`tests/release_binary.rs`). Decide what a test binary needs: probably
  optimization without LTO, with more codegen units.
- `ITER-3` Less to rebuild. `src/runtime/eval/lower.rs` is 16,000 lines and
  `crates/xsht/src/lint.rs` 15,000, and a change to either recompiles its
  whole crate; every new lint already lives in its own file. The root
  package's unit tests cannot be built with `--release` at all (LLVM
  recursion), which is why they run in debug.
- `ITER-4` A faster native suite. About 1,000 tests run a whole script
  through a fresh `xsh` process (`test.run_script`); the few
  `system-report` tests take five to eight seconds each. `CMD-6`
  (`test.expect`) is the natural place to run a script test in process.
- `ITER-5` A faster `xsht` integration target, which is dominated by the
  corpus tests (lint and format invariance over this repository and
  Laputa) and competes with itself: the 15 s lint budget test passes alone
  and fails when the rest of the target runs beside it.
- `ITER-6` Check a module once per workspace. `xsht lint` still rechecks
  shared modules under every root that imports them (about 26 s of thread
  time here); a per-module checked-interface cache is the fix, and `xsh`
  startup would use the same cache.
- `ITER-8` On a musl host (Alpine) `rustc` uses musl's allocator and spends
  most of a thin-LTO build in the kernel. On a 32-thread x86_64 machine a
  release rebuild of `xsh` after touching `src/lib.rs` takes 163 s (519 s
  user, 3,059 s system) and 33 s (292 s user, 2 s system) with
  `LD_PRELOAD=/usr/lib/libjemalloc.so.2` on the `cargo build`. With the
  preload on builds only, the whole gate sequence takes about eight minutes
  there: 26 s for the release build, 51 s for the native suite, 69 s + 147 s
  for the `xsht` targets, 64 s + 8 s for the root integration target, and
  37 s + 32 s for the unit tests. Decide where the preload belongs (a
  `rustc-wrapper`, `cargo dev`, or the machine's environment); it must not
  reach the processes that tests spawn.
- `ITER-7` Run only the gates a change can affect: map changed paths to the
  rows of the table in `docs/TESTING.md`, as one `cargo dev` command.

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
