use std::{cmp::Ordering, sync::OnceLock};

use crate::{
    alloc::{AllocOp, ThreadAllocInfo},
    black_box,
    time::{FineDuration, Timestamp},
};

/// Returns the smallest non-zero duration that the operating-system timer can
/// measure. The result is cached.
pub(crate) fn timer_precision() -> FineDuration {
    static CACHED: OnceLock<FineDuration> = OnceLock::new();

    *CACHED.get_or_init(measure_precision)
}

fn measure_precision() -> FineDuration {
    // Start with the worst possible minimum.
    let mut min_sample = FineDuration::MAX;
    let mut seen_count = 0;

    // If timing in immediate succession fails to produce a non-zero sample,
    // an artificial delay is added by looping. `usize` is intentionally
    // used to make looping cheap.
    let mut delay_len: usize = 0;

    loop {
        for _ in 0..100 {
            let sample_start: Timestamp;
            let sample_end: Timestamp;

            if delay_len == 0 {
                // Immediate succession.
                sample_start = Timestamp::start();
                sample_end = Timestamp::end();
            } else {
                // Add delay.
                sample_start = Timestamp::start();
                for n in 0..delay_len {
                    crate::black_box(n);
                }
                sample_end = Timestamp::end();
            }

            let sample = sample_end.duration_since(sample_start);

            // Discard sample if irrelevant.
            if sample.is_zero() {
                continue;
            }

            match sample.cmp(&min_sample) {
                Ordering::Greater => {
                    // If we already delayed a lot, and not hit the seen
                    // count threshold, then use current minimum.
                    if delay_len > 100 {
                        return min_sample;
                    }
                }
                Ordering::Equal => {
                    seen_count += 1;

                    // If we've seen this min 100 times, we have high
                    // confidence this is the smallest duration.
                    if seen_count >= 100 {
                        return min_sample;
                    }
                }
                Ordering::Less => {
                    min_sample = sample;
                    seen_count = 0;
                }
            }
        }

        delay_len = delay_len.saturating_add(1);
    }
}

/// Returns the overheads added by the benchmarker.
///
/// `min_time` and `max_time` do not consider this as benchmarking time.
pub(crate) fn bench_overheads() -> &'static TimedOverhead {
    static CACHED: OnceLock<TimedOverhead> = OnceLock::new();

    CACHED.get_or_init(|| TimedOverhead {
        sample_loop: sample_loop_overhead(),
        tally_alloc: measure_tally_alloc_overhead(),
        tally_dealloc: measure_tally_dealloc_overhead(),
        tally_realloc: measure_tally_realloc_overhead(),
    })
}

/// Returns the per-iteration overhead of the benchmarking sample loop.
fn sample_loop_overhead() -> FineDuration {
    static CACHED: OnceLock<FineDuration> = OnceLock::new();

    *CACHED.get_or_init(measure_sample_loop_overhead)
}

/// Calculates the per-iteration overhead of the benchmarking sample loop.
fn measure_sample_loop_overhead() -> FineDuration {
    let sample_count: usize = 100;
    let sample_size: usize = 10_000;

    // The minimum non-zero sample.
    let mut min_sample = FineDuration::default();

    for _ in 0..sample_count {
        let start = Timestamp::start();

        for i in 0..sample_size {
            _ = crate::black_box(i);
        }

        let end = Timestamp::end();

        let mut sample = end.duration_since(start);
        sample.picos /= sample_size as u128;

        min_sample = min_sample.clamp_to_min(sample);
    }

    min_sample
}

fn measure_tally_alloc_overhead() -> FineDuration {
    let size = black_box(0);
    measure_alloc_info_overhead(|alloc_info| alloc_info.tally_alloc(size))
}

fn measure_tally_dealloc_overhead() -> FineDuration {
    let size = black_box(0);
    measure_alloc_info_overhead(|alloc_info| alloc_info.tally_dealloc(size))
}

fn measure_tally_realloc_overhead() -> FineDuration {
    let new_size = black_box(0);
    let old_size = black_box(0);
    measure_alloc_info_overhead(|alloc_info| alloc_info.tally_realloc(old_size, new_size))
}

// SAFETY: This function is not reentrant. Calling it within `operation`
// would cause aliasing of `ThreadAllocInfo::current`.
fn measure_alloc_info_overhead(operation: impl Fn(&mut ThreadAllocInfo)) -> FineDuration {
    // Initialize the current thread's alloc info.
    let alloc_info = ThreadAllocInfo::current();

    let sample_count = 100;
    let sample_size = 50_000;

    let result = measure_min_time(sample_count, sample_size, || {
        if let Some(mut alloc_info) = ThreadAllocInfo::try_current() {
            // SAFETY: We have exclusive access.
            operation(unsafe { alloc_info.as_mut() });
        }
    });

    // Clear alloc info.
    if let Some(mut alloc_info) = alloc_info {
        // SAFETY: We have exclusive access.
        let alloc_info = unsafe { alloc_info.as_mut() };

        alloc_info.clear();
    }

    result
}

/// Calculates the smallest non-zero time to perform an operation.
fn measure_min_time(sample_count: usize, sample_size: usize, operation: impl Fn()) -> FineDuration {
    let loop_overhead = sample_loop_overhead();
    let mut min_sample = FineDuration::default();

    for _ in 0..sample_count {
        let start = Timestamp::start();

        for _ in 0..sample_size {
            operation();
        }

        let end = Timestamp::end();

        let mut sample = end.duration_since(start);
        sample.picos /= sample_size as u128;

        // Remove benchmarking loop overhead.
        sample.picos = sample.picos.saturating_sub(loop_overhead.picos);

        min_sample = min_sample.clamp_to_min(sample);
    }

    min_sample
}

/// The measured overhead of various benchmarking operations.
pub(crate) struct TimedOverhead {
    pub sample_loop: FineDuration,
    pub tally_alloc: FineDuration,
    pub tally_dealloc: FineDuration,
    pub tally_realloc: FineDuration,
}

impl TimedOverhead {
    pub fn total_overhead(&self, sample_size: u32, alloc_info: &ThreadAllocInfo) -> FineDuration {
        let sample_loop_overhead = self.sample_loop.picos.saturating_mul(sample_size as u128);

        let tally_alloc_overhead = self
            .tally_alloc
            .picos
            .saturating_mul(alloc_info.tallies.get(AllocOp::Alloc).count as u128);

        let tally_dealloc_overhead = self
            .tally_dealloc
            .picos
            .saturating_mul(alloc_info.tallies.get(AllocOp::Dealloc).count as u128);

        let tally_realloc_overhead = self.tally_realloc.picos.saturating_mul(
            alloc_info.tallies.get(AllocOp::Grow).count as u128
                + alloc_info.tallies.get(AllocOp::Shrink).count as u128,
        );

        FineDuration {
            picos: sample_loop_overhead
                .saturating_add(tally_alloc_overhead)
                .saturating_add(tally_dealloc_overhead)
                .saturating_add(tally_realloc_overhead),
        }
    }
}
