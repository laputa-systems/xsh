# XSH: ten coordinated language-ergonomics improvements

Implement all ten changes below, including the language contract, checker, verified indexed runtime, formatter, safe lint autofixes, corpus migration, and native verification. This is an implementation task, not a request to leave a plan.

One neat commit per change. Fan out 10 worktrees, one for each change, using gpt-6.1-sol (not gpt-6-sol) on medium effort. You integrate as lanes finish.

The host is **macOS aarch64**. Linux, Docker, cross-compilation, release packaging, and CI-platform changes are out of scope. Use the repository's pinned Rust toolchain and debug builds for routine work. Preserve unrelated and in-progress changes; do not reset the checkout or overwrite another task's work.

## Shared semantic foundation

Before editing individual features, identify the current owners and write the contract changes in the canonical specification. Establish one checked distinction between statement position and value position, consumed by lowering and tooling. Expressions must not acquire different meanings because an optimizer considers their values unused.

Preserve source-order evaluation, evaluation count, lazy branches, lexical scope, effect checking, typed Result propagation, cleanup order, and source diagnostics. Internal temporaries must be hygienic and must not appear as user-visible bindings or diagnostics. Reuse the existing arena, CST, checker facts, indexed verifier, execution frames, and pattern machinery. Surface syntax can lower to ordinary control flow when that is sufficient; do not build a general abstraction framework merely to share a few cases.

Implement the ten features in dependency order: first block/value classification; then named arguments, lists, slicing, destructuring, and pattern predicates; then guarded control flow, comparisons, optional postfix operations, and multi-clause comprehensions. Integrate lints and tests as each feature lands. This ordering is not permission to omit later features.

## 1. Generalize value-producing if/match branches and tails

Support ordinary multi-statement blocks as branches of expression-form `if` and `match`, and allow a final exhaustive `if` or `match` to supply a function/task/callback's value where that context consumes a value.

```xsh
# Before
pure label(code: Int) -> Str {
  match code {
    0 => return "ok"
    _ => {
      let detail = f"exit $code"
      return detail
    }
  }
}

# After
pure label(code: Int) -> Str {
  match code {
    0 => "ok"
    _ => {
      let detail = f"exit $code"
      detail
    }
  }
}
```

Explicit initializer/argument/return expression contexts consume values. In a value context, every reachable non-diverging branch must produce a compatible value; an `if` requires an `else`, and a `match` must be exhaustive. Branches that return, propagate failure, or legally transfer control do not fabricate a Unit value to force type unification. Do not resolve incompatible branches by silently widening to Any.

In ordinary statement contexts, branch bodies remain statement bodies. In Unit/Result[Unit] function tails, a bare Bool inside such a branch remains an assertion. In Bool/Result[Bool] value tails and predicate callbacks, false remains a value. Infer value-producing callback/retry block results before classifying their final booleans. Non-tail boolean statements still assert. Preserve top-level statement behavior and existing integer-exit semantics; do not make every top-level control-flow statement an implicit value/exit-code expression.

`return`, `break`, `continue`, and `?` retain their lexical targets; a value block is not an anonymous function or new error boundary. Evaluate the selected value before leaving its scope and running its defers. Apply existing implicit Ok wrapping only at its established result boundary.

Preserve record-literal parsing in match arms, including existing `{}` value literals. Do not reinterpret every braced expression as a statement block. Update parser/formatter ambiguity coverage. Add a safe tail-return lint that removes returns only where the new branch/tail semantics are equivalent, including retained annotation context and comments.

## 2. Named-argument punning

```xsh
# Before
compile(root: root, target: target, jobs: jobs)?
# After
compile(root:, target:, jobs:)?
```

Inside expression-call argument lists, `name:` immediately followed by a comma or closing parenthesis means `name: name`. Resolve the value exactly as the ordinary identifier expression would be resolved at that location; do not search caller configuration, callee locals, record fields, or ambient state. Preserve ordinary arity, default-argument, overload, duplicate-name, effect, and evaluation-order rules.

Support mixed positional/explicit-named/punned arguments wherever the existing call rules allow them, with trailing commas and multiline layouts. This is not dictionary unpacking or new command-argument syntax. Missing names must be normal source-located resolution errors.

Add an autofix for `name: name` only when the resolved identifiers match. Preserve punning in formatting rather than expanding it again. Keep source spans on the original name for diagnostics and tooling.

## 3. List concatenation and compound assignment

```xsh
# Before
files = files.push(file)
files = files.extend(more_files)
# After
files += [file]
files += more_files
```

Add List + List concatenation and corresponding `+=`, using existing element-type compatibility and expected-type inference, including contextual empty lists. Reject scalar append through `+=`: one item is written `[item]`. Do not add implicit heterogeneous widening, map merging, numeric coercions, or a public operator-overloading protocol.

Preserve value semantics. `+=` updates only the mutable target; previous bindings and aliases retain their contents. Follow the established compound-assignment target evaluation order, evaluate each target selector once, and test self-concatenation and aliases. Reject immutable/invalid targets normally.

Lower singleton append and list extension to efficient existing operations; avoid a throwaway singleton allocation or new quadratic copying when the existing ownership-aware primitive can avoid it. Keep `.push`/`.extend` available for useful expression/chaining APIs; this is a canonical local-update spelling, not a blanket removal of those methods.

Autofix resolved `x = x.push(v)` and `x = x.extend(ys)` for proven-equivalent mutable targets. Be conservative for nested/effectful target expressions or intervening reads/writes. Do not merely match repeated source text.

## 4. Finish half-open slicing using the existing `..` spelling

```xsh
# Before — data: Bytes
let header = data.slice(0, 16)
let payload = data.slice(16, data.len() - 16)
# After
let header = data[..16]
let payload = data[16..]
```

Use the existing Slice AST/parser route. Verify and complete end-to-end List slicing, preserve existing Str slicing behavior, and add Bytes with byte-indexed half-open bounds. Support both bounds, either omitted bound, and both omitted. Do not add stride syntax, general range values, inclusive bounds, or colon slices.

First pin down the current List/Str normalization and error behavior in focused tests, including negative, reversed, empty, and out-of-range bounds; preserve it. Bytes should follow the existing List bound-normalization convention with byte units. Document the existing Str indexing unit accurately and preserve it; do not convert text slicing to byte offsets as collateral work.

Evaluate receiver, explicit start, and explicit end once in their existing order. Omitted bounds do not re-evaluate the receiver. Preserve ordinary view/copy/value semantics and reuse the existing Bytes/Str view representations where applicable.

A `.slice(offset, count)` call is not automatically equivalent to `[offset..count]`. Autofixes must prove the correct end calculation, bounds/error equivalence, overflow behavior, and evaluation order. Safely fix constant prefixes and stable suffix forms; leave uncertain cases unfixed with an explanation. Keep genuinely distinct offset/count APIs rather than deleting them indiscriminately. Reconcile the stale specification entry that calls all slicing out of scope.

## 5. Chained ordering comparisons

```xsh
# Before
0 <= offset and offset < limit
# After
0 <= offset < limit
```

A sequence of `<`, `<=`, `>`, and `>=` compares adjacent values, left to right, and short-circuits at the first false comparison. Evaluate every reached operand once. `a() < b() <= c()` evaluates a, then b; it evaluates c only if the first comparison succeeds. Check each pair using the existing operator type rules, with no new numeric coercions or changes to Float ordering/NaN behavior.

Keep a single comparison unchanged. Preserve parentheses: `(a < b) < c` is not a comparison chain. Do not chain equality, membership, or pattern tests into ordering chains; require explicit grouping and give a useful diagnostic for ambiguous mixtures. Document precedence relative to arithmetic, `and`/`or`, `in`, `is`, and `??`.

Retain per-operand/comparison spans. A failed bare assertion should identify the failed adjacent comparison and evaluated operands, without evaluating skipped expressions to print diagnostics.

Autofix an `and` ladder only when the duplicated shared operand is proven stable and safe to coalesce. Two calls to a pure function are not automatically interchangeable with one call: failure behavior and cost/evaluation-count contracts matter.

## 6. Nested and renamed record destructuring

```xsh
# Before
let root = config.root
let jobs = config.build.jobs
let target_name = config.build.target
# After
let {root, build: {jobs, target: target_name, ..}, ..} = config
```

Extend existing record binding targets recursively: a field may use shorthand, `field: binding_name`, or `field: {nested bindings}`. Preserve the existing `..` ignored-extra-fields convention; do not add a rest-record capture or spread/update semantics. Support `_` discards and reject duplicate bound names.

Apply this consistently to existing record-binding positions: let, var, for, comprehension targets, and guard-let. Keep export restrictions and ordinary module namespace integrity unchanged. Function-parameter destructuring, tuple/list patterns, arbitrary refutable let patterns, and a general pattern-binding redesign are not part of this feature.

Reuse the applicable field/type/pattern machinery rather than maintaining separate field-selection rules. Evaluate the source once, check required nested fields against known schemas, retain each selected field's type, and preserve dynamic-schema validation requirements. Do not silently validate/cast Any into a typed record. Establish the whole binding successfully before exposing its names; var creates ordinary mutable local values, not aliases into a shared record.

Autofix adjacent field-extraction bindings only when the root is stable, intermediate bindings are not needed elsewhere, annotation conversions are retained, and validation/evaluation order is equivalent. Do not collapse repeated effectful root calls into one or drop a meaningful intermediate binding/comment.

## 7. Multi-clause list and map comprehensions

```xsh
# Before
var sources: List[Path] = []
for package in packages {
  for file in package.sources {
    if file.ext() == "xsh" {
      sources = sources.push(file)
    }
  }
}
# After
let sources = [
  file
  for package in packages
  for file in package.sources
  if file.ext() == "xsh"
]
```

Generalize the existing comprehension grammar to one or more `for` clauses with interleaved `if` filters. Each later clause sees earlier bindings; a filter appears after the bindings it uses. Use the same qualifier sequence for list and map comprehensions, without independently redesigning map-key expression syntax.

Semantics are exactly nested existing for/if control flow in textual clause order. Each inner iterable is evaluated anew for each reached outer binding, not hoisted or cached. A false filter prevents all subsequent clauses and the output expression from evaluating. Evaluate the output once per surviving combination. Keep deterministic encounter order; map duplicate keys retain the existing later-entry-wins policy and established key/value evaluation order.

Retain the current supported List/Stream/fallible iterable domains and their propagation/effect rules. Do not silently clone a one-shot stream, materialize every intermediate collection, add concurrency, or expose partial results after failure. Clean up consumed streams on exhaustion, failure, and early exit using existing ownership rules. Filter booleans are values, never assertions.

Add an autofix for straightforward fresh-local accumulator/nested-loop patterns where the accumulator does not escape or participate in computation and no break/continue/return or other body effect would change meaning. Keep uncertain loops. Format multiple clauses over readable lines instead of forcing dense one-liners.

## 8. Non-binding pattern-test expressions

```xsh
# Before
let succeeded = match outcome {
  Ok(_) => true
  Err(_) => false
}
# After
let succeeded = outcome is Ok(_)
```

Introduce `value is Pattern` as a Bool-producing test backed by the existing pattern matcher. Evaluate the subject once. Support resolved tag/Result constructors, existing error variants, literals, and nested non-binding record/constructor patterns. Support `value is Type` and error-facet tests by resolving the RHS through the existing type/facet namespaces and applying the existing applicable match/type-test rules. A RHS name must resolve to an applicable type/facet/constructor, not silently become an always-matching variable binding. Diagnose ambiguous names and accept existing qualified names to resolve them.

No bindings escape the expression: reject `outcome is Ok(payload)`; use `_`, literals, or further non-binding patterns instead. Record shorthand that would bind is likewise invalid. Payload extraction still uses match/guard facilities. Do not add a parallel binding form, regex matching, `is not`, or a new cast. For this pass, use `or` between complete tests instead of adding pattern-alternation syntax to the RHS.

Use ordinary boolean precedence and specify it alongside feature 5. Negation is `!(value is Pattern)`. Preserve existing nominal error/facet identity, not string-name comparisons. Reuse existing sound narrowing for stable bindings where applicable, including the selected branch of feature 10; do not infer payload variables or retain refinements across invalidating mutation.

An inspected Result is not propagated or unwrapped merely by testing it. A bare `outcome is Ok(_)` is an assertion under the prerequisite contract; a bound/returned/filtered test is a value.

Autofix trivial boolean-producing matches only when the selected pattern and complement are proved equivalent, no binding is used, and no guard/body effects or comments are lost. Do not hide actual error-handling branches.

## 9. Complete optional-aware postfix operations

```xsh
# Before — override_name: Str?
let label = if override_name == null {
  "default"
} else {
  override_name.trim()
}
# After
let label = override_name?.trim() ?? "default"
```

Extend the existing `?.` mechanism to method calls, guarding the entire call including argument evaluation. Add guarded index/slice forms `value?[index]` and `value?[start..end]`, reusing the ordinary index/slice domains. An Optional null receiver returns null without evaluating arguments, index, or bounds; a non-null receiver performs the ordinary operation and lifts its result into the existing Optional representation. Flatten only redundant Optional wrapping, as appropriate to XSH's null representation.

Guard each nullable hop explicitly: `config?.server?.host?.trim()`. An ordinary `.` does not silently become safe because an earlier hop was optional. This guards receiver absence, not missing fields, absent map keys, invalid indices, failing methods, or arbitrary runtime errors. A non-null empty list indexed at zero must behave like ordinary indexing, not return null.

The current checker also treats `?.` on Result as propagation, and the parser has a dedicated `?.require(Type)` path. Preserve those established Result meanings and enforce the ordinary typed error/effect checks. On Result receivers, guarded-looking method/index forms mean one explicit outer Result propagation followed by the ordinary operation, not Err-to-null conversion. Resolve behavior from the checked outer type. Preserve existing supported dynamic-field cases, but require sufficient checked receiver information for new method/index overloads rather than guessing the outer wrapper from Any.

Do not recursively unwrap mixed Optional/Result nesting. An Optional receiver whose called method returns Result produces Optional[Result[T,E]]; callers must handle the layers explicitly. `?` does not become a catch-all operator for that shape. Preserve the dedicated Result `.require` path; optional schema validation is not an additional feature here. Distinguish adjacent postfix spellings from command argv and spaced propagation forms, retaining existing valid grouped forms and useful diagnostics.

Autofix explicit null-check branches only with a stable receiver and equivalent fallback, laziness, evaluation count, and errors. Verify interaction with `??`, slices, named-argument punning, and value-versus-assertion contexts. An Optional[Bool] is not a bare Bool assertion.

## 10. Complete guarded value-carrying control flow

```xsh
# Before
if cached != null {
  return cached
}
# After
return cached when cached != null
```

Extend the existing `when`/`unless` guarded-statement route to `return value when/unless condition`, `break value when/unless condition` wherever value-carrying break is already legal, and `yield value when/unless condition` in stream producers. Preserve current valueless return/break/continue forms. Do not add arbitrary postfix guards to every statement or make new loop forms value-producing as collateral work.

Evaluate the condition first; evaluate the payload only in the selected branch, despite its earlier textual position. This is exactly `if condition { return value }` or its unless counterpart. Apply the existing Bool/Status condition rules and branch-local narrowing while checking the payload. False/unselected guards continue execution normally. Errors, effects, return/result wrapping, loop targets, yielding, and defers retain their normal behavior.

For run-valued payloads, do not reinterpret literal external argv words as guard syntax. Require an explicitly grouped run expression when necessary to disambiguate the payload, and document/test that spelling.

Autofix single-action if branches into guarded forms when there is no else, comment loss, or control-flow/semantic change. Do not rewrite two-action branches or make a guarded return unconditional in tail analysis. Prefer readable multiline if blocks when the proposed one-liner would become unwieldy.

## Tooling and maintained-corpus migration

Provide one stable lint per applicable transformation, following existing naming conventions. Reuse/extend current rules where they already own the behavior. Analyze checked AST facts and resolved symbols, then produce CST-preserving edits; do not use regex substitutions or match arbitrary user methods by spelling.

Common requirements are comment retention, Unicode byte-span correctness, parentheses/precedence, named-argument ordering, type-annotation conversions, shared imported-source deduplication, and ordinary command/expression boundaries. Conservative refusal to autofix is preferable to changing meaning. Recheck candidate edits; do not suppress unrelated checker failures. Nested rules must converge, not conflict or oscillate. The second fix pass must be unchanged, and formatting must preserve the new constructs.

Migrate clear instances in maintained XSH source, stdlib scripts, core, dev, tests, examples, scoped showcase code, and relevant source snippets/embedded fixtures. Do not use code-golf density as a migration goal or force every legitimate existing form into the new syntax. Keep meaningful names, comments, explicit validation/errors, and useful method chains. Do not add ten separate idiom guides or regenerate published documentation.

Update the canonical spec, relevant typing/tooling/architecture contracts, registry language-reference metadata, `xsht api` examples, and the closest tests. Ensure the existing method-preference policy does not fight operator/compound-assignment rules. Keep frontend AST/CST consumers, formatting, structural grep/refactor, checking, runtime tracing, and xshi's XSH path consistent. Do not change xshi's separate shell-subset language.

## Verification and acceptance

Use focused native XSH integration tests for language behavior and Rust tests for parser/checker/tooling/CLI/verifier boundaries. Exercise original and rewritten programs as differential witnesses where appropriate, comparing returned values, observable side-effect order/count, failures, and cleanup behavior. Negative CLI/tooling tests must be witnessed independently rather than relying solely on the new bare-assertion mechanism.

For every feature cover successful examples, rejected ambiguous/ill-typed cases, source spans, formatter round-tripping, and lint fix/no-fix/idempotence. Cross-feature tests must include Bool tails versus assertions inside nested if/match, punned arguments in skipped optional calls, guarded-return narrowing, Bytes slices behind nullable receivers, failed comparison-chain diagnostics without skipped evaluation, and renamed targets inside filtered nested comprehensions. Include ordinary xsh builds without native-test support for core syntax execution.

Verify collection alias preservation, no duplicate effectful receiver/index evaluation, retained Str slice semantics including Unicode, List/Bytes bound behavior, short-circuit failures, Result/Optional nesting, nominal pattern matching, stream cleanup, retry-local propagation, and defer failure precedence. Tests must use deterministic local inputs and test-owned temporary resources; no external network services, privileges, or destructive host-wide actions.

Follow the current Test Map and exact package ownership. Representative native gates are:

```sh
cargo build -p xsh --bin xsh
cargo build -p xsht --bin xsht
cargo build -p xshi --bin xshi

cargo test --test integration syntax::
cargo test --test integration sema::
cargo test -p xsht --test integration lint::

cargo test --test integration runtime:: -- \
  --skip runtime::coverage:: \
  --skip runtime::examples::example_corpus_is_formatted \
  --skip runtime::examples::example_corpus_lints_without_warnings \
  --test-threads=1

target/debug/xsht test --jobs 1 tests/xsh/stdlib
```

Also run the nearest changed native test files, registry/API suites, focused verifier tests, and relevant formatter/CLI tests. Adjust commands to the actual checkout and record exact failures or skipped cases. Confirm the host really is native aarch64 macOS. Do not claim Linux verification.

Respect the no-global-formatter/autofixer rule. Focused formatter/lint/autofix acceptance tests operating on isolated temporary fixtures are explicitly authorized for this task; running a whole-repository formatting/lint-fix workflow is not. Apply maintained-corpus migrations as scoped, reviewed patches. Avoid broad development commands that trigger release builds, Docker, or unrelated rewriting. Reuse existing local performance workloads where touched hot paths warrant a check; do not create an unrelated benchmarking framework or make unsupported performance claims.

Complete all ten features and the prerequisite integration. Finish with a compact feature-by-feature account of implemented semantics, retained APIs, actual corpus migrations, deliberately unfixed patterns, and commands/results. Identify pre-existing failures and remaining limitations precisely; do not report parser-only support or an unexecuted example as completed end-to-end behavior.
