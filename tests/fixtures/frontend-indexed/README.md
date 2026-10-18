# Indexed Frontend Fixtures

`tests/xsh/frontend-indexed.xsh` runs `indexed-execution.xsh` and
`indexed-method-call.xsh` through the standard execution path. The first
covers indexed control flow and calls; the second guards dynamic method calls.

`src/runtime/eval/indexed/full.rs` uses `builtin-templates.xsh` to check nested
collection domains, named and spread binding, fresh map constraints, and lazy
Optional method arguments after frontend state is dropped. Both indexed routes
must use the same prepared builtin calls.
