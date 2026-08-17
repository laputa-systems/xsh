//! Thread-attributed allocation evidence for one ordinary indexed script run.
//!
//! This module is intentionally separate from `frontend_stats`: construction
//! accounting measures the compiler pipeline, while this reports allocations
//! made by the execution controller and explicitly spawned `par-map` workers.

use crate::execution::script::{RunOptions, ScriptOutput};
use crate::mem_track::{self, AllocTraffic};
use crate::runner::{RuntimeAllocationPhases, run_script_with_allocation_stats};

/// Aggregate allocation traffic from explicitly instrumented runtime workers.
///
/// `peak_thread_bytes` is the sum of each worker's thread-local peak. It is an
/// allocation-pressure signal, not a process RSS measurement or a precise
/// concurrent-live total: values may cross thread boundaries before dropping.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkerAllocationStats {
    pub stage_count: usize,
    pub alloc_count: usize,
    pub alloc_bytes: usize,
    pub peak_thread_bytes: usize,
}

impl WorkerAllocationStats {
    fn from_stages(stages: &[AllocTraffic]) -> Self {
        stages.iter().fold(
            Self {
                stage_count: stages.len(),
                ..Self::default()
            },
            |mut total, stage| {
                total.alloc_count = total.alloc_count.saturating_add(stage.alloc_count);
                total.alloc_bytes = total.alloc_bytes.saturating_add(stage.alloc_bytes);
                total.peak_thread_bytes = total.peak_thread_bytes.saturating_add(stage.peak_bytes);
                total
            },
        )
    }
}

/// Thread-attributed allocation phases for one script run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeAllocationStats {
    pub tracking_active: bool,
    pub construction: AllocTraffic,
    pub controller: AllocTraffic,
    pub workers: WorkerAllocationStats,
}

/// The script result plus allocation evidence collected around it.
#[derive(Clone, Debug)]
pub struct MeasuredScriptRun {
    pub output: ScriptOutput,
    pub allocation: RuntimeAllocationStats,
}

/// Run one ordinary script while splitting construction, controller, and
/// indexed-worker allocation traffic.
pub fn run_script(options: RunOptions) -> MeasuredScriptRun {
    let (output, phases) = run_script_with_allocation_stats(options);
    MeasuredScriptRun {
        output,
        allocation: RuntimeAllocationStats::from_phases(phases),
    }
}

impl RuntimeAllocationStats {
    fn from_phases(phases: RuntimeAllocationPhases) -> Self {
        Self {
            tracking_active: mem_track::tracking_installed(),
            construction: phases.construction,
            controller: phases.controller,
            workers: WorkerAllocationStats::from_stages(&phases.worker_stages),
        }
    }
}

impl MeasuredScriptRun {
    /// Serialize the measurement without mixing it into the script's stdout.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"tracking_active\":{},\"status\":{},\"stdout_bytes\":{},\"stderr_bytes\":{},\"construction\":{},\"controller\":{},\"workers\":{}}}\n",
            self.allocation.tracking_active,
            self.output.status,
            self.output.stdout.len(),
            self.output.stderr.len(),
            traffic_json(&self.allocation.construction),
            traffic_json(&self.allocation.controller),
            worker_json(&self.allocation.workers),
        )
    }
}

fn traffic_json(traffic: &AllocTraffic) -> String {
    format!(
        "{{\"live_bytes\":{},\"peak_bytes\":{},\"alloc_count\":{},\"alloc_bytes\":{}}}",
        traffic.live_bytes, traffic.peak_bytes, traffic.alloc_count, traffic.alloc_bytes,
    )
}

fn worker_json(workers: &WorkerAllocationStats) -> String {
    format!(
        "{{\"stage_count\":{},\"alloc_count\":{},\"alloc_bytes\":{},\"peak_thread_bytes\":{}}}",
        workers.stage_count,
        workers.alloc_count,
        workers.alloc_bytes,
        workers.peak_thread_bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::{AllocTraffic, MeasuredScriptRun, RuntimeAllocationStats, ScriptOutput, WorkerAllocationStats};

    #[test]
    fn worker_stats_sum_thread_local_traffic() {
        let workers = WorkerAllocationStats::from_stages(&[
            AllocTraffic {
                alloc_count: 2,
                alloc_bytes: 16,
                peak_bytes: 8,
                ..AllocTraffic::default()
            },
            AllocTraffic {
                alloc_count: 3,
                alloc_bytes: 32,
                peak_bytes: 24,
                ..AllocTraffic::default()
            },
        ]);

        assert_eq!(workers.stage_count, 2);
        assert_eq!(workers.alloc_count, 5);
        assert_eq!(workers.alloc_bytes, 48);
        assert_eq!(workers.peak_thread_bytes, 32);
    }

    #[test]
    fn json_keeps_script_output_separate_from_measurement() {
        let measured = MeasuredScriptRun {
            output: ScriptOutput {
                status: 0,
                stdout: b"ok\n".to_vec(),
                stderr: Vec::new(),
            },
            allocation: RuntimeAllocationStats {
                tracking_active: true,
                workers: WorkerAllocationStats {
                    stage_count: 2,
                    ..WorkerAllocationStats::default()
                },
                ..RuntimeAllocationStats::default()
            },
        };

        assert_eq!(
            measured.to_json(),
            "{\"tracking_active\":true,\"status\":0,\"stdout_bytes\":3,\"stderr_bytes\":0,\"construction\":{\"live_bytes\":0,\"peak_bytes\":0,\"alloc_count\":0,\"alloc_bytes\":0},\"controller\":{\"live_bytes\":0,\"peak_bytes\":0,\"alloc_count\":0,\"alloc_bytes\":0},\"workers\":{\"stage_count\":2,\"alloc_count\":0,\"alloc_bytes\":0,\"peak_thread_bytes\":0}}\n"
        );
    }
}
