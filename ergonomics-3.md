## 1. Boolean guards with an explicit failure branch

```xsh
# Before — jobs: Int
if jobs <= 0 {
  return error.fail("jobs must be positive")
}

# After
guard jobs > 0 else {
  return error.fail("jobs must be positive")
}
```

Add `guard condition else { ... }`, separately from guard-let. Use ordinary Bool/Status condition rules; no truthiness, implicit Result unwrapping, or special assertion error. The author controls the failure branch and error identity.

Evaluate the condition once. Success continues after the guard; failure executes the else block. Require every reachable path through else to leave the guard's enclosing continuation: return, applicable break/continue, or another operation the checker proves terminates. Falling through else is a checker error. An arbitrary fallible call is not proof of termination merely because it could fail.

Apply success refinements to subsequent statements and failure refinements inside else. Restrict refinements to sound existing null/type/pattern/field-presence rules and invalidate them on relevant mutation. Do not turn a dynamic shape test into an unchecked schema cast. Guard conditions and their boolean subexpressions are values, not assertions.

Keep lexical return/loop/retry targets, effects, and defer behavior. The guard does not establish a new Result boundary, catch scope, or value-producing expression. Imported-module executable-statement restrictions remain unchanged.

Autofix a leading negative if with a definitely exiting body when it produces the same control flow. Preserve the condition evaluation and exact error. Do not invert ordering operators unless their types justify equivalence; negating a Float comparison is not interchangeable with the opposite ordering when NaN is possible. Never replace meaningful domain validation with a bare assertion, or vice versa. Prefer an existing guard-let when the operation is actually binding a fallible value.

## 2. Computed-key map literals

```xsh
# Before
var counts: Map[Int] = {}
counts = counts.set(name, count)
counts = counts.set("total", total)

# After
let counts: Map[Int] = {
  [name]: count,
  "total": total,
}
```

Add computed entries `[key_expression]: value_expression` to brace literals. Any computed-key entry makes that literal a Map; its keys must be Str. Also support nonempty constant-key literals in an expected Map[T] context. Otherwise preserve the existing Record interpretation and empty-literal contextual rules.

Map literals may mix computed keys, constant keys, and `...map_expression` spreads. Spread operands in a Map literal must be Map values, not arbitrary records or dynamic objects. A spread-only literal without Map context must not silently change the existing record-spread interpretation; require an annotation where needed.

Check all values against T when available; otherwise use the existing justified common-element inference. Do not widen incompatible concrete values to Any just to accept the literal. No implicit key conversion, JSON-path parsing, or dotted-key nesting: a quoted key containing a dot is one key.

Evaluate entries left to right, and each computed key before its value. Evaluate every reached expression once, including values later overwritten. Later entries/spreads replace earlier duplicate keys, matching Map.set semantics. Stop on failure without evaluating later entries. Iteration order remains the map's canonical deterministic key order, not construction order.

Build the map without chains of intermediate maps. Reuse brace-literal/CST infrastructure while representing computed keys explicitly rather than inventing record-field names from source text.

Autofix fresh, non-escaping map initialization followed by set calls or equivalent set chains only when earlier bindings are not observed, annotations are retained, and evaluation order is unchanged. Do not replace mutation of a live map whose intermediate states are meaningful.

## 3. One block-parameter convention

```xsh
# Before
with config = load_config() {
  consume(config)?
} else |failure| {
  eprint $failure.message
}

# After
with config = load_config() {
  consume(config)?
} else { |failure|
  eprint $failure.message
}
```

Use `{ |parameters| ... }` wherever a construct supplies block parameters. Migrate the exceptional before-brace parameter form on with/guard-let error handlers to this convention. Reuse the normal block-header parser; do not introduce another callback form or extend which constructs supply values.

Recognize the header at the start of the block after allowed whitespace/comments. An error handler accepts zero parameters or one parameter of its existing error type; `_` may explicitly discard that input. Preserve the existing arities/types of other parameterized blocks. Plain if/else, boolean-guard failure branches, and defer blocks do not acquire an error parameter and must reject inappropriate headers.

Bind parameters immutably in the block's lexical scope. Keep error selection, body execution, effects, control-flow targets, and cleanup identical. Do not add a function-call frame or change Result propagation. A parameter cannot escape the handler, replace an outer binding, or capture a nonexistent error. Preserve namespace and duplicate-binding checks.

Use one authoritative representation of block parameters. Remove redundant owning-node parameter storage where it duplicates the block header, while retaining owner-specific input typing. Keep formatted strings, record literals, pattern alternatives, and pipelines distinct from block headers.

Make the outside-brace spelling invalid in ordinary source, with an actionable migration diagnostic. The linter must still recognize it through narrowly scoped recovery, move only the header/braces, preserve comments and body spans, and recheck the rewritten source. Do not retain an executable compatibility mode. Unsafe comment layouts may receive guidance without an autofix.

Test with and guard-let handlers, omitted/discard parameters, nested scopes, comments/newlines, invalid arity, and blocks with no supplied input. The inside-header form must never silently become a pipeline expression.

## 4. Field labels are labels, not variable declarations

```xsh
# Before
let row = {"type": kind}
let label = row.get("type")?

# After
let row = {type: kind}
let label = row.type

# A schema and an explicitly renamed binding can keep a wire-format name.
type Entry = {type: Str, size: Int}
let {type: entry_kind, size, ..} = entry
```

Allow keyword spellings such as `type`, `in`, and `match` in explicit field-label positions. Apply the same rule to record schemas and literals, constant map keys, member access, nested update paths, pattern/destructuring field labels, and named arguments, including record/error construction. Reuse one label-token reader instead of scattering keyword exceptions through parsers.

This does not unreserve keywords as variable, parameter, declaration, or import names. `let type = ...` remains invalid. A keyword field in a binding pattern needs an explicit legal binding name, as in `{type: entry_kind}`; keyword shorthand/punning must not fabricate a keyword-named variable. Ordinary callable labels still have to match the resolved signature. Preserve reserved standard-module names and internal namespace integrity in every binding position.

Retain quoted keys for arbitrary field/key text. Unquoting an identifier-shaped keyword label changes no key bytes, duplicate detection, record/map classification, serialization, or type identity. A quoted key containing a dot remains one key, not a nested path. Do not add new quoting forms, automatic wire-name renaming, or implicit schema validation.

Known-field access retains its checked field type; unknown/dynamic receivers retain their normal validation requirements. This is a label-parsing simplification, not permission to remove `.require` boundaries.

Autofix quoted identifier-shaped labels to bare labels only in supported label positions and without changing literal interpretation. A literal-key `.get(...)` may become direct field access only when the field is statically guaranteed, the Result is merely propagated, and its consumers/conversions remain equivalent; otherwise retain it. Do not remove handled Results, custom error context, or runtime validation. No new stdlib function is needed.

Test keyword labels across the listed contexts, constructor/spread interaction, renamed destructuring, serialization parity, invalid keyword bindings, and negative namespace-shadowing cases.

## 5. Nested functional record updates

```xsh
# Before
let updated = {
  ...config,
  build: {
    ...config.build,
    jobs,
    flags: {...config.build.flags, debug: true},
  },
}

# After
let updated = {
  ...config,
  build.jobs: jobs,
  build.flags.debug: true,
}
```

Allow dotted field paths in record-update literals with exactly one leading `...base` and no additional spreads. This mode requires a statically known record shape. Every intermediate field must be a record and every target field must already exist. Both dotted and simple entries in this mode are replacements compatible with the existing field types; ordinary record literals/spreads retain their separate behavior.

This is a functional update, not mutation of base or any aliases. A literal containing a dotted replacement without the leading base is invalid. No computed map keys, list indices, dynamic string paths, implicit creation of intermediate records, or general lens API. Quoted keys containing dots retain their ordinary literal-key meaning.

Evaluate the base once, then replacement RHS expressions once in source order. RHS expressions resolve in the surrounding lexical environment; they do not refer to an implicitly evolving record or new field bindings. Retain the original base snapshot even if an RHS changes a surrounding mutable binding.

Reject duplicate and ancestor/descendant-overlapping targets such as both build and build.jobs. Disjoint siblings such as build.jobs and build.flags.debug are valid. Apply the replacements only after successful evaluation/validation, with no partially updated output escaping. Already executed RHS effects are not rolled back.

Reuse indexed record-path update operations and structural storage. Share untouched branches, combine updates to a common ancestor efficiently, and retain known record shapes. Do not introduce runtime path parsing or reapply constructor defaults.

Autofix nested spread-and-replace structures only when repeated base/field reads are stable and no evaluation, annotation conversion, comment, or intermediate observation disappears. Do not collapse repeated calls or mutable reads merely because their source text matches.

## 6. Statically checked record-to-named-argument spreading

```xsh
# Before
compile(root: options.root, jobs: options.jobs, verbose: options.verbose)?

# After
compile(...options)?
```

Add `...record_expression` entries to expression-call argument lists, including typed record constructors that accept named arguments. They expand to named arguments, not positional arguments. Preserve `@list` as positional list splicing; do not conflate the two forms.

Require a finite statically known record field set and a statically checked callable signature. Expand exactly the fields visible in the checked record type, not additional hidden runtime fields. Each visible field must name an accepted parameter. Reject Any, dynamic empty Record, Map, Optional, Result, and dynamic callable shapes unless the source has explicitly established the required concrete shape first.

Allow multiple disjoint spreads and mixtures with ordinary/punned arguments under ordinary call rules. Duplicate argument names are errors regardless of whether they come from a spread, explicit argument, or occupied positional parameter. Do not introduce last-wins call overrides. Construct an updated record explicitly when overriding configuration.

Every visible field is supplied, even when its value is null. Null does not mean omit this argument or choose its default. Existing default, overload, rest-parameter, type, effect, and error rules apply after static expansion. No new keyword-rest or runtime dictionary-dispatch protocol.

Preserve ordinary callee/receiver evaluation, then argument entries in source order. Evaluate each spread expression once at its position; project its statically selected fields before later argument expressions. Use statically resolved argument slots rather than building a temporary keyword dictionary. Preserve diagnostics back to the spread and offending field.

Autofix repeated field-forwarding arguments only when they cover exactly the intended visible fields of a stable record and preserve evaluation/conversions. Do not forward additional configuration fields, collapse repeated effectful receiver calls, or suppress an unknown-parameter error.

## 7. Explicit argument placement in value pipelines

```xsh
# Before — template and prefix are stable bindings
let data = load_data()?
let rendered = render(template, data, strict: true)?
let output = decorate(prefix, rendered)

# After
let output = load_data()?
  |> render(template, _, strict: true)?
  |> decorate(prefix, _)
```

Permit one `_` placeholder as a whole positional argument or named-argument value in the immediate call of a value-pipeline stage. For example, `data |> render(template, data: _)` inserts data into that parameter.

When a placeholder exists, perform an ordinary call with input inserted at that explicit position; do not additionally apply implicit receiver/first-argument insertion. A bare function name with a placeholder resolves as an ordinary callable, not automatically as a method on input. Stages without a placeholder preserve their established behavior.

Restrict the placeholder to the immediate stage call's argument list. Reject multiple placeholders, holes embedded inside expressions, holes inside nested calls/spreads/blocks, and free holes outside this context. Parentheses around a whole hole may be normalized. This is not a lambda, currying, partial application, or pipe-wide implicit variable. Existing discard/pattern/block-parameter uses of `_` remain unchanged.

Evaluate pipeline input first and retain its value once. Then evaluate the stage's callee/receiver and remaining arguments in ordinary source order; the hole reads the retained input without reevaluating it. In an optional call, input is still evaluated before the stage; ordinary optional-call laziness controls the stage's other arguments. Explicit `?` retains its normal boundary and meaning.

Keep structured stream-stage names and item blocks separate. A hole in a value call does not request per-item mapping, collection, implicit unwrapping, or changed stage dispatch. Use ordinary map blocks for per-item work.

Lower to existing call instructions with hygienic temporary storage, retaining source maps. Autofix only safe linear temporary chains or nested calls where moving the input ahead of other arguments is proved equivalent. Do not reorder effectful/fallible calls or changing mutable reads. Keep ordinary calls when a pipeline adds no clarity.

## 8. List element and nested element assignment

```xsh
# Before — the index is known valid
values = [@values[..1], replacement, @values[2..]]

# After
values[1] = replacement

# Also
rows[index].count += 1
```

Extend existing lvalue paths to traverse List elements rooted in a permitted local var. Support ordinary assignment and compound operators accepted by the selected element type, including nested record/map/list paths where each individual step is otherwise supported.

A List write accepts exactly the indices valid for ordinary List indexing, preserving its existing negative-index policy. Reject out-of-bounds access: no clipping, implicit append, padding, or slice replacement. Immutable roots remain errors. Do not add Str/Bytes mutation or writes through arbitrary temporary receivers.

Preserve collection value semantics. Updating one binding must not modify prior bindings or aliases. Reuse copy-on-write/ownership-aware storage and path reconstruction; do not unconditionally copy entire lists when uniquely owned storage permits an update.

Use the same selector/RHS evaluation, old-value observation, and commit contract as established record/map lvalue assignments. Pin that contract in differential tests before extending it, including an RHS or selector that affects the same root. Evaluate each selector and RHS no more than once at its proper point. Preserve the established order of errors and side effects; do not introduce a second assignment policy or silently overwrite unrelated updates.

The selected replacement must retain contextual element/schema typing. A failed update must not expose a partially rebuilt ancestor chain; effects already executed outside the update are not rolled back. Preserve ordinary assignment-as-Unit behavior.

Autofix reconstruction patterns only where their exact bounds, length, annotations, and evaluation behavior are equivalent. Prefix/suffix slices may accept an index that an element write rejects, so arbitrary slice-splice reconstruction is not automatically safe to rewrite.

## 9. Ordinary named arguments for structured-stream options

```xsh
# Before
let output = items |> par-map --jobs=jobs { |item| transform(item) }
let ordered = rows |> sort-by --desc .size

# After
let output = items |> par-map(jobs:) { |item| transform(item) }
let ordered = rows |> sort-by(desc: true) .size
```

Replace the separate structured-stage `--option` / `--option=value` syntax with ordinary named arguments inside the stage's existing argument list. Reuse normal expression parsing, named-argument binding, punning, duplicate detection, and diagnostics. Keep inline projections and parameterized blocks as they are; do not add another stream-stage invocation form.

Inventory every currently supported stage option and declare its accepted name, type, default, and validation once alongside the stage contract. Convert kebab-case option names to snake_case labels (`--max-bytes` becomes `max_bytes:`); valueless Boolean flags become explicit `true`. Existing mutually exclusive flags, such as reduction modes, remain mutually exclusive Boolean parameters with the same required/default selection rules. Do not add a mode enum, rename stages, invent new parameters, or loosen which stages accept options.

Keep positional arguments positional where appropriate, and make their existing roles nameable through the same stage contract when necessary to combine them with options. Unknown/duplicate names, conflicting modes, and invalid types must produce source-located errors. Reuse statically checked record-to-named-argument spreading rather than implementing a second argument-spread protocol. A stage's parameters are not new callable stdlib functions.

Bind supplied arguments in source order and evaluate every reached argument once at that stage's established entry boundary, not when the whole pipeline is constructed and not for every item. Preserve source-creation/pull timing, preceding-stage effects, short-circuiting, materialization, worker policy, and cleanup. Pin the existing option/positional timing in tests. Where an old options-first arrangement requires it, name the positional arguments too so migration can retain their evaluation order; never silently reorder effectful expressions.

Lower the checked arguments into the existing fixed stage configuration and execution machinery. Remove the special option-expression parser and duplicate option binding/checking paths when superseded; efficient internal configuration structs are not removal targets. This is syntax consolidation, not a generic stream callback engine or runtime redesign.

The old flag spelling becomes a migration error in structured-stage position. Provide narrowly scoped parser/lint recovery and CST-preserving autofixes even after removal; recheck the result and do not suppress unrelated errors. Use explicit `jobs: expression` unless ordinary punning is actually equivalent. Where evaluation order or comments cannot be preserved, diagnose without an unsafe autofix and migrate maintained code manually at the correct evaluation boundary.

External command arguments, `run` options, module commands, and xshi's shell-subset flags are untouched. Tests must distinguish literal `--jobs` argv from structured-stage options and cover each registered option, defaults, mode conflicts, named/spread arguments, stage pull timing, formatting, and fix idempotence.

## 10. Composable pattern aliases and bound alternatives

```xsh
# Before
match event {
  Added(file) => queue(file)?
  Changed(file) => queue(file)?
  _ => {}
}

# After
match event {
  Added(file) | Changed(file) => queue(file)?
  _ => {}
}

# Retain the matched whole value while extracting fields
match event {
  Changed(file) as original => {
    record_event(original)?
    queue(file)?
  }
  _ => {}
}
```

Add `PATTERN as name` aliases and complete binding-safe `|` alternatives in the shared pattern machinery. An alias binds the complete value matched by that pattern node while inner bindings retain extracted values. It is not a cast, constructor, mutable reference, or new nominal type.

Alias binding is tighter than alternation: `P | Q as name` aliases Q only. Support grouped patterns so `(P | Q) as name` aliases the complete alternative. Grouping does not introduce tuple values. Require an ordinary non-discard alias name and reject duplicate bindings, including alias/inner-binding collisions.

Check every alternative in an isolated provisional scope. All alternatives must bind exactly the same names with the same resolved types, ignoring ordinary alias spelling differences in types. Do not union incompatible payload types or weaken them to Any. An alias outside an alternative gets the sound common subject type, not variant-only fields from whichever arm was analyzed last.

At runtime evaluate the subject once, try alternatives in order, and commit bindings only after an alternative completely matches. Roll back partial captures before trying the next. If the arm has a guard, evaluate it after the first successful pattern; guard failure continues to the next arm, not to another alternative in that same arm.

Apply aliases/alternatives consistently in match and other binding-pattern contexts. In non-binding `is` tests, allow only patterns without captures; require grouping for alternatives (`value is (P | Q)`) and reject aliases and binding-bearing alternatives. Preserve existing literal/constructor/type/facet distinctions, sound narrowing, nominal error identity, exhaustiveness/reachability checking, and source spans.

Use ordinary value ownership for captured subjects. Avoid speculative copying/materialization of payloads or list remainders from alternatives that fail. Autofix identical adjacent match bodies into one alternative only when bind sets/types, guards, effects, comments, and source attribution remain meaningful. Never combine arms merely because their rendered text resembles one another.

## Architecture and migration

Use existing arena/CST, checker, registry, lvalue, pattern, record-storage, and indexed frame machinery. Expected starting owners include `src/syntax/`, `src/sema/check/`, `src/runtime/eval/{lower.rs,indexed/,lowered_ops.rs,lowered_run/}`, `crates/xsh-registry/src/`, and `crates/xsht/src/{lint.rs,format.rs,edit.rs,cli/}`. Resolve current ownership rather than creating similarly named parallel modules.

Implement shared label/block-header parsing and argument binding before dependent map/record/stage syntax; implement shared lvalues and pattern bindings before their new consumers. Update verifier invariants and every applicable execution route. Do not add an AST interpreter fallback, runtime source rewriting, generic operator-overloading framework, scheduler, or unbounded global cache.

For each useful transformation, add or extend a stable lint using checked types/resolutions and CST-preserving edits. Preserve comments, Unicode spans, explicit error boundaries, annotation-driven conversions, precedence, evaluation count/order, and import-source deduplication. Purity alone is not permission to reorder or remove evaluations. Emit guidance without an autofix where equivalence cannot be proved.

Displaced block-header and stream-option spellings must remain recognizable for migration only: recovery must not permit execution, become a general legacy mode, or hide unrelated errors. Recheck proposed edits through the normal program pipeline. Make overlapping fixes deterministic and convergent, and assert a second pass produces no edits. Keep formatter, structural grep/refactor, checking, diagnostics, tracebacks, API queries, and xshi's XSH path consistent. Do not change the separate shell-subset language.

Migrate clear instances in maintained stdlib/core/dev code, native tests, examples, scoped showcase code, documentation snippets, and relevant embedded XSH fixtures. Do not force every legal old spelling into a new form, eliminate meaningful variable names, or pursue a line-count target. Keep distinct standard APIs and useful explicit error handlers.

Update canonical specification/typing/streams contracts, registry language-reference/API examples, and nearest architecture/testing documentation. Do not generate another authoritative language manual or rebuild generated documentation.

