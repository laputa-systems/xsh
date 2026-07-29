# Indexed Frontend Fixtures

`indexed-execution.xsh` is the frozen indexed-execution fixture. It prints:

```text
slice 13 120 true true
```

It covers literals, slots, parameters, capture, assignment, return, direct and
recursive calls, mutual recursion, control flow, propagation, a guarded match,
a record field read, a registered bytes operation, and an unused operation with
a stable error location.

`indexed-method-call.xsh` covers the dynamically typed `sources.len()` call as
an explicit indexed method operation. It guards the requirement that this form
remain executable rather than becoming a placeholder instruction or selecting a
different evaluator.
