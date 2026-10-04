#[macro_use]
#[path = "../../../tests/release_binary.rs"]
mod release_binary;

#[path = "cli.rs"]
mod cli;
#[path = "grep.rs"]
mod grep;
#[path = "lint.rs"]
mod lint;
#[path = "lint_format_invariance.rs"]
mod lint_format_invariance;
#[path = "lint_performance.rs"]
mod lint_performance;
