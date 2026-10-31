#![forbid(unsafe_code)]

pub mod auth;
pub mod channel;
pub mod client;
pub mod error;
pub mod exec;
pub mod host_key;
pub mod kex;
pub mod wire;

pub use client::{ClientConfig, Connection};
pub use error::{Certainty, Error};
pub use exec::{ExecOptions, ExecResult, ExecStatus};
pub use host_key::HostKeyPin;
pub use oxish_graviola::DEFAULT_PROVIDER;
pub use oxish_proto;
