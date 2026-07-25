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

`vertical-slice-unsupported.xsh` is valid XSH whose dynamically typed
`sources.len()` call is an intentional strict compact-lowering blocker. It must
never be converted into a valid placeholder instruction by the Phase 1 builder.
