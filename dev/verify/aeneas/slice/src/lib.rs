#![allow(dead_code)]

pub mod syntax {
    pub mod node {
        include!("binary_op.rs");
    }
}

#[path = "../../../../../src/arith.rs"]
pub mod arith;

#[path = "../../../../../src/duration.rs"]
pub mod duration;

#[cfg(test)]
mod witnesses;
