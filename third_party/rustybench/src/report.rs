//! Machine-readable benchmark results.

use deser::{Deserialize, Serialize};

use crate::{alloc::AllocOp, stats::Stats};

/// The schema emitted by `--format json`.
pub(crate) const SCHEMA: u32 = 1;

/// One measured benchmark entry.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct BenchmarkRecord {
    pub name: String,
    pub median_ns: u64,
    pub alloc_count: u64,
    pub alloc_bytes: u64,
    pub max_alloc_count: u64,
    pub max_alloc_bytes: u64,
    pub sample_count: u32,
    pub iter_count: u64,
}

/// The complete output of one benchmark executable invocation.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct BenchmarkReport {
    pub schema: u32,
    pub benchmarks: Vec<BenchmarkRecord>,
}

impl BenchmarkRecord {
    pub(crate) fn from_stats(name: &str, stats: &Stats) -> Self {
        let allocation = stats.alloc_tallies.get(AllocOp::Alloc);
        let max_alloc = &stats.max_alloc;

        Self {
            name: name.to_owned(),
            median_ns: nanos(stats.time.median.picos),
            alloc_count: count(allocation.count.median),
            alloc_bytes: count(allocation.size.median),
            max_alloc_count: count(max_alloc.count.median),
            max_alloc_bytes: count(max_alloc.size.median),
            sample_count: stats.sample_count,
            iter_count: stats.iter_count,
        }
    }
}

impl BenchmarkReport {
    pub(crate) fn new(benchmarks: Vec<BenchmarkRecord>) -> Self {
        Self {
            schema: SCHEMA,
            benchmarks,
        }
    }
}

fn nanos(picos: u128) -> u64 {
    picos
        .saturating_add(500)
        .checked_div(1_000)
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or(u64::MAX)
}

fn count(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value.round().min(u64::MAX as f64) as u64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_preserves_report_contract() {
        let report = BenchmarkReport::new(vec![BenchmarkRecord {
            name: "suite::work::input".to_owned(),
            median_ns: 42,
            alloc_count: 3,
            alloc_bytes: 96,
            max_alloc_count: 2,
            max_alloc_bytes: 64,
            sample_count: 10,
            iter_count: 1_000,
        }]);

        let json = deser_json::to_string(&report).expect("benchmark report contains JSON values");
        let decoded: BenchmarkReport = deser_json::from_str(&json).unwrap();
        assert_eq!(decoded, report);
    }
}
