# Frontend Campaign Fixtures

`vertical-slice.xsh` is the frozen Phase 1 scorecard input. The arena/runtime
oracle prints:

```text
slice 13 120 true true
```

It covers literals, slots, parameters, capture, assignment, return, direct and
recursive calls, mutual recursion, control flow, propagation, a guarded match,
a record field read, a registered bytes operation, and an unused operation with
a stable error location.

`vertical-slice-unsupported.xsh` was the strict compact-lowering blocker through
Phase 5. Phase 6 admits its dynamically typed `sources.len()` call as an
explicit indexed method operation. The narrow Phase 1 prototype still rejects
the wider expression instead of producing a placeholder instruction.
