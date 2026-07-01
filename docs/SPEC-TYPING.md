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
parameters, `returns` annotates defaulted exported proc returns, and `exports`
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

Parameterized types are `List[T]`, `Map[T]`, `Stream[T]`, `Result[T, E]`,
`Result[T]` as shorthand for `Result[T, Error]`, and `Optional[T]` written as
`T?` in type position. Record schemas and tag unions are user-defined named
types.

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

## Bindings And Annotations

For `let` and `var`, an explicit annotation supplies the expected type for the
initializer. The initializer must match that type. Without an annotation, the
binding receives the initializer type.

Destructuring requires a record-like value. If the record schema is known and
non-empty, destructuring an unknown field is a checker error. Empty `Record`,
`Any`, and recovery types remain dynamic.

Assignments to `var` are checked against the binding's declared or inferred
type. Assignments to `let` are errors. Compound assignments require operands
accepted by the operator and produce a value assignable to the existing binding
type.

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

`record_value.get(field)` returns `Result[Any]` because a string field name is
dynamic. Use `value.require(Schema)?` to convert dynamic data into a typed
schema.

`record.require` checks runtime contract records and returns `Record`; it
does not infer concrete XSH schemas. `"Any"` is the dynamic contract string.

In strict mode, literal contract records passed to `record.require` are checked
before runtime. Required and optional contract
fields must use literal type strings. Malformed type names, malformed
parameterized types, malformed `Result[...]`, and malformed `Proc(...) -> ...`
signatures produce `check.contract-type` warnings.

## Results

`Result[T, E]` has an `Ok(T)` success value and an `Err(E)` error value.
`Result[T]` uses `Error` as the error type.

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

## Optional Values

`T?` accepts either `Null` or `T`. It is a type-level optional shape, not a
separate runtime value kind.

`?.` accesses a field through an optional or result value. For `Optional[T]`,
the result type is optional. For `Result[T]`, the result type is the accessed
field type from `T`.

`??` unwraps `Optional[T]` by returning the contained `T` when present or the
fallback when the value is `null`. The fallback must match `T`.

The checker performs local flow-sensitive narrowing for simple null tests. In
the true branch of `if value != null`, and the false branch of `if value ==
null`, a binding of type `T?` is narrowed to `T`. `!` reverses the refinement.
For `and`, true-branch refinements from both operands apply. For `or`,
false-branch refinements from both operands apply. These refinements are local
to the checked branch body.

## Schema Check Boundaries

`value.require(Schema)` returns `Result[Schema]`. It turns dynamic host data into
a concrete XSH schema type for static checking.

The checker trusts the return type of `.require` once the result is unwrapped by
`?`, `with`, `guard let`, or a `match Ok(...)` arm. Runtime schema checking
remains responsible for checking the actual value against the schema.
Named standard record schemas use the same structural runtime check as
user-defined record schemas.

`cli.parse(argv, schema)` is also a schema boundary when `schema` is a literal
record. The checker infers the returned record shape from the descriptor
literal, including concrete fields for required, positional, and defaulted
scalars, optional fields for absent non-required scalars, `Bool` for flags, and
`List[T]` for repeated values.

Strict mode warns when `Any` flows into a concrete assignment, argument, return,
index, field access, or container merge without such a schema check boundary.
Keeping data as `Any`, empty `Record`, or another explicitly dynamic type does
not warn.

## Flow-Sensitive Narrowing

Flow-sensitive narrowing is local and lexical. A refinement shadows the original
binding only inside the branch, loop body, guarded statement, match arm, `with`
body, or `guard let` continuation where the condition proved it.

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

Refinements do not mutate the binding's type outside the proven scope. The
checker does not infer arbitrary boolean implications, field relationships,
numeric ranges, or path existence from conditions.

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

Named pure functions and procs have statically checked parameters and return
types. First-class `Pure` and `Proc` values are dynamic callable handles used for
module contracts and runtime-loaded APIs. Their `.call(...)` method returns
`Any` or `Result[Any]` because the concrete signature is known only to the
runtime contract validator.

Proc calls are effectful. Pure functions may call only pure functions and pure
standard APIs. Restricted procs may call only APIs whose effects are covered by
their declared effect set. Unrestricted procs retain compatibility behavior and
may call any proc or effectful API.

## Diagnostics

Definite type errors are reported as checker errors with source spans. Strict
dynamic issues are warnings with code `check.strict-any` and fail only
`xsht check --strict`.

Diagnostics should name expected and actual types when that helps explain the
failure. Diagnostics must not expose recovery types as user-facing source types.
When recovery is necessary, later checks should prefer suppressing cascades over
guessing a misleading concrete type.
