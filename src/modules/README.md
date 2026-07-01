# Standard Modules

This directory contains host helpers for standard modules. Public API shape is
declared in `crates/xsh-registry/src/signature/*`; `src/modules/signature.rs`
adapts that registry into checker/runtime types. Runtime dispatch lives in
`src/runtime/eval/modules.rs` and
`src/runtime/eval/methods.rs` when evaluator state is needed.

## Routing

| Change | Start here |
|---|---|
| add or change a module function | `crates/xsh-registry/src/signature/modules.rs`, then matching `src/modules/<name>.rs` |
| add or change a value method | `crates/xsh-registry/src/signature/methods.rs`, then `src/runtime/eval/methods.rs` |
| add a stream stage signature | `crates/xsh-registry/src/signature/streams.rs` |
| add a runtime operation ID | `crates/xsh-registry/src/runtime_op.rs` |
| add standard record schemas | `crates/xsh-registry/src/records.rs` |
| change net transport internals | `crates/xsh-net`, with XSH adapters in `src/modules/net.rs` and `src/runtime/eval/modules/net.rs` |

Keep reusable host logic here when it does not need evaluator state. Keep
source spans, mocks, trace-sensitive behavior, cwd/env state, and runtime values
in the main evaluator adapters.

Generated stdlib docs come from the language registry. See
`docs/GENERATED-DOCS.md` before editing `docs/STDLIB.md` directly.
