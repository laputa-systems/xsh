use std::time::Instant;

use crate::time::{FineDuration, fence};

/// A measurement timestamp backed by the operating-system monotonic clock.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Timestamp(Instant);

impl Timestamp {
    #[inline(always)]
    pub fn start() -> Self {
        fence::full_fence();
        let value = Self(Instant::now());
        fence::compiler_fence();
        value
    }

    #[inline(always)]
    pub fn end() -> Self {
        fence::compiler_fence();
        let value = Self(Instant::now());
        fence::full_fence();
        value
    }

    pub fn duration_since(self, earlier: Self) -> FineDuration {
        self.0.duration_since(earlier.0).into()
    }
}
