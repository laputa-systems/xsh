## 1. Typed record construction with schema-owned defaults

Move repeated construction defaults to the record declaration:

```xsh
# Before
type BuildOptions = {
  root: Path,
  jobs: Int,
  verbose: Bool,
  features: List[Str],
}
let options: BuildOptions = {
  root,
  jobs: 4,
  verbose: false,
  features: [],
}

# After
type BuildOptions = {
  root: Path,
  jobs: Int = 4,
  verbose: Bool = false,
  features: List[Str] = [],
}
let options = BuildOptions(root:)
```

Add named-field construction for user-defined named record schemas, including qualified imported schemas. Constructor arguments use ordinary named arguments and punning; positional arguments, unknown fields, duplicate fields, and missing required fields are errors. Zero-argument construction works when every field has a default.

Resolve constructors through schema symbols, not capitalization or dynamic name lookup. Aliases to a named schema resolve to its defining constructor/default metadata rather than creating a competing identity. Diagnose callable/type name collisions instead of silently changing resolution. Constructors are static construction syntax, not new first-class callable objects.

Keep defaults deliberately bounded: scalar literals, signed numeric literals, recursively literal lists/records, and references to previously declared immutable constants composed from those forms. Resolve constants in the declaration's lexical module. No function calls, ambient reads, mutable captures, field-to-field dependencies, or `?` in defaults. Validate defaults once against field types, including contextual empty collections; reuse constant-analysis machinery instead of building general compile-time execution.

Evaluate supplied arguments exactly once in source order. Fill omitted fields from validated defaults. Defaults obey value semantics: updating one constructed value must not mutate another, even when immutable storage is shared.

**Defaults apply only to explicit construction.** They do not make fields optional, change record assignability, fill missing JSON fields, or alter `.require(Schema)`. Ordinary record literals and schema validation retain their explicit missing-field behavior. This is not a cast from dynamic input or an error-family constructor redesign.

Autofix clearly schema-typed record construction to a constructor call when equivalent. Omit an explicit default only when its expression is a proven identical constant; never drop an evaluation merely because its eventual value resembles a default. Preserve annotation-driven conversions and argument order.

## 2. Direct, typed map iteration

```xsh
# Before
for key in counts.keys() {
  let count = counts.get(key)?
  print f"$key=$count"
}

# After
for {key, value: count} in counts {
  print f"$key=$count"
}
```

Make `Map[T]` an iterable in ordinary `for` loops and comprehension clauses. Every item has structural type `{key: Str, value: T}`. A simple binding receives that same record; semantics must not depend on whether the loop target uses destructuring. Values of `T` that happen to be records or Results remain values, not additional wrappers to flatten.

Iterate in the same deterministic order as `Map.keys()` and `Map.values()`. Evaluate the source once and iterate a snapshot of its bindings. Reassigning the original map during the body does not alter that snapshot's keys or stored values. Retain existing value/resource semantics; a snapshot does not deep-copy live handles.

Use a cursor over retained map storage, not an allocated keys list plus repeated lookups or a fully materialized list of entry records. Construct only the current item when needed. Preserve cleanup on early exit.

Support `Result[Map[T], E]` at these iterable boundaries consistently with other fallible iterables: propagate failure, preserve E, and enforce the applicable error effects. Do not reinterpret failure as an empty map. Do not change unrelated pipeline map-source semantics or introduce a user-defined iterable protocol.

Keep `.keys()`, `.values()`, and `.get()` for their distinct uses. Autofix key-loop-plus-lookup only when the map remains stable across the relevant loop and lookup effects/failures cannot change. A loop that intentionally reads an updated map is not equivalent to snapshot entry iteration.

## 3. List-literal splicing

```xsh
# Before
let argv = ["cc"]
  .extend(flags)
  .extend(["-o", output_name])
  .extend(source_names)

# After
let argv = ["cc", @flags, "-o", output_name, @source_names]
```

Support `@expression` elements inside ordinary list literals, reusing the explicit list-splice vocabulary. Each splice requires a List and inserts exactly its elements at that position. Ordinary list-valued elements without `@` remain nested lists. Accept several splices, empty splices, multiline expressions, and trailing commas.

Apply normal homogeneous element typing and expected-type inference across scalar elements and spliced elements. Do not introduce additional Any widening, display conversion, or argv conversion. This constructs a List; only a later process boundary converts its elements to arguments.

Evaluate every ordinary element and splice expression once, left to right. A failure stops evaluation before subsequent elements. Require explicit Result handling, for example `@(load_flags()?)`. Reject implicit splicing of Stream, Map, Str, and Bytes; stream materialization remains explicit.

Use one append/build path with appropriate checked capacity growth, avoiding chains of intermediate lists. Preserve aliases and value semantics. Do not add conditional literal elements, call-keyword unpacking, or arbitrary comprehension grammar within a mixed literal.

Autofix compatible list concatenation/extension chains when evaluation order, element types, and annotation conversions are preserved. Do not introduce a splice where the original code intentionally appended a list as one element.

## 4. Pattern-bound conditionals and loops

```xsh
# Before
match outcome {
  Ok(value) => consume(value)?
  Err(_) => {}
}

# After
if let Ok(value) = outcome {
  consume(value)?
}
```

Add `if let PATTERN = expression { ... }`, optional `else`/`else if let`, and `while let PATTERN = expression { ... }`. Reuse normal constructor, record, literal, type/facet, wildcard, and sequence patterns where legal. Do not create a second pattern syntax.

This is **literal pattern matching**, not implicit Result/Optional unwrapping. `Ok(value)` selects an Ok payload; a bare binding pattern matches the entire subject. Pattern mismatch selects the else branch or ends the loop. An error while evaluating the subject still follows ordinary error propagation; explicitly written `?` retains its meaning.

Evaluate an if-let subject once. Reevaluate a while-let subject once per condition check, including after continue. Bind successful captures immutably and atomically for that branch/iteration only. Failed matches expose no partial bindings, and captured names do not leak into else branches or subsequent statements.

Reject or clearly diagnose irrefutable binding conditions that cannot test anything. Keep established guard-let behavior separate; do not silently reinterpret it through these new constructs. Do not add condition-binding chains in this feature.

Support value-producing if-let where an ordinary if expression is legal, requiring an else and compatible branch values. While-let remains a statement. Preserve lexical targets, source spans, refinements, boolean-value/assertion classification, and per-iteration cleanup.

Autofix two-arm matches with one selected pattern and a genuine complement, retaining the else when it handles anything meaningful. Do not turn an error handler into an empty branch or erase a match guard.

## 5. List patterns for structured argument and token parsing

```xsh
# Before
if argv.len() == 2 and argv[0] == "build" {
  let target = argv[1]
  build(target)?
}

# After
if let ["build", target] = argv {
  build(target)?
}
```

Add List patterns to the shared matcher:

```xsh
[]                       # Exactly empty.
[first, second]          # Exactly two elements.
[head, ..tail]            # At least one; bind the remaining List.
["build", target, ..]     # Ignore any trailing elements.
```

Require a single trailing rest marker, either `..` or `..name`. Support nested List/record/constructor patterns and ordinary element literals. No middle-rest patterns, sequence concatenation patterns, or new tuple type.

Use them in match, if-let, while-let, and non-binding pattern tests. Named element/rest bindings are forbidden in non-binding tests. Do not add an implicitly failing ordinary `let [a, b] = values`; refutable sequence binding belongs in a construct that handles mismatch.

Require List subjects or the existing explicit dynamic-pattern boundary. Do not consume Streams, split Str into characters, or coerce Bytes into integers. Preserve known element types; rest bindings have List[T]. Type patterns may refine genuinely dynamic elements using the shared rules.

Check length before accessing elements. A structural mismatch is a nonmatch, never an indexing exception. Publish bindings only after the whole pattern succeeds. Avoid allocating rest copies for unsuccessful matches; bound remainders retain ordinary list value semantics.

Extend exhaustiveness/reachability checking conservatively. Recognize a catch-all, `[..]`/`[..rest]`, and the complete `[]` plus `[_, ..]` partition; do not claim arbitrary literal-element patterns exhaust List[T]. Require a catch-all where coverage cannot be established.

Autofix straightforward stable-list length/index tests and extraction only with proved equivalent bounds, evaluation count, short-circuiting, and branch behavior. Preserve guards and validation of dynamic data.

## 6. Error-aware fallback blocks

```xsh
# Before
let config = match load_config() {
  Ok(value) => value
  Err(failure) => {
    eprint f"using defaults: ${failure.message}"
    default_config()
  }
}

# After
let config = load_config() ?? { |failure|
  eprint f"using defaults: ${failure.message}"
  default_config()
}
```

Extend Result fallback with `result ?? { |error_name| statements; tail_value }`. The block requires exactly one parameter, which may be `_` to discard the error. Its `{ |...|` prefix distinguishes it from a record-valued fallback. This form requires Result; retain ordinary Optional fallback without inventing an error payload for null.

Evaluate the Result once. On Ok, return its payload without running the block. On Err, bind the exact nominal error value and evaluate the block once. Non-diverging tails must have the compatible success type; no hidden rewrapping, Any widening, or conversion of failures to null.

The block is a lexical value block, not a closure or fresh Result boundary. `return`, legal loop transfers, and `?` retain their surrounding targets, including retry-local propagation. Defers run on block exit. A boolean fallback tail is a value when the result consumer expects Bool.

Preserve `??` precedence and right associativity. Scope the error parameter only to the fallback block and retain useful source attribution without automatically replacing or swallowing handler failures.

Autofix identity-Ok matches with one equivalent Err handler, preserving messages, guards, effects, and control flow. Matches that transform success values or distinguish multiple error cases are not automatically this shape.

## 7. Deferred cleanup blocks

```xsh
# Before
proc cleanup(lock: Path, scratch: Path) [fs, error] {
  lock.remove()?
  scratch.remove()?
}
defer cleanup(lock, scratch)

# After
defer {
  lock.remove()?
  scratch.remove()?
}
```

Allow a statement block after `defer`. Register it without executing its body, then execute it at the end of the registering lexical scope through the ordinary cleanup machinery. It participates as one action in LIFO ordering.

Resolve names lexically at registration; read captured local values when cleanup runs, with their owning scope retained through cleanup. For registration-time snapshots, users bind an immutable value explicitly. Block-local declarations remain block-local. Preserve existing expression-form defer semantics rather than changing them to accommodate blocks.

Check every deferred effect even when normal execution might never leave the scope successfully. Require a Unit-compatible body. Apply ordinary assertion and Result[Unit] statement behavior; `?` fails this cleanup action and is reported by the cleanup runner, not used to bypass other registered cleanups.

An error aborts the remaining statements in this one block. Other cleanup actions must still run; retain primary-error precedence and secondary cleanup reporting. Do not merge adjacent defers automatically: separate registrations have different order and failure behavior.

Reject return/yield and transfers targeting outside the deferred block. Permit local loops and their local break/continue, and nested defers with ordinary scope behavior. Do not add asynchronous cleanup, destructor inference, or a resource-management framework.

A narrow helper-to-block suggestion may be offered for a private, single-use cleanup helper when binding timing, effects, control flow, and diagnostic attribution are preserved. Otherwise leave helpers alone; do not implement a general inliner solely for this feature.

## 8. Explicit yield delegation

```xsh
# Before
for entry in entries {
  yield entry
}

# After
yield @entries
```

Inside a stream producer, `yield @expression` delegates a List[T] or Stream[T] into the enclosing Stream[T], emitting its elements rather than one collection value. Ordinary `yield collection` keeps its existing meaning when the output item type is itself a collection.

Evaluate the delegated expression exactly once when execution reaches the statement. A List emits in order; a Stream is pulled one item at a time only as the downstream consumer requests items. Do not pre-collect, buffer the entire child, or start a worker. Result-valued sources require explicit propagation: `yield @(make_entries()?)`.

Use the existing producer cursor/frame and cancellation machinery. On normal exhaustion, finish the child and continue the parent. On early downstream termination, close the active child and unwind the parent in the established inner-to-outer order, running each defer once. Preserve source and error identity for failures during child creation or later pulls.

Retain stream ownership/alias rules and enforce output-type and effect compatibility. Chained delegation must not introduce native recursion proportional to delegation depth or a second evaluator.

Autofix only transparent forwarding loops: no transformations, filters, additional effects, unused cleanup actions, or control transfers. Preserve any necessary Result propagation and binder-level conversions. Verify early termination and late-error timing, not merely final collected output.

## 9. Prepared regex literals

```xsh
# Before
let assignment = regex.compile(r"^\s*[A-Z_]+=")?

# After
let assignment = rx"^\s*[A-Z_]+="
```

Add `rx"..."` and `rx"""..."""` literals producing Regex. Their contents follow raw-string delimiter rules: no string escape decoding or interpolation. Backslashes and inline regex flags belong to the regex engine. Do not introduce slash-delimited regexes or an alternative matching dialect.

Use XSH's existing regex compiler and its pinned engine/settings; `src/modules/regex.rs` is a starting owner. Keep dynamic `regex.compile` for runtime pattern strings. All existing Regex operations retain their behavior.

Validate and compile literals during checked program/module preparation, never on every expression evaluation. Carry the prepared immutable Regex into indexed execution. Repeated calls and supported evaluator reuse should share it. Avoid independently compiling the same occurrence in the checker, lowering, and runtime; bounded program-local reuse is sufficient, not a process-global unbounded cache.

Malformed literals are source-located preparation errors, including in unreachable code. `xsht check` must diagnose them without executing scripts. Retain literal source text/spans for formatting and diagnostics, and ensure ordinary builds do not depend on the test feature.

Autofix only statically known, valid, directly propagated compile calls when the decoded pattern can be represented exactly and no Result-valued handling is being preserved. Re-encode escaped input accurately. Leave invalid-pattern tests, dynamic strings, consumed Results, custom contexts, and error-recovery code alone. Do not claim that moving an invalid regex failure to preparation preserves behavior.

## 10. Return-type inference for private pure helpers

```xsh
# Before
pure normalized_name(name: Str) -> Str {
  name.trim().lower()
}

# After
pure normalized_name(name: Str) {
  name.trim().lower()
}
```

Permit omission of a return annotation on non-exported pure functions. Infer from the body using known parameter types and checked callee signatures, not from call sites, runtime values, or external callers. Parameters retain their normal annotation/default rules.

Collect explicit return values and reachable fallthrough tails, applying compatible branch unification without silently choosing Any. A final boolean in this inference context is a value, not an assertion. Do not convert non-tail statements into return values. Reject mixed value/missing-return paths when no consistent result exists.

Infer concrete return shapes only. If empty collections, error-only returns, contextual record construction, or propagation leave the result/error shape underdetermined, request an annotation. Do not infer a new implicit Result boundary merely because the body uses `?`, and do not remove a declared conversion or implicit Ok wrapper by changing inference rules.

Process local dependencies independently of declaration order. For recursive dependency components, require annotations on their otherwise unannotated return signatures rather than introducing polymorphic-recursion inference. Cycles must produce actionable diagnostics, not guessed types or repeated analysis.

Exported pure functions and module-contract signatures still require explicit returns. Do not change proc default returns, pure/effect restrictions, unrestricted-proc semantics, or introduce effect inference as collateral scope.

Make inferred types available to all downstream tooling and lowering. Update annotation tooling so it can render these inferred returns when requested without fighting a brevity lint.

Offer annotation removal only after rechecking the definition and affected calls without it and confirming the exact relevant type/behavior is preserved. Retain annotations supplying empty-container types, named-schema constraints, result wrapping, or overload context.

