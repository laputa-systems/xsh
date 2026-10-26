//! Allocation traffic and observed wall/CPU boundaries for one indexed run.
//!
//! Wall time uses the monotonic clock and includes observer overhead. Allocation
//! traffic remains thread-attributed. CPU covers this process's threads and
//! excludes descendants and waiting; none of these signals measures process RSS.

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
pub(crate) struct RuntimePhaseTimes {
    pub wall_time_ns: RuntimePhaseDurations,
    pub cpu_time_ns: RuntimePhaseDurations,
}

pub(crate) struct PhaseDurationSlot<'a> {
    pub wall_ns: &'a mut Option<u128>,
    pub cpu_ns: &'a mut Option<u128>,
}

impl RuntimePhaseTimes {
    pub fn slot(&mut self, select: fn(&mut RuntimePhaseDurations) -> &mut Option<u128>) -> PhaseDurationSlot<'_> {
        PhaseDurationSlot {
            wall_ns: select(&mut self.wall_time_ns),
            cpu_ns: select(&mut self.cpu_time_ns),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimePhaseDurations {
    pub preparation_inclusive: u128,
    pub project_parse_load_exclusive: Option<u128>,
    pub check_solve_fact_build_exclusive: Option<u128>,
    pub declaration_publication_exclusive: Option<u128>,
    pub lower_verify_install_exclusive: Option<u128>,
    pub execution_finish_exclusive: Option<u128>,
    pub diagnostic_finish_exclusive: Option<u128>,
}

impl RuntimePhaseDurations {
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

    fn report<'a>(&self, clock: &'a str) -> PhaseReport<'a> {
        PhaseReport {
            clock,
            preparation_inclusive: self.preparation_inclusive,
            project_parse_load_exclusive: self.project_parse_load_exclusive,
            check_solve_fact_build_exclusive: self.check_solve_fact_build_exclusive,
            declaration_publication_exclusive: self.declaration_publication_exclusive,
            lower_verify_install_exclusive: self.lower_verify_install_exclusive,
            preparation_other_exclusive: self.preparation_other_exclusive(),
            execution_finish_exclusive: self.execution_finish_exclusive,
            diagnostic_finish_exclusive: self.diagnostic_finish_exclusive,
        }
    }
}

#[derive(deser::Serialize)]
struct PhaseReport<'a> {
    clock: &'a str,
    preparation_inclusive: u128,
    project_parse_load_exclusive: Option<u128>,
    check_solve_fact_build_exclusive: Option<u128>,
    declaration_publication_exclusive: Option<u128>,
    lower_verify_install_exclusive: Option<u128>,
    preparation_other_exclusive: u128,
    execution_finish_exclusive: Option<u128>,
    diagnostic_finish_exclusive: Option<u128>,
}

#[derive(deser::Serialize)]
struct TrafficReport {
    live_bytes: usize,
    peak_bytes: usize,
    alloc_count: usize,
    alloc_bytes: usize,
}

impl From<&AllocTraffic> for TrafficReport {
    fn from(traffic: &AllocTraffic) -> Self {
        Self { live_bytes: traffic.live_bytes, peak_bytes: traffic.peak_bytes,
            alloc_count: traffic.alloc_count, alloc_bytes: traffic.alloc_bytes }
    }
}

#[derive(deser::Serialize)]
struct AttributionReport<'a> {
    scope: &'a str,
    alloc_count: usize,
    alloc_bytes: usize,
}

#[derive(deser::Serialize)]
struct WorkerReport<'a> {
    stage_count: usize,
    alloc_count: usize,
    alloc_bytes: usize,
    peak_thread_bytes: usize,
    attributions: Vec<AttributionReport<'a>>,
}

impl<'a> From<&'a WorkerAllocationStats> for WorkerReport<'a> {
    fn from(workers: &'a WorkerAllocationStats) -> Self {
        Self {
            stage_count: workers.stage_count,
            alloc_count: workers.alloc_count,
            alloc_bytes: workers.alloc_bytes,
            peak_thread_bytes: workers.peak_thread_bytes,
            attributions: workers.attributions.iter().map(|attribution| AttributionReport {
                scope: attribution.scope,
                alloc_count: attribution.alloc_count,
                alloc_bytes: attribution.alloc_bytes,
            }).collect(),
        }
    }
}

#[derive(deser::Serialize)]
struct MeasurementReport<'a> {
    tracking_active: bool,
    status: u8,
    stdout_bytes: usize,
    stderr_bytes: usize,
    construction: TrafficReport,
    controller: TrafficReport,
    execution: TrafficReport,
    workers: WorkerReport<'a>,
    #[deser(skip_serializing_if = Option::is_none)]
    wall_time_ns: Option<PhaseReport<'a>>,
    #[deser(skip_serializing_if = Option::is_none)]
    cpu_time_ns: Option<PhaseReport<'a>>,
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
    let (measured, phase_times) = measured_script(options);
    let report = measured.json_report(Some(&phase_times));
    (measured, report)
}

fn measured_script(options: RunOptions) -> (MeasuredScriptRun, RuntimePhaseTimes) {
    let (output, phases) = run_script_with_allocation_stats(options);
    let phase_times = phases.phase_times.clone();
    let measured = MeasuredScriptRun {
        output,
        allocation: RuntimeAllocationStats::from_phases(phases),
    };
    (measured, phase_times)
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

    fn json_report(&self, phase_times: Option<&RuntimePhaseTimes>) -> String {
        let report = MeasurementReport {
            tracking_active: self.allocation.tracking_active,
            status: self.output.status,
            stdout_bytes: self.output.stdout.len(),
            stderr_bytes: self.output.stderr.len(),
            construction: TrafficReport::from(&self.allocation.construction),
            controller: TrafficReport::from(&self.allocation.controller),
            execution: TrafficReport::from(&self.allocation.execution),
            workers: WorkerReport::from(&self.allocation.workers),
            wall_time_ns: phase_times.map(|times| times.wall_time_ns.report("monotonic_wall")),
            cpu_time_ns: phase_times.map(|times| times.cpu_time_ns.report("process_cpu")),
        };
        let mut output = deser_json::to_string(&report)
            .expect("allocation reports contain only strings, integers, and booleans");
        output.push('\n');
        output
    }
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
        let times = super::RuntimePhaseDurations {
            preparation_inclusive: 100,
            project_parse_load_exclusive: Some(10),
            check_solve_fact_build_exclusive: Some(20),
            declaration_publication_exclusive: Some(30),
            lower_verify_install_exclusive: Some(25),
            execution_finish_exclusive: Some(500),
            diagnostic_finish_exclusive: None,
        };
        assert_eq!(times.preparation_other_exclusive(), 15);
        let json = deser_json::to_string(&times.report("monotonic_wall")).expect("phase JSON");
        assert!(json.contains("\"execution_finish_exclusive\":500"));
        assert!(json.contains("\"diagnostic_finish_exclusive\":null"));
        let interrupted = super::RuntimePhaseDurations {
            preparation_inclusive: 45,
            project_parse_load_exclusive: Some(10),
            check_solve_fact_build_exclusive: Some(20),
            diagnostic_finish_exclusive: Some(5),
            ..super::RuntimePhaseDurations::default()
        };
        assert_eq!(interrupted.preparation_other_exclusive(), 15);
        assert!(deser_json::to_string(&interrupted.report("monotonic_wall")).expect("phase JSON").contains("\"declaration_publication_exclusive\":null"));
    }

    #[test]
    #[should_panic(expected = "exclusive preparation phases must fit inside preparation")]
    fn phase_wall_times_reject_overlapping_or_out_of_parent_measurements() {
        let times = super::RuntimePhaseDurations {
            preparation_inclusive: 10,
            project_parse_load_exclusive: Some(11),
            ..super::RuntimePhaseDurations::default()
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
        let allocation = xsh::host::json::parse(&measured.to_json()).expect("allocation JSON");
        let mut report = xsh::host::json::parse(&report).expect("phase JSON");
        let report_fields = report.as_map_mut().expect("phase report must be an object");
        let phases = report_fields.remove("wall_time_ns").expect("phase report adds wall times");
        let cpu = report_fields.remove("cpu_time_ns").expect("phase report adds CPU times");
        assert_eq!(xsh::host::json::encode(&report).expect("host JSON document must be encodable"), xsh::host::json::encode(&allocation).expect("host JSON document must be encodable"));
        assert!((&allocation).get("wall_time_ns").is_none());
        assert!((&allocation).get("cpu_time_ns").is_none());
        assert!((&phases).get("preparation_inclusive").and_then(|value| value.as_u64()).is_some());
        assert!((&phases).get("execution_finish_exclusive").and_then(|value| value.as_u64()).is_some());
        assert!(phases.get("diagnostic_finish_exclusive").is_some_and(|value| value.is_null()));
        assert!(cpu.get("clock").and_then(|value| value.as_str()) == Some("process_cpu"));
        for key in ["preparation_inclusive", "project_parse_load_exclusive", "check_solve_fact_build_exclusive", "declaration_publication_exclusive", "lower_verify_install_exclusive", "preparation_other_exclusive", "execution_finish_exclusive"] {
            assert!((&cpu).get(key).and_then(|value| value.as_u64()).is_some(), "{key}");
        }
        assert!(cpu.get("diagnostic_finish_exclusive").is_some_and(|value| value.is_null()));
    }

    #[test]
    fn phase_json_preserves_the_full_nanosecond_domain() {
        let times = super::RuntimePhaseDurations {
            preparation_inclusive: u128::MAX,
            execution_finish_exclusive: Some(u128::MAX),
            ..super::RuntimePhaseDurations::default()
        };
        let json = deser_json::to_string(&times.report("process_cpu")).expect("phase JSON");
        let value = crate::host::json::parse(&json).expect("phase document");
        for name in ["preparation_inclusive", "preparation_other_exclusive", "execution_finish_exclusive"] {
            assert_eq!(value.get(name).and_then(|value| value.as_u128()), Some(u128::MAX));
        }
        assert!(value.get("diagnostic_finish_exclusive").is_some_and(|value| value.is_null()));
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
