# XSH Typechecking Specification

This document is the detailed contract for XSH typechecking. `docs/SPEC.md`
remains authoritative for the language as a whole; when this document and
`docs/SPEC.md` disagree, update both in the same change.

## Goals

The checker prevents mistakes at script boundaries without turning ordinary
script code into annotation-heavy application code. Inference establishes reusable declaration relationships from definitions,
independently of callers. Explicit annotations express deliberate domain, schema,
effect, or public promises; omitted types do not weaken dynamic or host boundaries.

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

Checking and execution preparation enforce one dynamic boundary contract.
Concrete values can be erased into `Any`, while a dynamic value cannot establish
a concrete type from an annotation, parameter, or return expectation alone.
Use explicit `.require(Type)` validation, a checked type pattern, or an applicable
module contract at the actual input boundary. `xsht check --strict` was removed;
remove the option because this policy applies by default.

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
types, anonymous record shapes, erased `Record`, discard `_`, local scalar
bindings, local `Unit` bindings, or source files other than the requested
scripts.

During `xsht check`, `reveal_type(expr)` is a checker-only builtin. It accepts
exactly one positional argument, emits a note containing the inferred type of
that argument, and has type `Unit`. Named arguments, splices, and wrong arity
are ordinary checker errors. Outside reveal-enabled checking, `reveal_type`
reports `check.reveal-type`; it is not part of the runtime API.
Named callables and transparent immutable aliases reveal their principal scheme
with type, row, and effect relationships and requirements. Other expressions
reveal their checked instantiated type. Rendering consumes the same frozen facts
as source tooling queries.

## Type Kinds

`Any` is the public dynamic type. Values from untyped host data, JSON decoding,
dynamic record helpers, and first-class dynamic callable dispatch may have this
type. Concrete values can be erased into `Any`; values typed `Any` remain
dynamic until an explicit validation or type-pattern boundary establishes a
concrete type. Dynamic operations retain their runtime checks.

Field access, indexing and slicing on an `Any` receiver retain dynamic results
without intrinsic effects; reached operand effects still apply. Calling a method
on `Any` has an unknown effect summary. That call cannot establish a pure or
finite effect contract, including when its result is discarded.

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
- Any concrete value matches expected `Any`. Actual `Any` cannot match a concrete
  expectation without explicit validation.
- `Unknown` and `Invalid` suppress cascades after an earlier error; they cannot
  certify an executable program.
- `List`, `Map`, `Stream`, `Result`, and `Optional` match when their contained
  types match recursively.
- `Null` matches any `Optional[T]`.
- A `T` value matches `Optional[T]`.
- Record schemas are width-compatible: a record with at least the expected
  fields, and compatible types for those fields, matches the expected record.
- Builtin `Record` erases field knowledge. It accepts known record values but
  cannot establish a known record schema. A literal `{}` has an exact empty shape
  and cannot certify required fields.
- Tag union values match only the same tag-union type.

`List`, `Map`, and `Stream` parameters are invariant: an existing `List[Str]` is not
`List[Any]`, and `List[Any]` is not `List[Str]`. The same concrete constraints
apply through nested containers and numeric domains; a `Stream[Int]` cannot
establish `Stream[UInt]` through an annotation. Annotations do not validate
elements. `Result` error-family subtyping and nullable value compatibility
retain their checked directional rules.
Equality may compare a concrete value with its nullable domain in either operand
order. Comparing values produces `Bool`; it does not validate a dynamic operand
or make a nullable operand non-null.
Contextual literals and constructors use independently supplied element types
while checking each value. Empty local inference variables must be solved before
crossing a boundary that requires a concrete type.

Mixed inferred containers retain the most specific type justified by all items.
Recovery after an earlier error does not weaken a concrete element. Genuinely
dynamic contributions produce dynamic elements, which remain dynamic at later
uses. Collection inference does not inspect runtime values.

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

A preparation-time constant string index retains a definition-owned choice
between Map indexing and structural record projection when the receiver is
generic. The record alternative relates the result to that exact field and
retains the remaining row; it does not turn the record into a homogeneous Map.
The Map alternative relates the result to the invariant value parameter.
Callable signatures and nested producer permissions follow the selected field
or Map value, independently of equal item types. Nonconstant record keys retain
the existing erased projection boundary.

Half-open slices preserve their checked receiver type: `List[T]` becomes
`List[T]`, `Str` becomes `Str`, and `Bytes` becomes `Bytes`. Each supplied bound
must be `Int`; omitted bounds introduce no new expression or dynamic conversion.
Unchecked `Any` bounds require explicit `.require(Int)` validation before they
can establish an integer bound. An `Any` receiver remains dynamic and retains
runtime domain checks.

## Bindings And Annotations

For `const`, `let` and `var`, an explicit annotation supplies the expected type for the
initializer. The initializer must match that type. Without an annotation, the
binding receives the initializer type.

`const` additionally requires a concrete preparation-time data type. Its value
and dependency graph are checked before runtime; ordinary let/var bindings and
parameters cannot supply a constant. Context reaches empty containers and
constructor fields. `Any`, recovery types, callable values, and handles are not
constant data. Exported constants remain `ModuleExportType::Value` entries.

Destructuring requires a record-like value. If the record schema is known,
destructuring an unknown field is a checker error, including on an exact empty
record. Erased `Record`, `Any`, and recovery types remain dynamic.

Assignments to `var` are checked against the binding's declared or inferred
type. Assignments to `let` are errors. Compound assignments require operands
accepted by the operator and produce a value assignable to the existing binding
type.

Compound assignment retains a sealed operator requirement when its operands
belong to a generic function. Each call instantiates that function's requirement;
the mutable slot remains monomorphic within the instance. List `+=` requires a
List operand with the same element type, including optional and nominal element
domains. It does not append a scalar or widen the existing collection type.
Compound assignment completes with Unit. A UInt replacement retains the slot's
UInt domain while signed operands and arithmetic intermediates still cross the
checked storage boundary before publication.

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
erased `Record` or `Any` is dynamic and returns `Any`. Field access on a known
record shape reports `check.unknown-field` when the
field is not part of the schema unless a local flow-sensitive refinement has
established that the field exists.

`record_value.get(field)` retains `Result[FieldType]` when a literal or
prepared constant Str key selects a field visible in the checked record or
module contract. Dynamic keys and fields with no proven visible type retain
`Result[Any]`. Lookup remains fallible, and a present nullable value remains
`Ok(null)`. Use `value.require(Schema)?` to validate or convert genuinely
dynamic data at a typed boundary.

An unresolved inferred field type does not impose a concrete validation target.
Once that field type is fixed, `Any` cannot enter it without validation. This
distinction applies through nested records and containers.

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

An inert `Err(error)` constructor has an expression-owned scheme for its absent
success payload. The known error type remains fixed; no Unit or Any success is
invented. Propagating that proven error completes no success path, so its absent
payload contributes nothing to later branch joins. A try body with no successful
type anchor reports `check.try-success-type`; a retry result may retain the
quantified unreachable payload without manufacturing a successful value.

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

An unconstrained operand owned by the definition acquires its Result shape at
the postfix operation. `Checker::propagation_result_parts` relates the original
operand to independent success and error variables at its lexical level; it
does not choose either payload from callers or supply a default error type.
A local capture with one failure contribution preserves that exact error
variable through its own Result and through forwarded instantiations.
With multiple contributions, `RequirementTemplate::ErrorJoin` retains their
independent input ports and the capture's output port. Unknown inputs remain
conditional relationships in the principal scheme. Once known, variants of one
nominal family join to that family, and distinct checked error families join to
`Error`. A written error bound validates the joined output; it does not ground
the independent inputs to the bound. Forwarding copies the entire relationship,
so caller order cannot change the declaration's error parameters.

Tail values in functions and tasks may be implicitly wrapped in `Ok(...)` when
the declared return type is `Result[T]` and the tail expression has type `T`.
Ignoring a value-producing `Result` is a checker error. A statement-position
`Result[Unit]` auto-propagates.

A checked `Bool` in statement use has result `Unit` and propagates
`AssertionError.Failed(message: Str)` through `Checker::check_propagation`.
Only statement consumers and `Unit` / `Result[Unit]` tails assert; inferred
value tails are classified after inference. `CheckOutput::assertion_spans`
records this decision for lowering. `Any` is never inspected dynamically to
classify a statement. Explicit discards and boolean value consumers remain
values. Declared error families are not widened, and restricted procs require
`error` for assertion propagation outside a retry attempt.

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

Unchecked dynamic flows into concrete assignments, arguments, returns, and
nested container contracts are ordinary errors. Keeping intentionally dynamic
data as `Any` or erased `Record` is permitted; dynamic operations retain their
runtime checks.

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
- `"field" in record_value` refines a record binding inside the true
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
  dynamic matched value (`Any`, erased `Record`, or a recovery type), test the
  runtime value against the type expression, and narrow the arm binding to that
  type. They are for intentionally dynamic data, not for rechecking ordinary
  concrete values.

Pattern bindings preserve producer paths from the original matched value.
`Ok` and `Err` project the success and data-error payloads respectively; record
fields and list items preserve their nested paths. Aliases retain the full
matched value, and alternatives join the possible producer profiles for the
same binding. Matching a value neither opens its producers nor removes their
pull or cleanup permissions.

For tag unions, a `match` without a wildcard or catch-all binding reports
`check.non-exhaustive-match` when any variant is uncovered. This diagnostic is a
warning so scripts can stage migrations, but the type information in covered
arms is still precise.

## Callable Values

Required, defaulted, and rest parameter annotations, returns, producer item
annotations, and finite callable effects may be inferred from the definition and
its declaration dependencies. Analyze each declaration independently of its callers;
instantiate each completed rank-1 scheme freshly at a call. Exported definitions
may publish complete inferred schemes. Written module-contract signatures retain
their promises. A callback parameter or collection element is monomorphic within
one instantiation; it does not acquire an independently quantified scheme.

Mutually dependent declarations keep monomorphic placeholders until every body
in the group has contributed its constraints. Capture and value restrictions
apply before the group generalizes. Shared variables have one binder owner;
each member exposes only its reachable binders, so membership in the same group
does not grant access to unrelated variables. Reordering declarations does not
change their principal relationships.

Defaults participate in declaration constraints, but never acquire access to other
parameters. Supplied arguments evaluate in written source order; omitted defaults
evaluate once in parameter order in the declaration environment. Producer defaults
remain delayed until first pull, and CLI defaults retain their preparation-only
restriction. Explicit annotations still supply contextual conversion, validation,
nominal, UInt, and optional-domain boundaries. An unresolved executable overload or
underconstrained boundary requires a local annotation rather than caller training,
dynamic fallback, or an invented success value.

A default-only header with a written return uses the same declaration constraints
as its fully annotated equivalent. Literal defaults can anchor a concrete type;
null and empty containers introduce Optional or container relationships that
still need independent constraints. Mutable collection bindings and their early
aliases share one variable: later item constraints apply to every earlier read.
Mutation does not create a fresh independently generalized collection.

Callable signatures remain checked facts through compatible aliases, conditionals,
fields, containers, parameters, mutable slots, and returns. Preserve callable kind,
parameter/default relationships, return shape, and latent effects. Referencing,
storing, or returning a callable executes neither its body nor its defaults.
Explicit `Pure`/`Proc` erasure and runtime-loaded dynamic handles retain their
existing dynamic `.call(...)` contract and runtime validation.

Static standard-API references also retain their canonical argument admission
rules and producer relationships. Each reference or permitted enclosing scheme
instantiation has one monomorphic callable signature. Passing that value as a
callback does not independently instantiate its contract at each invocation.
Declared `Any` parameters retain their explicit erasure, while native guards
check each original supplied value before erasure. A compatible conditional
retains every possible callable authority, in either branch order; an invocation
must satisfy the native guards of every possible alternative. The source binding
plan retains supplied and omitted slots. Native proc defaults run at call time
even when the result contains a Stream, whose pull and cleanup permissions remain
separate from invocation effects.

Stage callback protocols retain the original callable value and invoke it with
the checked positional item/result/effect relationship. A single selected
member keeps its exact callable kind. For `InvocationPlan::All`, an exact kind
checks the aggregate of the selected original branches: Pure requires every
branch to be Pure; Proc permits Pure and Proc branches and requires at least
one Proc branch; Stream requires every branch to be Stream. Stream mixed with
Pure or Proc has no such aggregate. Pure/Proc stage forms continue to exclude
Stream callbacks. The aggregate chooses a protocol; it does not replace any
branch's signature, labels, default/rest binding, timing, or effects.
`CallableDomain::Exact` owns this check, and `StageCallback::Protocol` retains
the operation/formal-slot projection until the invocation is selected.

Proc calls retain proc kind even when their inferred effect summary is empty.
Pure functions may call only pure functions and pure standard APIs. Restricted
callables may call only APIs whose effects are covered by their effective bound.
Ordinary pure/proc/stream definitions, exported or private, infer finite effective
and required summaries from bodies, defaults, and declaration dependencies when
clauses are omitted. Explicit clauses, including deliberately written public
promises and `[]`, remain upper bounds. Recursive summaries are a least fixed
point independent of declaration order; retained callable signatures carry latent
effect relationships through application.

Producer body work and omitted defaults remain latent until the established pull
boundary. Supplied arguments retain their eager effects. Script producer effect
clauses bound latent body/default/cleanup work. Creation, pull, and close summaries
remain distinct; pull includes cleanup performed on exhaustion or failure.
Producer facts preserve nested field, container, optional, Result, and callable
paths through bindings and returns. Generic forwarding retains parameter and
capture relationships instead of guessing effects from `Stream[T]`. An opaque or unrestricted
external dependency has unknown effects, never an empty summary, and cannot
satisfy a restricted caller. Local error capture erases outward `error` only.
Written module contracts and host entrypoint promises retain their boundaries;
an omitted outer entry summary does not erase requirements of work it executes.
Native-test bodies remain statement consumers. Checked effect facts and inference
provenance are shared by frontend consumers, tooling, and callable alias checks.

## Diagnostics

Type errors, including unchecked dynamic boundaries and unknown fields on known
shapes, are checker errors with source spans. Checking and execution preparation
reject the same invalid boundaries.

Diagnostics should name expected and actual types when that helps explain the
failure. Diagnostics must not expose recovery types as user-facing source types.
When recovery is necessary, later checks should prefer suppressing cascades over
guessing a misleading concrete type.

## Checked Statement and Value Positions

Statement consumption and Result elaboration are declaration decisions. Resolve
them after constraints establish enough information and before generalization;
record them in the solved frontend facts consumed by lowering and tooling.
Instantiation, candidate trials, discarded call results, optimizer liveness, and
runtime values cannot choose a different meaning.

| Context | Meaning |
|---|---|
| Omitted ordinary pure/proc tail established as Bool | Return Bool data, including false. |
| Omitted ordinary tail quantified as `T` | Return `T` with the same elaboration at every instantiation, including Bool, Unit, Optional, and Result. |
| Written Unit/Result[Unit] boundary or native test body | Consume statements; retain established Bool assertions and implicit propagation. |
| Non-tail Bool established by declaration constraints | Assert under the ordinary statement contract. |
| Non-tail operation with established implicit propagation | Preserve the operation's checked callable kind and Result shape. |
| Non-tail quantified value whose behavior still depends on `T` | Reject locally; request explicit discard or a sufficient contract. |
| `let _ = expression` | Discard the resulting value; retain explicit `?`, effects, cleanup, and control transfers inside the initializer. |
| Omitted ordinary body with normal payloads and outward `?` | Infer one success/error relationship and select its Result elaboration once. |
| Normal completion independently established as a Result | Preserve the Result interpretation without adding another wrapper. |
| Quantified payload later instantiated as Result | Preserve the selected payload wrapper and nesting; never flatten dynamically. |
| Explicit `Ok(result)` | Preserve requested nesting. |
| Ambiguous payload-versus-Result boundary | Request an annotation rather than search over control-flow meanings. |
| Empty/no-value ordinary proc | Retain Result[Unit] success unless explicitly contracted otherwise. |
| Error-only or underconstrained success boundary | Require context or an annotation; invent neither success data nor a public Never type. |
| Non-completing path | Contribute no manufactured Unit/value; retain its internal control-flow fact. |

An explicitly annotated proc retains its data return separately from its
out-of-band propagation channel. For example, a proc returning Int with `[error]`
may use `?`; a successful call produces Int and a failure exits the proc through
the established channel. Omitted boundaries infer the success type from normal
completions and the error type from propagated failures, using existing nominal
error joins. An explicit return is a value and cannot be reinterpreted merely to
make an inferred boundary fit.

Generic iteration over an outer Result map, string, or byte source needs an
established lexical completion boundary. A written return fixes that boundary
without fixing the receiver's item, key, or row types. For a written Result return,
the canonical candidate's Result error projection must be assignable to the
declared error type; plain iterable candidates contribute no failure type. The
relationship survives generalization and forwarding in
`OperationCall.declared_error_bound` and `CandidateTemplate.failure_projection`.
`Checker::check_source_iteration_operation` keeps the receiver shape probe separate
from this retained contextual relationship. An omitted ambiguous completion still
requires sufficient receiver or return annotation before generalization.

For loops, comprehension generators, and delegated yields reuse the original
checked iterable expression's type endpoint. A composite view such as
`Stream[T]` does not authorize importing a replacement shell around the same
binder. `SolvedTypes.statement_operations` and `comprehension_operations` retain
the original source identity, argument binding, and item producer flow; readonly
queries preserve their canonical candidate authority after the parsed AST is gone.

Inside local try/retry, the capture fixes its own Result shape. An unresolved
iterator receives a fresh definition-owned error port in
`OperationCall.declared_error_bound`; `Checker::end_error_boundary` relates each
original contribution to the capture's final error type. A single contribution
preserves that port's identity, while multiple nominal contributions use the
retained `ErrorJoin` relationship. The capture neither borrows the enclosing function's error
type nor unifies independent nominal failures. Captured ERROR stays local, and
producer permissions such as TIME remain in the callable's effects. A port with
no observed failure remains underconstrained and requires context or a local
annotation instead of a fabricated Error or Any substitution.

An ordinary return inside a retry attempt still leaves the enclosing function.
When every path returns, the retry initializer contributes no normal completion
to that function's written Result boundary. The absent payload's expression
scope survives a following `?` through `expression_value_scopes` and
`non_completing_expressions`, without making the projected value polymorphic or
manufacturing a success type. Return values and genuinely reachable attempt
tails still obey the declared type.

`assert condition, message` consumes Unit and requires concrete Bool and Str,
rejecting Any, Status, Optional, and Result wrappers. Both expressions are
checked with ordinary effects and propagation, including a message skipped at
runtime. The assertion establishes no continuation refinement. Local try/retry
error inference includes the nominal AssertionError failure, as for a bare Bool
statement. Native tests retain their Result[Unit] body contract.

Every reachable value branch must agree; value `if` requires `else`, and value
`match` must cover its subject without relying on guards. Diverging branches
contribute no Unit value. Returns, loop transfers, retry attempts, deferred cleanup,
and cancellation retain their lexical targets and timing. A try body captures
one layer; a Result-valued tail remains nested success data unless explicitly
propagated. Callback Bool remains a value where the callback consumes Bool, while
Unit-consuming callbacks retain their statement rules.

Producer item inference uses reachable yield and delegation sites in the producer's
definition. `stream` fixes callable kind; inferred empty effects cannot promote a
proc to pure. Creation remains lazy, inferred types never materialize a producer,
and cancellation runs delegated child cleanup before parent cleanup. Process
Status remains process data under its established statement and explicit-discard
rules.

### Executable semantic observations

`bench/typing/semantic-cases.json` records independent ordinary CLI sources,
immutable baseline observations, candidate observations, and which differences are
intentional acceptance or rejection. `tests/xsh/typing-inference-preserved.xsh`
preserves written annotations and established contexts;
`typing-inference-targets.xsh` specifies new accepted relationships;
`typing-inference-negative.xsh` requires semantic rejection after parsing rather
than counting the older parser's required-annotation error as evidence.

The baseline compiler leaves omitted public/main/stream effect clauses unrestricted,
defaults a nonempty omitted proc to Result[Unit], requires
required/rest parameter annotations, and limits omitted private pure returns.
These historical facts are not the declaration inference contract above. The
preserved cohort keeps purposeful explicit boundaries when reproducing those
observations. Original annotation-specific coverage remains alongside the new
contracts until its genuinely invalid replacement is established.

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
