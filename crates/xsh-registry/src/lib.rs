//! Shared language-facing registry data for XSH.
//!
//! This crate is intentionally independent from the main `xsh` crate so build
//! scripts and runtime code can consume the same definitions without creating a
//! Cargo build cycle.

pub mod api_docs;
pub mod errors;
mod examples;
pub mod records;
pub mod reference;
pub mod runtime_op;
pub mod signature;
pub mod symbols;
pub mod types;

pub use runtime_op::RuntimeOp;

pub const CORE_BUILTIN_SYMBOLS: &[&str] = &[
    "<unknown>",
    "Unit",
    "Any",
    "Null",
    "Bool",
    "Int",
    "UInt",
    "Float",
    "Duration",
    "Str",
    "Bytes",
    "Digest",
    "Regex",
    "Path",
    "Map",
    "Module",
    "Record",
    "Status",
    "EnvPathList",
    "Error",
    "ProcessError",
    "Pure",
    "Proc",
    "Command",
    "ProcessHandle",
    "Result",
];

pub const FIXED_SEMANTIC_SYMBOLS: &[&str] = &[
    "ARGV", "Err", "Ok", "args", "false", "main", "module", "true",
];
