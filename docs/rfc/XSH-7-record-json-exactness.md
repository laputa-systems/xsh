# XSH-7: Record and JSON exactness

Status: draft

## Summary

Define how dynamic record validation handles unknown fields without changing width-compatible static record typing.

## Motivation

One schema may accept future fields at one boundary and reject them at another. Static `exact` record syntax would give the type one policy for validation, decoding, and evolution.

## Design

Keep record types width-compatible. Add exactness to the dynamic validation operation, such as an option on `json.require` or `.require`.

The operation must choose one unknown-field policy: reject, preserve, or strip. Encoding and defaults must state whether they observe preserved unknown fields.

## Alternatives considered

Add `exact { ... }` record syntax. This makes exactness part of assignment and call compatibility, even when only one input boundary needs strict validation.

Make every JSON schema exact. This breaks forward-compatible readers and gives callers no passthrough mode.

## Open questions

- Does exact validation return the same static type or a distinct validated type?
- Can nested schemas choose different unknown-field policies?
- Does stripping unknown fields require an explicit output record type?
- How do defaults interact with preserved unknown fields during encoding?

## Decision

No decision yet.
