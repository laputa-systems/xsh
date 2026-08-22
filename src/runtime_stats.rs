//! Thread-attributed allocation evidence for one ordinary indexed script run.
//!
//! This module is intentionally separate from `frontend_stats`: construction
//! accounting measures the compiler pipeline, while this reports allocations
//! made by the execution controller and explicitly spawned `par-map` workers.

use crate::execution::script::{RunOptions, ScriptOutput};
use crate::mem_track::{self, AllocTraffic, WorkerAllocationScope, WorkerStageTraffic};
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
    pub attributions: Vec<WorkerAllocationAttribution>,
}

/// Aggregate worker allocation traffic for one execution segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerAllocationAttribution {
    pub scope: &'static str,
    pub alloc_count: usize,
    pub alloc_bytes: usize,
}

impl WorkerAllocationStats {
    fn from_stages(stages: &[WorkerStageTraffic]) -> Self {
        let mut total = Self {
            stage_count: stages.len(),
            attributions: WorkerAllocationScope::ALL
                .into_iter()
                .map(|scope| WorkerAllocationAttribution {
                    scope: scope.label(),
                    alloc_count: 0,
                    alloc_bytes: 0,
                })
                .collect(),
            ..Self::default()
        };
        for stage in stages {
            total.alloc_count = total.alloc_count.saturating_add(stage.traffic.alloc_count);
            total.alloc_bytes = total.alloc_bytes.saturating_add(stage.traffic.alloc_bytes);
            total.peak_thread_bytes = total
                .peak_thread_bytes
                .saturating_add(stage.traffic.peak_bytes);
            for (scope, attribution) in WorkerAllocationScope::ALL
                .into_iter()
                .zip(&mut total.attributions)
            {
                let scoped = stage.scopes[scope as usize];
                attribution.alloc_count =
                    attribution.alloc_count.saturating_add(scoped.alloc_count);
                attribution.alloc_bytes =
                    attribution.alloc_bytes.saturating_add(scoped.alloc_bytes);
            }
        }
        total
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
        "{{\"stage_count\":{},\"alloc_count\":{},\"alloc_bytes\":{},\"peak_thread_bytes\":{},\"attributions\":{}}}",
        workers.stage_count,
        workers.alloc_count,
        workers.alloc_bytes,
        workers.peak_thread_bytes,
        worker_attributions_json(&workers.attributions),
    )
}

fn worker_attributions_json(attributions: &[WorkerAllocationAttribution]) -> String {
    let entries = attributions
        .iter()
        .map(|attribution| {
            format!(
                "{{\"scope\":\"{}\",\"alloc_count\":{},\"alloc_bytes\":{}}}",
                attribution.scope, attribution.alloc_count, attribution.alloc_bytes,
            )
        })
        .collect::<Vec<_>>();
    format!("[{}]", entries.join(","))
}

#[cfg(test)]
mod tests {
    use super::{
        AllocTraffic, MeasuredScriptRun, RuntimeAllocationStats, ScriptOutput,
        WorkerAllocationScope, WorkerAllocationStats, WorkerStageTraffic,
    };
    use crate::mem_track::ScopedAllocTraffic;

    #[test]
    fn worker_stats_sum_thread_local_traffic() {
        let workers = WorkerAllocationStats::from_stages(&[
            WorkerStageTraffic {
                traffic: AllocTraffic {
                    alloc_count: 2,
                    alloc_bytes: 16,
                    peak_bytes: 8,
                    ..AllocTraffic::default()
                },
                scopes: [
                    ScopedAllocTraffic {
                        alloc_count: 1,
                        alloc_bytes: 8,
                    },
                    ScopedAllocTraffic::default(),
                    ScopedAllocTraffic::default(),
                    ScopedAllocTraffic::default(),
                ],
            },
            WorkerStageTraffic {
                traffic: AllocTraffic {
                    alloc_count: 3,
                    alloc_bytes: 32,
                    peak_bytes: 24,
                    ..AllocTraffic::default()
                },
                scopes: [
                    ScopedAllocTraffic::default(),
                    ScopedAllocTraffic::default(),
                    ScopedAllocTraffic {
                        alloc_count: 3,
                        alloc_bytes: 32,
                    },
                    ScopedAllocTraffic::default(),
                ],
            },
        ]);

        assert_eq!(workers.stage_count, 2);
        assert_eq!(workers.alloc_count, 5);
        assert_eq!(workers.alloc_bytes, 48);
        assert_eq!(workers.peak_thread_bytes, 32);
        assert_eq!(workers.attributions.len(), WorkerAllocationScope::ALL.len());
        assert_eq!(workers.attributions[0].scope, "worker_setup");
        assert_eq!(workers.attributions[0].alloc_bytes, 8);
        assert_eq!(workers.attributions[1].scope, "par_map_results");
        assert_eq!(workers.attributions[2].scope, "par_map_item");
        assert_eq!(workers.attributions[2].alloc_bytes, 32);
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
            "{\"tracking_active\":true,\"status\":0,\"stdout_bytes\":3,\"stderr_bytes\":0,\"construction\":{\"live_bytes\":0,\"peak_bytes\":0,\"alloc_count\":0,\"alloc_bytes\":0},\"controller\":{\"live_bytes\":0,\"peak_bytes\":0,\"alloc_count\":0,\"alloc_bytes\":0},\"workers\":{\"stage_count\":2,\"alloc_count\":0,\"alloc_bytes\":0,\"peak_thread_bytes\":0,\"attributions\":[]}}\n"
        );
    }
}
