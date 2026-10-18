//! Allocation traffic and observed wall-time boundaries for one indexed run.
//!
//! Wall time uses the monotonic clock and includes observer overhead. Allocation
//! traffic remains thread-attributed; neither signal measures process RSS.

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
    /// The caller thread's controller stage: what the run did outside the
    /// evaluation thread.
    pub controller: AllocTraffic,
    /// The evaluation thread: where the script's statements actually run, and
    /// therefore where a script's own allocation traffic lands.
    pub execution: AllocTraffic,
    pub workers: WorkerAllocationStats,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimePhaseWallTimes {
    pub preparation_inclusive: u128,
    pub project_parse_load_exclusive: Option<u128>,
    pub check_solve_fact_build_exclusive: Option<u128>,
    pub declaration_publication_exclusive: Option<u128>,
    pub lower_verify_install_exclusive: Option<u128>,
    pub execution_finish_exclusive: Option<u128>,
    pub diagnostic_finish_exclusive: Option<u128>,
}

impl RuntimePhaseWallTimes {
    fn preparation_other_exclusive(&self) -> u128 {
        let observed = [
            self.project_parse_load_exclusive,
            self.check_solve_fact_build_exclusive,
            self.declaration_publication_exclusive,
            self.lower_verify_install_exclusive,
        ].into_iter().flatten().try_fold(0u128, u128::checked_add)
            .expect("exclusive preparation phase sum must fit in nanoseconds");
        self.preparation_inclusive.checked_sub(observed)
            .expect("exclusive preparation phases must fit inside preparation")
    }

    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\"clock\":\"monotonic_wall\",\"preparation_inclusive\":{},",
                "\"project_parse_load_exclusive\":{},\"check_solve_fact_build_exclusive\":{},",
                "\"declaration_publication_exclusive\":{},\"lower_verify_install_exclusive\":{},",
                "\"preparation_other_exclusive\":{},\"execution_finish_exclusive\":{},",
                "\"diagnostic_finish_exclusive\":{}}}"
            ),
            self.preparation_inclusive,
            optional_ns(self.project_parse_load_exclusive),
            optional_ns(self.check_solve_fact_build_exclusive),
            optional_ns(self.declaration_publication_exclusive),
            optional_ns(self.lower_verify_install_exclusive),
            self.preparation_other_exclusive(),
            optional_ns(self.execution_finish_exclusive),
            optional_ns(self.diagnostic_finish_exclusive),
        )
    }
}

fn optional_ns(duration: Option<u128>) -> String {
    duration.map_or_else(|| "null".to_string(), |duration| duration.to_string())
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
    measured_script(options).0
}

/// Run once and return allocation data plus the tooling phase JSON report.
/// Instrumented durations are not unobserved performance samples.
pub fn run_script_with_phase_report(options: RunOptions) -> (MeasuredScriptRun, String) {
    let (measured, wall_time_ns) = measured_script(options);
    let report = measured.json_report(Some(&wall_time_ns));
    (measured, report)
}

fn measured_script(options: RunOptions) -> (MeasuredScriptRun, RuntimePhaseWallTimes) {
    let (output, phases) = run_script_with_allocation_stats(options);
    let wall_time_ns = phases.wall_time_ns.clone();
    let measured = MeasuredScriptRun {
        output,
        allocation: RuntimeAllocationStats::from_phases(phases),
    };
    (measured, wall_time_ns)
}

impl RuntimeAllocationStats {
    fn from_phases(phases: RuntimeAllocationPhases) -> Self {
        Self {
            tracking_active: mem_track::tracking_installed(),
            construction: phases.construction,
            controller: phases.controller,
            execution: mem_track::take_eval_traffic().unwrap_or_default(),
            workers: WorkerAllocationStats::from_stages(&phases.worker_stages),
        }
    }
}

impl MeasuredScriptRun {
    /// Serialize the measurement without mixing it into the script's stdout.
    pub fn to_json(&self) -> String {
        self.json_report(None)
    }

    fn json_report(&self, wall_time_ns: Option<&RuntimePhaseWallTimes>) -> String {
        let phases = wall_time_ns.map_or_else(String::new, |times| {
            format!(",\"wall_time_ns\":{}", times.to_json())
        });
        format!(
            "{{\"tracking_active\":{},\"status\":{},\"stdout_bytes\":{},\"stderr_bytes\":{},\"construction\":{},\"controller\":{},\"execution\":{},\"workers\":{}{}}}\n",
            self.allocation.tracking_active,
            self.output.status,
            self.output.stdout.len(),
            self.output.stderr.len(),
            traffic_json(&self.allocation.construction),
            traffic_json(&self.allocation.controller),
            traffic_json(&self.allocation.execution),
            worker_json(&self.allocation.workers),
            phases,
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
    fn phase_wall_times_reconcile_exclusive_work_without_counting_execution_twice() {
        let times = super::RuntimePhaseWallTimes {
            preparation_inclusive: 100,
            project_parse_load_exclusive: Some(10),
            check_solve_fact_build_exclusive: Some(20),
            declaration_publication_exclusive: Some(30),
            lower_verify_install_exclusive: Some(25),
            execution_finish_exclusive: Some(500),
            diagnostic_finish_exclusive: None,
        };
        assert_eq!(times.preparation_other_exclusive(), 15);
        let json = times.to_json();
        assert!(json.contains("\"execution_finish_exclusive\":500"));
        assert!(json.contains("\"diagnostic_finish_exclusive\":null"));
        let interrupted = super::RuntimePhaseWallTimes {
            preparation_inclusive: 45,
            project_parse_load_exclusive: Some(10),
            check_solve_fact_build_exclusive: Some(20),
            diagnostic_finish_exclusive: Some(5),
            ..super::RuntimePhaseWallTimes::default()
        };
        assert_eq!(interrupted.preparation_other_exclusive(), 15);
        assert!(interrupted.to_json().contains("\"declaration_publication_exclusive\":null"));
    }

    #[test]
    #[should_panic(expected = "exclusive preparation phases must fit inside preparation")]
    fn phase_wall_times_reject_overlapping_or_out_of_parent_measurements() {
        let times = super::RuntimePhaseWallTimes {
            preparation_inclusive: 10,
            project_parse_load_exclusive: Some(11),
            ..super::RuntimePhaseWallTimes::default()
        };
        times.preparation_other_exclusive();
    }

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
    fn phase_report_runs_once_and_preserves_allocation_json_fields() {
        use crate::modules::json::{compact_raw_json, parse_raw_json, raw_json_as_u64, raw_json_get};
        use miniserde::json::Value as JsonValue;

        let root = tempfile::tempdir().expect("create phase report fixture");
        let script = root.path().join("once.xsh");
        let counter = root.path().join("counter");
        std::fs::write(&counter, "0").expect("initialize execution counter");
        std::fs::write(&script, r#"let counter = fp"{args[0]}"
let count = counter.read_text()?.parse_int()?
counter.write(f"{count + 1}")?
print "once"
"#).expect("write phase report fixture");
        let (measured, report) = super::run_script_with_phase_report(crate::execution::script::RunOptions {
            script: script.display().to_string(),
            args: vec![counter.into_os_string()],
            coverage_trace_dir: None,
        });
        assert_eq!(measured.output.status, 0, "{:?}", measured.output.stderr);
        assert_eq!(measured.output.stdout, b"once\n");
        assert!(measured.output.stderr.is_empty());
        assert_eq!(std::fs::read_to_string(root.path().join("counter")).expect("read execution counter"), "1");
        let allocation = parse_raw_json(&measured.to_json()).expect("allocation JSON");
        let JsonValue::Object(mut report) = parse_raw_json(&report).expect("phase JSON") else {
            panic!("phase report must be an object");
        };
        let phases = report.remove("wall_time_ns").expect("phase report adds wall times");
        assert_eq!(compact_raw_json(&JsonValue::Object(report)), compact_raw_json(&allocation));
        assert!(raw_json_get(&allocation, "wall_time_ns").is_none());
        assert!(raw_json_get(&phases, "preparation_inclusive").and_then(raw_json_as_u64).is_some());
        assert!(raw_json_get(&phases, "execution_finish_exclusive").and_then(raw_json_as_u64).is_some());
        assert!(matches!(raw_json_get(&phases, "diagnostic_finish_exclusive"), Some(JsonValue::Null)));
    }

    #[test]
    fn allocation_literal_and_json_remain_compatible() {
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
            "{\"tracking_active\":true,\"status\":0,\"stdout_bytes\":3,\"stderr_bytes\":0,\"construction\":{\"live_bytes\":0,\"peak_bytes\":0,\"alloc_count\":0,\"alloc_bytes\":0},\"controller\":{\"live_bytes\":0,\"peak_bytes\":0,\"alloc_count\":0,\"alloc_bytes\":0},\"execution\":{\"live_bytes\":0,\"peak_bytes\":0,\"alloc_count\":0,\"alloc_bytes\":0},\"workers\":{\"stage_count\":2,\"alloc_count\":0,\"alloc_bytes\":0,\"peak_thread_bytes\":0,\"attributions\":[]}}\n"
        );
    }
}
