# XSH-180: Define the ordering used by min and max

Status: draft

## Summary

Correct extrema for admitted finite Float and Duration values. Define the
remaining ordering choices explicitly so that `min`, `max`, their inferred
forms, and keyed reductions agree with the contract they expose.

The finite numeric defect can be fixed independently of this RFC's open
questions. This draft does not authorize restricting the currently admitted
List, Result, or opaque payload types, or widening the sort-key domain.

## Motivation

At reviewed revision `956a67a548ee09e6cbd2a448be2c3fa3f6714848`, the
[MinMaxItem predicate](../../src/sema/sealed_operation.rs) accepts resolved
types other than top-level `Any`. The
[SPEC](../SPEC.md#133-stages) deliberately preserves Float, List, nested Any,
and Result inputs and distinguishes this predicate from the sort-key domain.
The [API documentation source](../../crates/xsh-registry/src/reference.rs)
describes numeric extrema.

The [terminal implementation](../../src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs)
uses [compare_lowered_sort_keys](../../src/runtime/eval/lowered_ops.rs).
That comparator handles Int, Bool, strings, paths, and records; its fallback
compares type names. Two Float values or two Duration values therefore compare
equal regardless of their magnitudes.

This script exits successfully with `9 1`:

```xsh
let numbers = [9.0, 1.0]
let lo = numbers |> min()?
let hi = numbers |> max()?
print f"{lo} {hi}"
```

Its numeric extrema should be `1 9`. The equivalent Duration script with
`[9s, 1s]` prints `9s 1s`; inferred generic Float wrappers reproduce the same
error. These were executed with the pinned nightly and verification profile
on native Linux GNU. The
[generic aggregate fixture](../../tests/fixtures/runtime/generic-pipeline-aggregates.xsh)
currently compares its Float results to a second route through the same
comparator, so all three tests in its native test file pass despite this bug.

The broader admitted domain raises a different question: what is the ordering
of a List, a Result, or a record containing opaque values? The current SPEC
states their acceptance but does not supply that order. The numeric witness
does not establish which structural or opaque ordering the language should use.

## Design

### Repair finite numeric comparison

Compare finite Float values numerically and Duration values by duration.
Use that decision in direct and inferred extrema, and inspect keyed min/max
reductions that share the comparator. Keep the current empty-input error,
source-error propagation, cleanup, result type, and tie behavior while making
this bounded correction.

Keep `MinMaxItem` separate from `SortKey`: adding numeric extrema support must
not silently admit Float or Duration sort keys. A common value-comparison
helper may be reused where the checked domains allow it, but a type-name
fallback cannot implement numeric value ordering.

### Resolve the complete ordering contract

After the owner decides the questions below, give the checked predicate and
runtime comparison one explicit definition of the supported domain. Update
the SPEC template and API documentation together. If acceptance changes,
state that compatibility change and provide a diagnostic that names the
unsupported item type. Until that decision, preserve existing nonnumeric
acceptance rather than treating this draft as permission to reject it.

### Acceptance evidence

- Assert literal expected extrema for finite Float and Duration inputs in
  both source orders, through direct and inferred calls and list/live sources.
- Retain Int extrema and empty-input controls. Exercise source failures and
  cleanup without pulling additional items after an error.
- Cover keyed min/max with independent expected values; comparing two callers
  of one comparator does not establish correct ordering.
- When the broader policy is accepted, cover its NaN, infinity, signed-zero,
  tie, nested-value, and unsupported-type cases directly.

## Alternatives considered

**Use the existing sort-key domain for all extrema.** This would reject
currently admitted numeric and structural inputs. It also conflicts with the
SPEC's deliberate separation of these domains, so it is not the numeric fix.

**Keep type-name comparison for every unmatched value.** This preserves the
observed implementation but cannot produce numeric extrema. It also leaves
within-type structural ordering unexplained.

**Define a total structural order immediately.** A recursive order could make
the broad accepted domain useful, but it requires decisions about field order,
variant tags, opaque payloads, and NaN. Those decisions should not delay the
finite Float/Duration correction or be inferred from accidental behavior.

## Open questions

1. **Which nonnumeric values have an extrema order?** Options are a documented
   structural order for a defined subset, a caller-provided projection or
   comparator for other types, or retaining the current acceptance with an
   explicit compatibility policy. Recommend a defined structural subset and
   an explicit projection/comparison route for opaque data. Any rejection of
   currently admitted values needs an approved compatibility decision.
2. **How should NaN behave?** Options include a defined total Float order,
   propagation, ignoring NaN when another value exists, or a typed error.
   Recommend a typed error unless XSH adopts a shared total Float ordering;
   whichever policy is chosen must specify all-NaN input and infinities.
3. **Which value wins an equal comparison?** The terminal currently delegates
   ties to Rust min/max selection, while keyed reduction keeps the existing
   accumulator on equality. Options are preserving those route-specific
   choices or consistently retaining the first value. Recommend first-value
   retention for predictable source-order behavior, with signed zero covered
   explicitly. Preserve current ties in the independent numeric correction.

## Decision

Pending owner review. Finite Float/Duration comparison is the actionable bug
in XSH-180; broader domain, NaN, and tie changes remain proposals.
