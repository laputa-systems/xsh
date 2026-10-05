use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A frontend stage a tooling command spends time in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    /// Reading configuration and finding the files to process.
    Discover,
    /// Reading, lexing, parsing, and resolving imports.
    Load,
    /// Type and effect checking, which also publishes the facts lowering
    /// consumes.
    Check,
    /// Lowering to the indexed program and verifying it.
    Lower,
    /// Lint rules.
    Lint,
    /// Computing and validating autofix rewrites.
    Fix,
}

impl Stage {
    const ALL: [Self; 6] = [
        Self::Discover,
        Self::Load,
        Self::Check,
        Self::Lower,
        Self::Lint,
        Self::Fix,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Discover => "discover",
            Self::Load => "load",
            Self::Check => "check",
            Self::Lower => "lower",
            Self::Lint => "lint",
            Self::Fix => "fix",
        }
    }
}

/// Where one `xsht check` or `xsht lint` run spent its time.
///
/// A stage accumulates the time of every thread that ran it, so the stage
/// times of a command with parallel workers add up to more than its wall-clock
/// total. A stage the run never entered is left out of the report.
pub struct StageTimings {
    started: Instant,
    files: AtomicUsize,
    nanos: [AtomicU64; Stage::ALL.len()],
    entered: [AtomicUsize; Stage::ALL.len()],
}

impl StageTimings {
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
            files: AtomicUsize::new(0),
            nanos: Default::default(),
            entered: Default::default(),
        }
    }

    pub fn time<T>(&self, stage: Stage, run: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = run();
        self.record(stage, started.elapsed());
        value
    }

    pub fn record(&self, stage: Stage, elapsed: Duration) {
        let index = stage as usize;
        let nanos = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        self.nanos[index].fetch_add(nanos, Ordering::Relaxed);
        self.entered[index].fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_files(&self, files: usize) {
        self.files.store(files, Ordering::Relaxed);
    }

    /// The closing report line, or `None` when the command stopped before it
    /// processed any file.
    pub fn report(&self, command: &str) -> Option<String> {
        let files = self.files.load(Ordering::Relaxed);
        if files == 0 {
            return None;
        }
        let stages = Stage::ALL
            .iter()
            .filter(|stage| self.entered[**stage as usize].load(Ordering::Relaxed) > 0)
            .map(|stage| {
                let nanos = self.nanos[*stage as usize].load(Ordering::Relaxed);
                format!("{} {}", stage.label(), seconds(Duration::from_nanos(nanos)))
            })
            .collect::<Vec<_>>();
        let noun = if files == 1 { "file" } else { "files" };
        Some(format!(
            "xsht {command}: {files} {noun} in {} (thread time by stage: {})\n",
            seconds(self.started.elapsed()),
            stages.join(", "),
        ))
    }
}

fn seconds(duration: Duration) -> String {
    format!("{:.2}s", duration.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_names_only_entered_stages_in_pipeline_order() {
        let timings = StageTimings::start();
        timings.set_files(2);
        timings.time(Stage::Lint, || ());
        timings.time(Stage::Load, || ());
        let report = timings.report("lint").expect("files were processed");
        assert!(report.starts_with("xsht lint: 2 files in "), "{report}");
        assert!(
            report.ends_with("s (thread time by stage: load 0.00s, lint 0.00s)\n"),
            "{report}"
        );
    }

    #[test]
    fn report_is_absent_when_no_file_was_processed() {
        let timings = StageTimings::start();
        timings.time(Stage::Discover, || ());
        assert_eq!(timings.report("check"), None);
    }
}
