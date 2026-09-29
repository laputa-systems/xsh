#![allow(dead_code, unused_imports)]

mod alias;
mod app;
#[doc(hidden)]
#[cfg(feature = "benchmark")]
pub mod bench;
mod builtin;
mod complete;
mod config;
mod denv;
mod history;
mod input;
mod line;
mod listing;
mod path;
#[cfg(test)]
mod ported_tests;
mod prompt;
mod render;
mod repl;
mod session;
mod shell;
mod signal;
mod sys;
mod term;
mod z;

pub use app::{
    CliOutput, OneCommandOptions, RunOptions, check_source, run, run_one_command,
    run_one_command_with_options, run_with_options,
};
