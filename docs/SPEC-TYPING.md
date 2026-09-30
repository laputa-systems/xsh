# XSH Typechecking Specification

This document is the detailed contract for XSH typechecking. `docs/SPEC.md`
remains authoritative for the language as a whole; when this document and
`docs/SPEC.md` disagree, update both in the same change.

## Goals

The checker prevents mistakes at script boundaries without turning ordinary
script code into annotation-heavy application code. Inference is local and
predictable. Types should become explicit at module, function, schema checks, and
process boundaries, while local bindings normally inherit the type of their
initializer.

The checker must keep going after an error when it can do so without producing
misleading follow-up diagnostics. Recovery types are implementation details and
must not appear in user annotations, standard API reference output, or ordinary
diagnostic prose.

## Checking Modes

Boolean guards use ordinary Bool/Status condition checks. The full checker
publishes `CheckOutput::definitely_exiting_block_spans` only after checking
failure paths, lexical exit applicability, match coverage, and terminating calls.
The safe leading-if rewrite consumes those facts. Guard branches share existing
null, type, pattern, and field-presence refinements. Assignments invalidate
overlapping record paths; writes to proven disjoint siblings retain evidence.
Unknown procedure calls invalidate mutable facts independently of declared effects.
No Result error input or success payload is introduced by a Boolean guard.

Default checking is compatibility-oriented. It reports definite syntax, name,
effect, arity, and type errors, but permits public dynamic values at concrete
boundaries where existing scripts may already rely on runtime check.

Strict dynamic checking is selected only by `xsht check --strict`. It has the
same grammar and runtime semantics as default checking, but adds migration
warnings for unsafe `Any` flows. Strict warnings are rendered as warnings and
make `xsht check --strict` exit with status `2`.

`xsh` does not have a strict execution mode. Strictness is a tooling surface, not
a script compatibility switch.

`xsht check --annotate[=CLASS,...]` is a tooling mode over normal checking. It
may write inferred annotations only after source loading, parsing, module
loading, and checking have produced no diagnostics. The rewrite surface is
intentionally conservative. Bare `--annotate` uses the exact class list in
`check.annotate` from `xsht-config.ini` when present, or the built-in default classes
`params`, `returns`, and `exports`. `params` annotates defaulted proc/pure
parameters, `returns` annotates inferred private pure returns and defaulted exported proc returns, and `exports`
annotates exported simple `let`/`var` bindings. The opt-in `locals` class
annotates local simple bindings with non-trivial types (`List`, `Map`, `Result`,
optional, `Command`, `Pure`, `Proc`, or tag union). `--annotate=locals` is
shorthand for defaults plus `locals`; `--annotate=all` enables every class. It
must not annotate
destructuring bindings, dynamic `Any`, checker recovery types, internal-only
types, anonymous record shapes, empty `Record`, discard `_`, local scalar
bindings, local `Unit` bindings, or source files other than the requested
scripts.

During `xsht check`, `reveal_type(expr)` is a checker-only builtin. It accepts
exactly one positional argument, emits a note containing the inferred type of
that argument, and has type `Unit`. Named arguments, splices, and wrong arity
are ordinary checker errors. Outside reveal-enabled checking, `reveal_type`
reports `check.reveal-type`; it is not part of the runtime API.

## Type Kinds

`Any` is the public dynamic type. Values from untyped host data, JSON decoding,
dynamic record helpers, and first-class dynamic callable dispatch may have this
type. `Any` is assignable to and from every type in default mode, subject to
runtime check where a host operation requires a concrete value.

`Unknown` and `Invalid` are checker-internal recovery types. `Unknown` means the
checker could not determine a type after a prior error or unsupported dynamic
shape. `Invalid` means a source annotation or type definition was already
diagnosed as invalid. Both suppress cascaded diagnostics. Source code cannot
annotate a value as `Unknown`; use `Any` for dynamic values.

Concrete scalar types are `Null`, `Bool`, `Int`, `Duration`, `Str`, `Bytes`,
`Digest`, `Regex`, `Path`, `Status`, `Error`, `ProcessError`, `Command`, `Pure`,
`Proc`, and `Unit`.

Parameterized types are `List[T]`, `Map[K, V]` (`Map[V]` means `Map[Str, V]`), `Stream[T]`, `Result[T, E]`,
`Result[T]` as shorthand for `Result[T, Error]`, and `Optional[T]` written as
`T?` in type position. Record schemas and tag unions are user-defined named
types. `enum Name { Variant, Payload(T) }` declares a nominal tag type,
including single-variant types. `type Alias = Name` preserves that identity
without creating constructors.
A Str-backed enum such as `enum State: Str { Ready = "ready", Empty = "" }`
remains nominal: neither Str assignment nor a type pattern performs conversion.
Only explicit `.require(Schema)` maps exact declared strings into enum slots,
recursively and atomically. The backing does not change constructor names or
allow another enum with identical strings to satisfy the type.

User record schemas and aliases accept lexical type parameters. Applications
are resolved by declaration identity and fully resolved arguments, then cached
within the checked program. Their concrete field types participate in ordinary
record/container assignability, field access, destructuring, and schema checks.
An unsupplied parameter never becomes `Any`. Defaults are checked against rigid
parameter identities so they must be valid for every argument type. Imported
applications retain the declaring module's private dependencies.


A schema constructor without written arguments creates fresh type variables for
that occurrence. Supplied fields and an independently annotated expected
application constrain them to exactly agreeing concrete types; defaults do not
select arguments. Non-null optional values provide evidence, while null and
empty containers alone do not. An explicit annotation can select `Any` or
`Null`; unconstrained evidence never guesses them. Nested constructors share
constraints until their enclosing constructor completes, then all instances
must be concrete before checked facts are published.

Unused parameters remain unresolved without an expected application. For
`Marker[T] = {name: Str}`, a value annotated `Marker[Str]` supplies only its
structural fields to `Wrap(marker: value)`; it does not identify an unobservable
argument in `Wrap[T] = {marker: Marker[T]}`. An independently expected
`Wrap[Int]` supplies that argument and still accepts the width-compatible
marker. This retains structural record assignability. Unresolved constructors
report `check.constructor-inference` and request an annotation or field evidence.

## Assignability

The checker uses structural assignability for built-in container and record
types:

- Identical concrete types match.
- `Any`, `Unknown`, and `Invalid` match any expected type; strict mode may still
  warn for `Any`.
- `List`, `Map`, `Stream`, `Result`, and `Optional` match when their contained
  types match recursively.
- `Null` matches any `Optional[T]`.
- A `T` value matches `Optional[T]`.
- Record schemas are width-compatible: a record with at least the expected
  fields, and compatible types for those fields, matches the expected record.
- An empty `Record` is explicitly dynamic and matches any record schema.
- Tag union values match only the same tag-union type.

Container parameters are treated invariantly for concrete typechecking:
`List[Str]` is not `List[Any]` because mutation and later reads would otherwise
lose guarantees. `Any` remains gradual: `List[Any]` can flow where `List[Str]` is
expected in default mode, and strict mode warns because the element values have
not been validated.

Mixed inferred containers keep the most specific type that is justified by all
items. A concrete element is not weakened merely because another expression is
unknown after an error. Empty generic constructors such as `map.empty()` take
their concrete element type from the expected context. If an element is truly
dynamic, the inferred container becomes `List[Any]` or `Map[Any]`; strict mode
warns when that dynamic container is used as a concrete container.

Computed brace keys require exactly `Str`; `Any`, Optional, Result, Path,
Bytes, and numeric values do not supply an implicit key conversion. A computed
entry or expected `Map[T]` context classifies the literal as a Map. Each value
and Map-spread element uses the same justified homogeneous inference as List
items. Expected types reach nested literal values and named record conversions;
a dynamic bound record is not accepted as a Map spread.

For a list literal, ordinary elements contribute their value type while
`@expression` contributes the element type of its checked `List[T]`. The
expected `List[T]` context reaches both ordinary elements and splice operands,
including empty literals and named record annotation conversions. Inference
uses the same homogeneous merge as ordinary list literals; `@` adds no `Any`
widening or display conversion. A dynamic `Any` operand is not a known List and
must first be checked explicitly. `Result` and `Stream` wrappers require explicit
handling before they can be spliced. Invalid operand diagnostics cover the
original `@expression` span (`check.list-splice-type`).

Half-open slices preserve their checked receiver type: `List[T]` becomes
`List[T]`, `Str` becomes `Str`, and `Bytes` becomes `Bytes`. Each supplied bound
must be `Int`; omitted bounds introduce no new expression or dynamic conversion.
The checker retains ordinary gradual `Any` behavior, with runtime domain checks.

## Bindings And Annotations

For `const`, `let` and `var`, an explicit annotation supplies the expected type for the
initializer. The initializer must match that type. Without an annotation, the
binding receives the initializer type.

`const` additionally requires a concrete preparation-time data type. Its value
and dependency graph are checked before runtime; ordinary let/var bindings and
parameters cannot supply a constant. Context reaches empty containers and
constructor fields. `Any`, recovery types, callable values, and handles are not
constant data. Exported constants remain `ModuleExportType::Value` entries.

Destructuring requires a record-like value. If the record schema is known and
non-empty, destructuring an unknown field is a checker error. Empty `Record`,
`Any`, and recovery types remain dynamic.

Assignments to `var` are checked against the binding's declared or inferred
type. Assignments to `let` are errors. Compound assignments require operands
accepted by the operator and produce a value assignable to the existing binding
type.

## Duration Operators

Duration addition and subtraction require two Duration operands and return
Duration. Multiplication requires one Duration and one Int in either order.
Duration division accepts Int (Duration result) or Duration (Int interval count).
Ordering accepts two Durations and returns Bool. Mixed numeric dimensions,
Float scaling, and remainder are rejected; no contextual numeric coercion is
introduced. Compound assignments require the result to retain the binding type.

Full checking, compact body facts, and lowering infer the same result type.
Runtime and bounded constant preparation use `checked_duration_binary` for the
unsigned millisecond domain, signed scalar constraints, and checked interval
counts. Integer specialization must inspect operand types: an Int result from
Duration division does not make its operands Int.

## Records

Record literals infer a schema from their fields. When a record literal is
checked against a known schema, field values are checked against the expected
field types, extra literal fields are rejected, and missing required fields are
rejected unless a spread may provide them.

Record field access on a known field returns that field type. Field access on an
empty `Record` or `Any` is dynamic and returns `Any`. In strict mode, field
access on a known non-empty record schema reports `check.unknown-field` when the
field is not part of the schema unless a local flow-sensitive refinement has
established that the field exists.

`record_value.get(field)` retains `Result[FieldType]` when a literal or
prepared constant Str key selects a field visible in the checked record or
module contract. Dynamic keys and fields with no proven visible type retain
`Result[Any]`. Lookup remains fallible, and a present nullable value remains
`Ok(null)`. Use `value.require(Schema)?` to validate or convert genuinely
dynamic data at a typed boundary.

`record.require` and its parallel string-contract grammar are removed.
Use a named schema at `.require(Schema)`. Optional key presence, nullable field
values, callable signatures, and dynamic validation policies remain distinct
contracts; migration must not invent nulls, defaults, or callable promises.
`check.removed-record-require` offers an identity migration only when a plain
record and an existing named schema already prove the required scalar fields.
The ordinary checker rechecks the complete edited source.

## Results

`Result[T, E]` has an `Ok(T)` success value and an `Err(E)` error value.
`Result[T]` uses `Error` as the error type.

`retry [delays] on (PATTERN) { ... }` checks its non-binding pattern against the
attempt's inferred error type. Nominal variants from an unrelated family are
impossible; error identity is retained through selection. The pattern adds no
captures, effects, conversion, or dynamic widening.

`Err(error, cause: failure)` retains precisely the first operand's error type
`E`; the cause does not join or widen `E`. Both operands must be assignable to
`Error` when the named cause is supplied. The one-argument constructor preserves
its existing generic `E` inference. Extra positional operands, duplicate causes,
unknown labels, and non-Error causes are errors under ordinary static argument
binding. A cause contributes neither facets nor payload fields to outer patterns.

`xsht check` emits `check.error-cause` guidance for a directly bound Err handler
that translates only `failure.message` into a declared error's sole message
argument. It suggests deliberately retaining the original diagnostic cause and
has no automatic fix. Richer payload construction and shadowing scopes are
outside this narrow guidance; custom messages and error contracts remain choices
owned by the application.

Postfix `?` may be applied only to `Result` values. It produces the `Ok` type
and propagates the `Err` value from a `Result`-returning context. In effectful
procs, `?` also requires the `error` effect unless the context is unrestricted.

Tail values in functions and tasks may be implicitly wrapped in `Ok(...)` when
the declared return type is `Result[T]` and the tail expression has type `T`.
Ignoring a value-producing `Result` is a checker error. A statement-position
`Result[Unit]` auto-propagates.

`match` constructor patterns narrow `Result` arms. In an `Ok(value)` arm,
`value` has the success type. In an `Err(error)` arm, `error` has the error
type.

For `Result[T, E] ?? { |failure| ... }`, the immutable parameter has exactly `E`
and a reachable handler tail must have `T`. Explicit handler Results remain
Results; this expression adds no implicit Ok wrapper or propagation boundary.
The block always uses value-tail classification, including Bool and Unit success
types. A Result literal with an unknown success shape may use the checked handler
or enclosing expected type to establish that shape. Optional fallback retains its
existing payload-only form.

## Optional Values

`T?` accepts either `Null` or `T`. It is a type-level optional shape, not a
separate runtime value kind.

`?.` guards fields and methods; adjacent `?[...]` guards indexing and
half-open slicing over the ordinary supported domains. Resolve the outer
receiver type before checking the ordinary operation. `Optional[T]` lifts
that operation's result into Optional and skips all arguments/bounds on null;
`Result[T, E]` propagates one outer Result with the usual error effect and
error-type checks. Guard each nullable hop explicitly. New method/index
operations require enough checked receiver information to select their domain;
Bare Any retains its established dynamic field behavior. A known outer
`Result[Any]` may propagate once before an ordinary supported dynamic operation.

An Optional method returning Result has type `Optional[Result[T, E]]`.
Optional lifting flattens only redundant Optional layers; no mixed wrapper is
recursively unwrapped. `(text?.parse_int() ?? Ok(0))?` handles the Optional and
Result layers at separate boundaries. A guarded Bool-producing method returns
Optional[Bool], which is not a Bool assertion. `?.require(Type)` retains its
Result propagation and schema validation path; it does not validate an
Optional receiver.

`??` unwraps `Optional[T]` by returning the contained `T` when present or the
fallback when the value is `null`. The fallback must match `T`.

The checker performs local flow-sensitive narrowing for null tests on bindings
and statically known record-field paths. In
the true branch of `if value != null`, and the false branch of `if value ==
null`, a binding of type `T?` is narrowed to `T`. `!` reverses the refinement.
For `and`, true-branch refinements from both operands apply. For `or`,
false-branch refinements from both operands apply. These refinements are local
to the checked branch body. Use this form when the value is needed only when
present; use `??` when a fallback is appropriate:

```xsh
let maybe_name: Str? = "demo"
if maybe_name != null {
  print maybe_name
}
let name = maybe_name ?? "unknown"
```

## Schema Check Boundaries

`value.require(Schema)` returns `Result[Schema]`. It turns dynamic host data into
a concrete XSH schema type for static checking.

The checker trusts the return type of `.require` once the result is unwrapped by
`?`, `with`, `guard let`, or a `match Ok(...)` arm. Runtime schema checking
remains responsible for checking the actual value against the schema.
Named standard record schemas use the same structural runtime check as
user-defined record schemas.

`cli.parse`, `cli.applet`, and `cli.parse_full` retain the result shape of
established constant descriptors, including inline data, imported `const`
values, closed constant projections, and constant record composition. One
normalized descriptor plan supplies argument policy and checked field types.
Required and defaulted scalars are concrete, absent non-required scalars are
optional, flags are `Bool`, and repeated values are `List[T]`. Explicit
`required: false` also applies to positional forms. `parse_full.values` retains
this shape while its provenance and warnings keep their existing contracts.

Runtime bindings and descriptor-producing calls remain dynamic. Known invalid
descriptors are checking errors at their declaration source; the descriptor
normalizer remains responsible for validating dynamically supplied data. A
forced non-Bool flag retains a dynamic field type because the parser can return
a Bool for an unvalued spelling and its declared scalar for attached values or
defaults.

Strict mode warns when `Any` flows into a concrete assignment, argument, return,
index, field access, or container merge without such a schema check boundary.
Keeping data as `Any`, empty `Record`, or another explicitly dynamic type does
not warn.

## Flow-Sensitive Narrowing

Flow-sensitive narrowing is local and lexical. A refinement shadows the original
binding only inside the branch, loop body, guarded statement, match arm, `with`
body, or `guard let` continuation where the condition proved it.

For `return payload when condition`, `break payload when condition`, and
`yield payload when condition`, check the condition before the payload and apply
its true-branch refinements inside the payload. `unless` uses false-branch
refinements. The payload retains ordinary return/break/yield type and effect
requirements even when the condition is a constant that skips it at runtime.
These guarded statements can fall through, so they do not establish an
unconditional return for the enclosing body.

Deferred cleanup blocks retain refinements of immutable captures. A mutable
capture is checked against its original binding type, because its value is read
when cleanup runs and can change after registration. Narrow it again inside the
cleanup body when needed.

Supported refinements:

- `value.require(Schema)?`, `with name = value.require(...)`, and `guard let
  name = value.require(...)` bind the checked schema type.
- `match result { Ok(value) => ... }` binds `value` to the `Result` success type.
- `match result { Err(error) => ... }` binds `error` to the `Result` error type.
- Tag-union constructor patterns bind payload values to the variant payload
  types and check the matched tag-union type.
- `value != null`, `value == null`, and `!` around those tests narrow optional
  bindings as described above.
- `record_value.has("field")` refines a record binding inside the true
  branch so `record_value.field` is known to exist with type `Any` unless the
  field already had a more precise schema type.
- `match value { name is Type => ... }` tests dynamic values and binds `name`
  as `Type` inside that arm. `_ is Type` tests without introducing a binding.

Immutable Bool aliases share bounded facts identified by binding identity,
mutation version, and known record path. Alias chains do not expand expressions.
Assignments invalidate overlapping paths and prefixes; disjoint static sibling
writes preserve facts. Calls that may change captures invalidate mutable facts
without treating an effect clause as an alias guarantee. Immutable record copies
retain independent snapshot identities, including in deferred bodies.

Successful guards, ordinary statement assertions, and branches preserve proofs
on their continuations; joins intersect reaching facts. Recovery from a failed
assertion does not establish its success facts. Callable and deferred mutable
captures require checks at execution scope. The checker does not infer arbitrary
Boolean implications, dynamic indexing relationships, numeric ranges, resource
liveness, or filesystem existence from conditions.

## Match And Patterns

Pattern checking is type-directed:

- Literal patterns must match the matched value type.
- Record patterns require record-like values and check known fields.
- `Ok` and `Err` patterns require `Result` values.
- Tag constructor patterns require the corresponding tag-union type.
- Binding patterns bind the matched value type, except zero-field tag variants
  are treated as constructor patterns when the name is known.
- Type patterns have the form `name is Type` or `_ is Type`. They require a
  dynamic matched value (`Any`, empty `Record`, or a recovery type), test the
  runtime value against the type expression, and narrow the arm binding to that
  type. They are for intentionally dynamic data, not for rechecking ordinary
  concrete values.

For tag unions, a `match` without a wildcard or catch-all binding reports
`check.non-exhaustive-match` when any variant is uncovered. This diagnostic is a
warning so scripts can stage migrations, but the type information in covered
arms is still precise.

## Callable Values

Defaulted parameter types come from checked declaration expressions. An omitted
annotation is resolved before callers from constants, projections, primitive
operations and established callable signatures. Null and unconstrained empty
collections require annotations; body uses and supplied arguments never anchor
an omitted parameter. Default names resolve outside the callable's parameters.
Declaration cycles that cannot establish a material type require an explicit
annotation. Named schemas, UInt constraints, optional domains and contextual
conversions remain explicit unless complete rechecking proves equivalence.

Named pure functions and procs have statically checked parameters and return
types. Private pure returns may be inferred from their definitions, independently
of declaration and caller order. Return inference accepts concrete compatible
shapes, preserves explicit Result boundaries, and requires annotations for
recursive components, exported functions, and underdetermined shapes. Checked
return facts are shared with indexed preparation and annotation tooling.
First-class `Pure` and `Proc` values are dynamic callable handles used for
module contracts and runtime-loaded APIs. Their `.call(...)` method returns
`Any` or `Result[Any]` because the concrete signature is known only to the
runtime contract validator.

Proc calls are effectful. Pure functions may call only pure functions and pure
standard APIs. Restricted procs may call only APIs whose effects are covered by
their effective effect set. Ordinary private procs without a clause infer a
finite transitive summary from checked bodies; explicit clauses remain upper
bounds. Recursive summaries are a least fixed point independent of declaration
order. Opaque or unrestricted dependencies remain unknown and cannot satisfy a
restricted caller. Local error capture erases outward `error` only. Public,
module-contract, CLI/test entry, conventional `main`, and stream boundaries keep
an unrestricted missing-clause contract. Checked effect facts and inference
provenance are shared by compact signatures, tooling, and static alias checks.

## Diagnostics

Definite type errors are reported as checker errors with source spans. Strict
dynamic issues are warnings with code `check.strict-any` and fail only
`xsht check --strict`.

Diagnostics should name expected and actual types when that helps explain the
failure. Diagnostics must not expose recovery types as user-facing source types.
When recovery is necessary, later checks should prefer suppressing cascades over
guessing a misleading concrete type.

## Checked Statement and Value Positions

The checker records `StatementPosition` for each checked statement in
`CheckOutput::statement_positions`; compact body facts retain the same distinction
by `StmtId`. Initializers and call/return payloads consume values. Function tails
consume their declared non-Unit value, and callback/retry tails infer their result
before classifying booleans. Unit and Result[Unit] tails retain statement behavior.
A Bool in statement position asserts; a Bool in value position retains false.
`assert condition, message` always consumes Unit and requires concrete Bool and
Str, rejecting Any, Status, Optional, and Result wrappers. Both expressions are
checked with ordinary effects and propagation, including a message skipped at
runtime. The assertion establishes no continuation refinement. Local try/retry error
inference includes the core Error failure, as for a bare Bool statement.

Value branches may contain lexical statements followed by a compatible tail.
Every reachable value branch must agree; `if` requires `else`, and `match` must
cover its subject without relying on guards. Diverging branches contribute no
Unit value. Ordinary branch scopes apply the same null refinements in expression
and statement syntax. Result success wrapping remains at the function boundary.
`tests/xsh/value-blocks.xsh` covers branch values, Bool contexts, records, and
lexical control transfer.

### Local Result boundary inference

`try` checks its value block against the expected Result success type and
collects propagation errors at that boundary, separately from lexical returns.
An inferred Result tail remains nested data; only propagation contributes its
error family to the captured Result. A Unit success annotation consumes the
tail as a statement, preserving Bool assertions and Result[Unit] propagation.
Underconstrained error-only blocks require a success annotation.

## Native test declarations

`test NAME [effects]? { |ctx| ... }` checks its body as `Result[Unit]` through
the ordinary proc body checker. The optional block parameter is an immutable
`TestContext`; `_` discards it. The name participates in declaration collisions
but never enters the ordinary callable namespace. Explicit effect lists retain
the normal proc restrictions, and registration performs no evaluation.

## Ordered Map key domains

Map key types resolve through aliases to Str, Int, UInt, Bool, Bytes,
Path, or Duration. Computed entries infer one concrete key domain; mixed domains
and Any keys are rejected. Empty Map values take K and V from context. UInt retains its nonnegative
constraint in semantic types and aliases while sharing the Int runtime value
and ordered key representation. Compiled key boundaries enforce the constraint. Map
methods bind the registry's `ReceiverMapKey` and `ReceiverMapValue` parameters to
the receiver before overload selection. Entry iteration retains `{key: K,
value: V}` through both full and compact checking. The indexed semantic type pool
stores both children and validates them before execution.
## Lookup absence facts

`Str.find`, `Str.byte_at`, and `Bytes.byte_at` produce `Int?`; a zero byte or
offset is a successful value. Checked null guards refine these results to
`Int`. Nullable arithmetic remains invalid until absence is handled.
`List.get(index)` and `Map.get(key)` produce `Result[T]`, even when `T` is
optional: a present null is successful data, and `??` handles only an error.
The removed fallback overloads are recognized by migration tooling without
providing an executable compatibility signature.

### Inferred validation targets

`value.require()` consumes an independently checked expected schema; it does
not assert that the receiver already has that type. Explicit annotations and
selected parameter contracts supply the boundary. `RequirementTarget` retains
the concrete `Type` and `SchemaExpectation` application identities so full and
compact checking prepare the same validator. Each `?` adds one expected Result
layer; validation removes one success layer to select its target. Recovery
types and unresolved type arguments supply no target. Source fixes compare
both the concrete type and schema application identities before removing only
the explicit argument.
