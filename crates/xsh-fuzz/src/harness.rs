//! Checking in-process and executing in a sandboxed child.
//!
//! Checking never executes code, so it runs in-process under
//! `catch_unwind`. Execution runs in a child re-exec of this binary
//! (`xsh-fuzz exec FILE`) through the ordinary script runner,
//! `xsh::execution::script::run_script`, with a cleared environment, a private
//! temporary working directory, resource limits, a wall-clock timeout, and an
//! output cap.

use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use xsh::execution::evaluator::Evaluator;
use xsh::frontend::check::{CheckOptions, Checker};
use xsh::frontend::load::parse_load_check_text;
use xsh::frontend::source::SourceMap;

/// Text that marks an implementation defect rather than a program error.
pub const INTERNAL_MARKERS: &[&str] = &[
    "indexed IR could not encode",
    "could not encode",
    "panicked",
    "internal error",
    "evaluator defect",
    "not yet implemented",
    "entered unreachable code",
    "RUST_BACKTRACE",
    "verified startup IR",
];

pub fn internal_marker(text: &str) -> Option<&'static str> {
    INTERNAL_MARKERS
        .iter()
        .copied()
        .find(|marker| text.contains(marker))
}

/// Text that marks a runtime type disagreement with the checker.
pub fn runtime_type_error(stderr: &str) -> bool {
    stderr.lines().any(|line| {
        let line = line.trim();
        (line.contains("expected ") && line.contains(", found "))
            || line.contains("type mismatch")
            || line.contains("wrong type")
            || line.contains("runtime.type")
    })
}

#[derive(Clone, Debug, Default)]
pub struct CheckReport {
    pub parse: Vec<String>,
    pub check: Vec<String>,
    /// Preparation (lowering) diagnostics for a checker-accepted program.
    pub lower: Vec<String>,
    /// Panic payload, if checking panicked.
    pub panic: Option<String>,
    /// Diagnostics whose spans do not address the source text.
    pub bad_spans: Vec<String>,
    /// Lint codes, when requested and the program checked clean.
    pub lint_codes: Option<Vec<String>>,
}

impl CheckReport {
    pub fn accepted(&self) -> bool {
        self.panic.is_none()
            && self.parse.is_empty()
            && self.check.is_empty()
            && self.lower.is_empty()
    }

    pub fn internal_error(&self) -> Option<String> {
        if let Some(panic) = &self.panic {
            return Some(format!("checker panicked: {panic}"));
        }
        for diagnostic in self.parse.iter().chain(&self.check).chain(&self.lower) {
            if let Some(marker) = internal_marker(diagnostic) {
                return Some(format!("internal diagnostic ({marker}): {diagnostic}"));
            }
        }
        if let Some(span) = self.bad_spans.first() {
            return Some(format!("diagnostic with an invalid span: {span}"));
        }
        None
    }
}

fn describe(diagnostic: &Diagnostic) -> String {
    format!(
        "{}[{}]: {}",
        diagnostic.severity.as_str(),
        diagnostic.code.map_or("-", DiagnosticCode::name),
        diagnostic.message
    )
}

fn span_problems(diagnostics: &[Diagnostic], sources: &SourceMap, out: &mut Vec<String>) {
    for diagnostic in diagnostics {
        let spans = diagnostic
            .span
            .iter()
            .chain(diagnostic.labels.iter().map(|label| &label.span))
            .chain(
                diagnostic
                    .fix_hints
                    .iter()
                    .filter_map(|hint| hint.span.as_ref()),
            );
        for span in spans {
            let valid = sources.get(span.source_id).is_some_and(|file| {
                let text = file.text();
                span.end() <= text.len()
                    && text.is_char_boundary(span.start())
                    && text.is_char_boundary(span.end())
            });
            if !valid {
                out.push(format!(
                    "{} at {}..{}",
                    describe(diagnostic),
                    span.start(),
                    span.end()
                ));
            }
        }
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-text panic payload".into()
    }
}

/// Parses, checks, and prepares `text` without executing it.
pub fn check_text(file: &str, text: &str) -> CheckReport {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut report = CheckReport::default();
        let mut entry = parse_load_check_text(
            file,
            text.to_string(),
            Vec::new(),
            CheckOptions {
                interactive_commands: None,
                reveal_types: false,
                migration_diagnostics: true,
                embedded_bodies: true,
            },
        );
        span_problems(
            &entry.parsed.diagnostics,
            &entry.sources,
            &mut report.bad_spans,
        );
        report.parse = entry
            .parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .map(describe)
            .collect();
        if !report.parse.is_empty() {
            return report;
        }
        let Some(checked) = entry.checked.take() else {
            report
                .check
                .push("no check output after a clean parse".into());
            return report;
        };
        span_problems(&checked.diagnostics, &entry.sources, &mut report.bad_spans);
        report.check = checked
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .map(describe)
            .collect();
        if !report.check.is_empty() {
            return report;
        }
        let declarations = Checker::compact_declarations(&entry.parsed.arena, checked);
        let lowered = Evaluator::compact_lowerability_diagnostics_with_parts(
            &entry.parsed.arena,
            entry.entry_source_id,
            entry.sources.clone(),
            declarations,
            Vec::new(),
            "xsh-fuzz".into(),
        );
        span_problems(&lowered, &entry.sources, &mut report.bad_spans);
        report.lower = lowered.iter().map(describe).collect();
        report
    }));
    match result {
        Ok(report) => report,
        Err(payload) => CheckReport {
            panic: Some(panic_text(&*payload)),
            ..CheckReport::default()
        },
    }
}

#[derive(Clone, Debug)]
pub struct RunReport {
    pub status: Option<i32>,
    pub signal: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    /// The child's physical footprint when it was killed for exceeding the
    /// memory limit.
    pub memory_exceeded: Option<u64>,
    /// The largest footprint sampled while the child ran.
    pub peak_memory: u64,
    pub elapsed: Duration,
}

/// Limits for one sandboxed execution.
#[derive(Clone, Debug)]
pub struct Sandbox {
    /// The `xsh-fuzz` executable that provides the `exec` worker.
    pub exe: PathBuf,
    pub timeout: Duration,
    pub output_cap: usize,
    /// Physical footprint above which the child is killed. macOS does not
    /// enforce data or address-space rlimits, so the parent samples the
    /// footprint (`proc_pid_rusage` on macOS, `VmRSS` on Linux) while it
    /// waits.
    pub memory_limit: u64,
}

pub const DEFAULT_MEMORY_LIMIT: u64 = 256 << 20;

impl Sandbox {
    pub fn new(exe: PathBuf) -> Self {
        Self {
            exe,
            timeout: Duration::from_secs(10),
            output_cap: 1 << 20,
            memory_limit: DEFAULT_MEMORY_LIMIT,
        }
    }

    /// Runs `source` as a script in a fresh private directory.
    pub fn run(&self, source: &str) -> std::io::Result<RunReport> {
        let dir = tempfile::Builder::new().prefix("xsh-fuzz-").tempdir()?;
        let script = dir.path().join("program.xsh");
        std::fs::write(&script, source)?;
        let work = dir.path().join("work");
        std::fs::create_dir(&work)?;
        let started = Instant::now();
        let mut child = Command::new(&self.exe)
            .arg("exec")
            .arg(&script)
            .env_clear()
            .current_dir(&work)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let cap = self.output_cap;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let read_out = std::thread::spawn(move || read_capped(stdout, cap));
        let read_err = std::thread::spawn(move || read_capped(stderr, cap));
        let mut timed_out = false;
        let mut memory_exceeded = None;
        let mut peak_memory = 0;
        let mut pause = Duration::from_micros(200);
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if let Some(footprint) = footprint(child.id()) {
                peak_memory = peak_memory.max(footprint);
                if footprint > self.memory_limit {
                    memory_exceeded = Some(footprint);
                    let _ = child.kill();
                    break child.wait()?;
                }
            }
            if started.elapsed() > self.timeout {
                timed_out = true;
                let _ = child.kill();
                break child.wait()?;
            }
            std::thread::sleep(pause);
            pause = (pause * 2).min(Duration::from_millis(10));
        };
        let stdout = read_out.join().unwrap_or_default();
        let stderr = read_err.join().unwrap_or_default();
        use std::os::unix::process::ExitStatusExt;
        Ok(RunReport {
            status: status.code(),
            signal: status.signal(),
            stdout,
            stderr,
            timed_out,
            memory_exceeded,
            peak_memory,
            elapsed: started.elapsed(),
        })
    }
}

/// The physical memory footprint of a live process, in bytes.
#[cfg(target_os = "macos")]
pub fn footprint(pid: u32) -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    let pid = i32::try_from(pid).ok()?;
    // SAFETY: with RUSAGE_INFO_V2, proc_pid_rusage writes one rusage_info_v2
    // into the buffer, which is that size; on failure it is left untouched
    // and not read.
    let status =
        unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V2, info.as_mut_ptr().cast()) };
    // SAFETY: a zero status means the kernel filled the structure.
    (status == 0).then(|| unsafe { info.assume_init() }.ri_phys_footprint)
}

/// The resident memory of a live process, in bytes.
#[cfg(target_os = "linux")]
pub fn footprint(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn footprint(_pid: u32) -> Option<u64> {
    None
}

fn read_capped(mut reader: impl Read, cap: usize) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                if buffer.len() < cap {
                    let take = count.min(cap - buffer.len());
                    buffer.extend_from_slice(&chunk[..take]);
                }
            }
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

/// The child side of [`Sandbox::run`]: apply resource limits, then run the
/// script through the ordinary runner. Never returns.
pub fn exec_worker(script: &Path) -> ! {
    use rustix::process::{Resource, Rlimit, setrlimit};
    let limit = |resource: Resource, value: u64| {
        let _ = setrlimit(
            resource,
            Rlimit {
                current: Some(value),
                maximum: Some(value),
            },
        );
    };
    limit(Resource::Cpu, 20);
    // No file may grow: generated programs have no filesystem effect.
    limit(Resource::Fsize, 0);
    limit(Resource::Core, 0);
    // Enforced on Linux only; the parent's footprint sampling is the bound
    // everywhere.
    limit(Resource::Data, 1 << 30);
    limit(Resource::Nofile, 64);
    let output = xsh::execution::script::run_script(xsh::execution::script::RunOptions {
        script: script.to_string_lossy().into_owned(),
        args: Vec::new(),
        coverage_trace_dir: None,
    });
    use std::io::Write;
    let _ = std::io::stdout().write_all(&output.stdout);
    let _ = std::io::stderr().write_all(&output.stderr);
    std::process::exit(i32::from(output.status))
}

/// Why a well-typed generated program failed.
#[derive(Clone, Debug)]
pub enum Failure {
    /// The checker rejected a program the generator built as well typed.
    Rejected(String),
    /// Checking or preparation produced an internal error or panicked.
    Internal(String),
    /// The program failed at runtime.
    Runtime(String),
    /// The program ran but printed something other than the reference output.
    WrongOutput {
        expected: String,
        actual: String,
    },
    Timeout,
    /// A small well-typed program exceeded the memory limit.
    Memory(String),
    Crash(String),
}

impl Failure {
    pub fn kind(&self) -> &'static str {
        match self {
            Failure::Rejected(_) => "rejected",
            Failure::Internal(_) => "internal",
            Failure::Runtime(_) => "runtime",
            Failure::WrongOutput { .. } => "wrong-output",
            Failure::Timeout => "timeout",
            Failure::Memory(_) => "memory",
            Failure::Crash(_) => "crash",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Failure::Rejected(text)
            | Failure::Internal(text)
            | Failure::Runtime(text)
            | Failure::Memory(text)
            | Failure::Crash(text) => text.clone(),
            Failure::WrongOutput { expected, actual } => {
                let mut text = String::new();
                for (index, (want, got)) in expected.lines().zip(actual.lines()).enumerate() {
                    if want != got {
                        text.push_str(&format!(
                            "line {}: expected {want:?}\n         actual   {got:?}\n",
                            index + 1
                        ));
                        break;
                    }
                }
                if text.is_empty() {
                    text = format!(
                        "expected {} lines, got {}",
                        expected.lines().count(),
                        actual.lines().count()
                    );
                }
                format!("{text}\n--- expected\n{expected}--- actual\n{actual}")
            }
            Failure::Timeout => "timed out".into(),
        }
    }
}

/// Checks a generated program and, when `sandbox` is given, runs it and
/// compares stdout with `expected`.
pub fn verify_generated(
    source: &str,
    expected: &str,
    sandbox: Option<&Sandbox>,
) -> Result<(), Failure> {
    let report = check_text("program.xsh", source);
    if let Some(internal) = report.internal_error() {
        return Err(Failure::Internal(internal));
    }
    if !report.accepted() {
        let mut text = String::new();
        for line in report
            .parse
            .iter()
            .chain(&report.check)
            .chain(&report.lower)
        {
            text.push_str(line);
            text.push('\n');
        }
        return Err(Failure::Rejected(text));
    }
    let Some(sandbox) = sandbox else {
        return Ok(());
    };
    let run = sandbox
        .run(source)
        .map_err(|error| Failure::Crash(format!("spawn failed: {error}")))?;
    if let Some(footprint) = run.memory_exceeded {
        return Err(Failure::Memory(format!(
            "killed at {} MiB, over the {} MiB limit",
            footprint >> 20,
            sandbox.memory_limit >> 20
        )));
    }
    if run.timed_out {
        return Err(Failure::Timeout);
    }
    if let Some(signal) = run.signal {
        return Err(Failure::Crash(format!(
            "killed by signal {signal}\n{}",
            run.stderr
        )));
    }
    if run.status != Some(0) {
        let kind = if let Some(marker) = internal_marker(&run.stderr) {
            format!("internal ({marker})")
        } else if runtime_type_error(&run.stderr) {
            "runtime type error".to_string()
        } else {
            "runtime error".to_string()
        };
        return Err(Failure::Runtime(format!(
            "{kind}, status {:?}\n{}",
            run.status, run.stderr
        )));
    }
    if run.stdout != expected {
        return Err(Failure::WrongOutput {
            expected: expected.to_string(),
            actual: run.stdout,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CheckReport, internal_marker};

    #[test]
    fn unresolved_unreachable_is_an_ordinary_program_error() {
        let report = CheckReport {
            check: vec![
                "error[check.unresolved-call]: unresolved pure function call `unreachable`"
                    .into(),
            ],
            ..CheckReport::default()
        };
        assert!(!report.accepted());
        assert!(report.internal_error().is_none());
    }

    #[test]
    fn unreachable_implementation_panics_are_still_defects() {
        assert!(internal_marker("internal error: entered unreachable code").is_some());
        let report = CheckReport {
            panic: Some("entered unreachable code".into()),
            ..CheckReport::default()
        };
        assert!(report.internal_error().is_some());
    }
}
