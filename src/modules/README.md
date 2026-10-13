# Standard Modules

This directory contains host helpers for standard modules. Public API shape is
declared in `crates/xsh-registry/src/signature/*`; `src/modules/signature.rs`
adapts registry entries into checker/runtime types. Evaluator-dependent module
operations are dispatched from `src/runtime/eval/lowered_run.rs` and split by
domain under `src/runtime/eval/lowered_run/module_*.rs`.

## Routing

| Change | Start here |
|---|---|
| add or change a module signature | `crates/xsh-registry/src/signature/modules.rs` |
| change a native module helper | the matching implementation under `src/modules/` |
| add or change a value method | `crates/xsh-registry/src/signature/methods.rs`, then `src/runtime/eval/lowered_run.rs` and `src/runtime/eval/lowered_run/indexed_run/values.rs` |
| add a stream stage signature | `crates/xsh-registry/src/signature/streams.rs` |
| add a runtime operation ID | `crates/xsh-registry/src/runtime_op.rs` |
| add standard record schemas | `crates/xsh-registry/src/records.rs` |
| change net transport internals | `crates/xsh-net`, with XSH adapters in `src/modules/net.rs` and `src/runtime/eval/modules/net.rs` |

Keep reusable host logic here when it does not need evaluator state. Keep
source spans, mocks, trace-sensitive behavior, cwd/env state, and runtime values
in the main evaluator adapters.

Generated stdlib docs come from the language registry. See
`xsht api module:MODULE` before changing a public signature or runtime owner.
