# XSH-4: Affine runtime handles

Status: draft

## Summary

Reject statically visible reuse of a runtime handle after a consuming operation. Start with straight-line code in one scope.

## Motivation

`ProcessHandle`, `NetJob`, and `FsRoot` own runtime state. Operations such as `wait` and `close` consume that state, but the checker still permits a later use. The runtime then reports an already-consumed error.

## Design

Mark each checker-known handle operation as borrowing or consuming. After a consuming call, reject another read, method call, argument pass, or capture of the same binding in the current straight-line scope.

Parameters must declare whether a call borrows or consumes a handle. The first implementation does not infer ownership through aliases or merge ownership state across branches and loops.

## Alternatives considered

Keep runtime-only checks. This preserves the current type system but delays a visible error until execution.

Implement general linear types first. That scope includes values without runtime ownership and would delay the three known handle families.

## Open questions

- How does a procedure parameter declare a borrow or a consuming transfer?
- Which existing operations consume each handle family?
- How should the checker report ownership state after control-flow joins?

## Decision

No decision yet.
