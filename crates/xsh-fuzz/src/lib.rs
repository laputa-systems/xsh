//! XSH fuzzing: well-typed program generation with a reference evaluator,
//! registry-signature probes, and mutation of ill-formed programs.
//!
//! The soundness property under test is "well-typed programs do not go
//! wrong": a program the checker accepts must prepare, run without a runtime
//! type error or internal error, and print what the reference evaluator
//! predicts.

pub mod ast;
pub mod driver;
pub mod eval;
pub mod generator;
pub mod harness;
pub mod methods;
pub mod mutate;
pub mod probes;
pub mod rng;
pub mod shrink;
