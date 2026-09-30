## 1. `try`: a local Result-propagation boundary

```xsh
# Before: a helper exists only to establish a fallible region.
proc read_port() [fs, error] -> Result[Int] {
  let content = p"port".read_text()?
  content.trim().parse_int()?
}
let port = read_port() ?? 8080

# After
let port = try {
  let content = p"port".read_text()?
  content.trim().parse_int()?
} ?? 8080
```

Add `try { ... }` as an expression that evaluates its body once and produces Result[T,E]. This is local Result capture, not another exception system or a retry loop. Do not require `retry []` to express it, emit retry events for it, or add catch/finally syntax.

The body is a value block. Normal completion with T produces Ok(T). In an inferred value tail, a Result is data and remains nested unless explicitly unwrapped; `try { operation()? }` and `try { operation() }` are deliberately different. Preserve auto-propagation for non-tail Result[Unit] statements and explicitly Unit-consuming bodies. Do not force an inferred body to Unit merely to make its result discardable. Boolean value tails remain values; non-tail boolean statements retain assertion behavior. Empty bodies produce Ok(Unit).

Capture language-level propagation targeted at this region: explicit `?` and applicable statement auto-propagation, including assertion and plain-run failure. `Err(error)?` propagates into the nearest try/retry boundary; a Result-valued tail without propagation remains data. Do not catch parser/checker failures, evaluator defects, host panics, abort, or cancellation control transfers. An ordinary typed error value remains ordinary Result data, including errors describing canceled operations.

`return`, `break`, and `continue` retain lexical targets and leave the region without manufacturing an Ok. In particular, `return Err(error)` returns from the enclosing function, not merely from the try block; do not rewrite it to `Err(error)?` when those destinations differ. Nested regions capture only their own propagation. Run scope cleanups before completing the Result; apply existing primary/secondary cleanup-failure precedence. A cleanup failure without another primary failure must not disappear into Ok.

Reuse the checked error-boundary and frame/unwinding machinery behind retry. Local capture does not require the surrounding error effect solely for locally caught propagation; host effects remain checked, and an outer `?` still requires its normal effect. Infer T/E from expected types and the existing compatible error-family rules, never Any as a shortcut. Use the ordinary Error default where no narrower error type is established. Underconstrained error-only/value-producing blocks require an annotation; do not invent a success type. Model propagation and lexical transfers separately rather than catching every internal evaluator failure indiscriminately.

Offer only narrowly proved helper-elimination or empty-retry migration fixes. Preserve control-flow destinations, tail wrapping, effects, captures, and comments. Do not introduce a general inliner or call a retry-to-try rewrite trace-equivalent: genuine retry metadata is different.

## 2. Bare lexical blocks, without a `scope` keyword

```xsh
{
  let lock = acquire_lock()?
  defer release_lock(lock)
  rebuild_index()?
}
publish_index()?
```

Allow standalone `{ ... }` blocks to delimit local bindings and deferred cleanup. The example releases the lock before publishing, without an artificial conditional or helper function.

In statement position, a bare block is Unit-consuming. In an explicit value position or a genuine value-producing tail, it may produce its final value using the shared block-value rules:

```xsh
let index = {
  let scratch = prepare_scratch()?
  defer remove_scratch(scratch)
  build_index(scratch)?
}
```

**Resolve the record/map ambiguity syntactically, not by expected type or runtime values.** In positions that already require a body, braces remain body delimiters. Elsewhere, preserve existing record/map literal forms, including empty `{}`, field shorthand, keyword labels, computed keys, and spreads/updates. A statement or expression that is not a field entry can instead begin a bare block. Use a deterministic distinction in the existing parser, not general speculative parse-and-execute or duplicated parser pipelines.

Keep `{value}` as the existing shorthand record, not a single-expression block. A single identifier can be made unambiguously a block value with existing parentheses: `{ (value) }`. Other examples are `{ calculate() }` and `{ let value = calculate(); value }`. A record-valued block can contain an explicit record tail after its statements. Preserve existing record literal function tails; do not classify every opening brace in statement parsing as a lexical block. Malformed field-shaped literals must still produce literal diagnostics rather than silently becoming blocks.

Do not invent new delimiters, another introducer, or semicolon-dependent return semantics. Formatting must preserve distinctions such as `{ (value) }` rather than remove parentheses and turn a block into a record. Add focused grammar/formatting fixtures for these boundaries.

A bare block does not establish a Result boundary, add Ok wrapping, catch errors, create a function-return target, or change cwd/env. Propagation passes through to the enclosing try/retry/function boundary. Return/break/continue retain lexical destinations. Bindings remain local; legal assignments to surrounding vars retain ordinary value semantics. Do not extend imported modules' executable-statement permissions or top-level integer-exit rules.

Evaluate an outgoing value before cleanup, then run the block's defers exactly once before exposing that value. Preserve runtime-failure/cancellation cleanup and primary-error precedence. Reuse existing resource ownership/escape checks; do not claim a block makes a handle valid after its deferred cleanup invalidates it. Add no automatic destructors, resource protocol, anonymous-function frame, or thread.

Boolean tails follow their consumer: a Bool value block may yield false; a Unit-consuming block treats a bare Bool statement as an assertion. Do not infer a boolean-returning block merely to bypass statement assertions.

Autofix literal always-true conditionals used only as lexical blocks when removing the conditional preserves scope, effects, cleanup, comments, and value classification. Do not inline the block's contents into its parent. **Do not automatically move cleanup earlier** merely because a shorter lifetime looks attractive: later code may depend on it.

## 3. `assert`: a core assertion with failure-only context

```xsh
# Before
test.eq(actual, expected, message: f"package $name")?

# After
assert actual == expected, f"package $name"
```

Add `assert condition, message` as a statement. Require Bool and Str, with no truthiness or automatic Result unwrapping. In this feature the message is required: the message-free canonical form remains the bare boolean statement.

Evaluate the condition once. On true, return Unit without evaluating the message. On false, evaluate the message once and raise the same core AssertionError as bare assertions. The message supplements, rather than replaces, expression/operand diagnostics. Preserve short-circuiting, bounded rendering, and single evaluation of operands; do not evaluate skipped subexpressions for reporting.

The message is an ordinary checked expression. Include all possible effects and propagation in static checking. A failure while evaluating it follows ordinary propagation; never swallow it or reevaluate the condition. This feature does not require a special diagnostic-expression language.

Keep assertion propagation, typed error restrictions, retry/try targeting, cleanup, test reporting, and build-profile behavior identical to bare assertions. Implement without native-test support and without dispatch through the test module. This is not a value-producing replacement for result-valued assertion APIs.

Autofix custom-message test.ok/eq/ne statements only when condition/equality typing and assertion use are equivalent. Their old message arguments may be eagerly evaluated: only defer them when they are proved inert and non-failing. Otherwise preserve message evaluation in an explicit preceding binding at the original point, or decline the fix. Preserve argument ordering, dynamic/consumed Result uses, and meaningful context. Do not rewrite domain validation as an assertion.

## 4. `test`: explicit native-test declarations

```xsh
# Before
proc test_normalizes_name() -> Result[Unit] {
  normalize("  demo ") == "demo"
}

# After — same test ID
test test_normalizes_name {
  normalize("  demo ") == "demo"
}

# A test requiring its context
test writes_configuration [fs, error] { |ctx|
  let target = test.temp_path(ctx, "config")
  target.write("ready")?
  target.read_text()? == "ready"
}
```

Add top-level `test IDENT [effects]? { ... }` declarations. Their body contract is Result[Unit], with zero block parameters or one immutable TestContext parameter using the normal inside-brace header. A discard parameter is allowed. Omitted effect annotations retain the existing unrestricted-proc convention; explicit annotations are enforced normally.

Tests are explicitly registered harness entrypoints, not ordinary public functions. Do not make them callable, exportable, nestable, or automatically executed by importing a module or running a script. Recognize declarations separately from qualified `test.*` API access. Keep existing test-file discovery roots and do not expand discovery to every source file as part of this change.

Reuse the native runner's evaluator isolation, temp ownership, mocks, output capture, skip/fail classification, filtering, and coverage. Lower bodies through the same checked function/frame machinery, without a second test evaluator. Preserve normal test feature gates; disabled support must produce an actionable diagnostic rather than trying to execute a declaration as a command.

Test identity remains file plus declared name. Migration preserves the exact old name, including any test_ prefix, so filters and reports do not silently change. New names need no prefix. Reject duplicate/colliding declaration names deterministically. A test body never executes during checking or declaration registration.

Migrate genuine harness-only test procs and remove prefix-based discovery after migration. Before changing a proc, check whether other code calls it: extract shared work into an ordinary helper and keep one declared test, rather than silently removing a callable or running a test twice. Do not add parameterization, fixtures DSLs, tags, string-description IDs, or a new testing framework.

Provide migration diagnostics/tooling for legacy discovered signatures even after automatic discovery is removed. Never silently report zero tests because the file contains only unmigrated test procs. Preserve proc definitions used as normal helpers, meaningful comments, explicit early returns, and effect annotations.

## 5. `enum`: explicit tagged-union declarations

```xsh
# Before
type Mode = Fast | Thorough | Custom(Int)

# After
enum Mode { Fast, Thorough, Custom(Int) }

# Unambiguous single-variant data type
enum Token { Present(Str) }
```

Make `enum Name { Variant, Variant(T, ...), ... }` the canonical tagged-union declaration. Allow exports, multiline layouts, and trailing commas. Require at least one variant. Reuse the current nominal tag type and constructor/pattern representation; this is not an integer enum or a different runtime object.

Keep constructor names, module qualification, payload typing, equality, and exhaustiveness semantics unchanged for migrated types. Do not invent enum-name-qualified constructors as a side effect. Check duplicate variants and normal namespace collisions. Preserve existing documentation attachment and API/reference behavior.

The explicit introducer removes the need to infer tagged unions by peeking for pipes after a type declaration, and permits a single variant without a dummy alternative. Keep `type` for aliases and existing record/module schema declarations, and keep `error` for nominal error families. Do not add generics, discriminants, methods, derives, inheritance, or a Never type.

Remove the old executable `type Name = A | B` declaration form, retaining narrow migration recovery. Reuse existing internal TypeDefBody/tag storage where suitable instead of duplicating declaration machinery. A true alias such as `type Alias = Mode` remains an alias and is not a migration target.

Autofix resolved tag-union declarations while preserving variant order, payload annotations, comments, exports, and source mapping. Update all maintained declarations and fixtures. Do not infer that arbitrary identifier-shaped alias RHSs should become singleton enums.

## 6. `const`: a checked preparation-time data guarantee

```xsh
# Before: immutable, but not explicitly classified as static data.
let format_version = 1
let retry_delays = [100ms, 500ms, 1s]

# After
const format_version = 1
const retry_delays = [100ms, 500ms, 1s]
```

Add `const NAME [: Type] = expression`, including exported module constants and local constants. Unlike let, const guarantees data can be prepared without executing the script or reading ambient state. It is not merely another immutable-binding spelling.

Keep the admitted expression set bounded: scalar/path/regex literals, constant containers, references to other const declarations, constant record/enum construction, and primitive operators already supported by constant folding. Reuse and consolidate literal/default/constant-analysis machinery. Do not permit user calls, arbitrary methods, statement blocks, try/retry execution, loops, comprehensions, dynamic Any, handles, globals initialized by execution, environment reads, filesystem access, globbing, or `?`. A reference to an ordinary let is not a const guarantee, even if its initializer looks literal.

Resolve constants lexically and through normal module qualification, never ambient names. Analyze their finite dependency graph, reject cycles, and report invalid operations at their source spans during preparation. Preserve runtime operator overflow/division/type behavior rather than using unchecked host arithmetic. Require type context when an empty container cannot determine its element type. No CTFE interpreter, user macros, or new constant-only arithmetic semantics.

Store prepared immutable values in the checked program/module data. Evaluating a reference reuses that value under ordinary alias/copy-on-write semantics; mutating a var initialized from it must not mutate the constant or another instance. Path preparation retains path bytes without resolving against the preparation host/cwd. Resource handles cannot be smuggled through constant records.

Retain let for runtime initialization. An exported const remains an immutable typed value compatible with ordinary read-only module-value contracts, not a new runtime type. Local const definitions cannot capture function parameters or mutable/runtime locals.

Autofix only expressions proved within this subset whose move to preparation changes no evaluation, failure timing, or initialization dependency. Invalid constant-looking computations stay runtime let expressions unless the user explicitly chooses const. Do not force every literal local binding to const; prefer module-level configuration/protocol data and genuinely static tables.

## 7. Selective retry with `on` patterns

```xsh
let response = retry [100ms, 500ms] on (FetchError.Busy | FetchError.Timeout) {
  fetch()?
}?
```

Extend retry with an optional `on (PATTERN)` clause between the delay list and body. It selects which failed attempts are retryable using the shared **non-binding** error-pattern machinery: exact nominal variants, applicable facets, and alternatives. Parentheses delimit the pattern. Do not add callback predicates, captures, arbitrary condition expressions, another regex dialect, or a retry-policy stdlib API.

Keep the delay-list evaluation and attempt execution rules of retry. Run an attempt's defers before deciding whether to retry. On success, finish normally. On failure, a nonmatching error ends immediately with that original Err, without sleeping or executing another attempt. A matching failure consumes the next delay if one remains; otherwise return the final original Err. No on clause preserves existing retry-all-failures behavior. The delay expressions still evaluate once at retry entry under the existing rules, even if the eventual error does not match; skipping a sleep must not skip that initial evaluation.

Check the pattern against the attempt's error type. Do not match diagnostic strings or broaden typed errors to Any. Matching must have no script effects or error conversion. Pattern aliases/captures are invalid here; normal non-binding wildcard/type/facet rules apply. Give useful diagnostics for impossible patterns without silently ignoring them.

Retain nested try/retry boundaries, normal cancellation/abort behavior, delay effects, attempt numbering, and cleanup-failure precedence. A cleanup failure participates exactly as the existing retry contract prescribes; the selection filter must not conceal it. Add selection/stop-reason information to existing retry tracing rather than a separate trace subsystem.

Autofix only manually implemented selective-retry loops whose attempt count, failure classification, delay timing, effects, and cleanup are demonstrably identical. **Never add a filter to an unconditional retry as a supposedly semantics-preserving fix.** Test nonmatching first failure, matching exhaustion, mixed failures, empty delays, nested regions, and cancellation with deterministic local fixtures and fake/zero-delay clocks as available.

## Shared implementation and migration

Use the existing arena/CST, semantic checker, registry, loader, indexed store/verifier, execution frames, pattern matcher, and xsht edit machinery. Confirm owners under `src/syntax/`, `src/sema/check/`, `src/loader.rs`, `src/runtime/eval/`, `crates/xsh-registry/`, and `crates/xsht/` rather than introducing parallel representations.

Establish bare-block parsing/value classification, Result-propagation destinations, and cleanup before dependent features. Share try/retry boundary machinery without conflating their policy or trace events. Lower tests through the existing checked function representation and enums through existing tags; const preparation must not execute user code. Do not reintroduce a recursive interpreter fallback or runtime source rewriting.

For added contextual/reserved words, audit declaration-name collisions and command parsing. Preserve keyword-shaped field labels and literal external argv. Qualified `test.*` operations remain distinguishable from test declarations. Keep xshi's separate shell-subset contract unchanged and its XSH execution on the same implementation.

Each useful migration needs checked symbol/type/control-flow analysis and CST-backed edits, not regex replacement. Preserve evaluation order/count, mutable reads, short-circuiting, annotation-driven conversions, effects, typed errors, comments, and Unicode byte spans. Purity alone is not evidence that evaluation can move or disappear. Decline unsafe fixes with useful explanations.

Keep removed enum/test conventions recognizable for migration without making them executable compatibility paths. Recovery must work after removing the old public form, preserve unrelated checker errors, and never silently skip legacy tests. Recheck edits through the normal pipeline, deduplicate shared-source edits, and require fix idempotence. Update formatter, structural search/refactor, trace, and API consumers; canonical declarations must remain searchable.

Migrate clear instances in maintained stdlib/core/dev code, tests, examples, relevant snippets/embedded fixtures, and scoped showcase code. Update canonical specs, registry language-reference entries, closest architecture/testing contracts, and AGENTS/Test Map examples affected by explicit test declarations. Do not regenerate published documentation or add another authoritative language manual. Retain distinct APIs and useful explicit forms; do not impose a line-count target.

## Native verification

Prefer native XSH integration tests for language semantics and Rust tests for actual syntax/checker/tooling/verifier/CLI boundaries. Independently witness negative CLI results; do not validate try/assert/test behavior solely through the mechanisms being introduced.

For every feature include positive/negative cases, source diagnostics, formatter round-trips, safe-fix/no-fix witnesses, and second-pass idempotence. Differential tests compare values, error identity, evaluation order/count, and cleanup, explicitly accounting for intended source/trace label changes.

Cross-feature coverage must include:

- `Err(error)?` captured by the nearest nested try/retry versus `return Err(error)` escaping the enclosing function; successful, nested-Result, Bool, Unit, and underconstrained try tails; outer propagation/effect checking.
- Bare blocks versus record/map literals, shorthand and parenthesized identifier tails, nested update literals, outgoing values evaluated before defers, cleanup failures, and ordinary break/continue unwinding.
- Lazy assertion messages, messages that themselves propagate, preserved diagnostic operands, and assertion failure captured by try/retry.
- Explicit test registration with unchanged IDs, no execution during checking/import, old-proc migration diagnostics, helper extraction, and disabled-test-support behavior.
- Single-variant and imported enums; constants used in defaults without shared mutation; constant dependency cycles and invalid preparation-time computations; nominal retry patterns, nonmatching immediate stop, matching exhaustion, and cleanup before the next attempt.

Use deterministic local fixtures and test-owned temporary resources, not live network services, privileges, or destructive host-global commands. Reuse existing local workloads/counters for allocation or repeated-preparation regressions; do not build an unrelated benchmark system. Verify core features in supported ordinary xsh builds without native-test support.

