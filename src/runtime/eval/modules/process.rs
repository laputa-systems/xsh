//! Process-module runtime support. The old-AST `process.*` call dispatch
//! (`eval_process_call` and friends) was removed when the recursive evaluator
//! was deleted; the lowered runtime now handles `process.*` operations directly
//! in `lowered_run.rs`.
