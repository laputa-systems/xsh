//! Filesystem-module runtime support. The old-AST `fs.*` / `path.*` / `env.*`
//! call dispatch was removed when the recursive evaluator was deleted; the
//! lowered runtime in `lowered_run.rs` handles those operations directly
//! (including its own fs-root bookkeeping helpers).
