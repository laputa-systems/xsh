//! Fuzzing campaigns: seeds, parallel workers, minimization, and failure
//! files.

use crate::generator::{GenConfig, Generated, finish, generate};
use crate::harness::{Failure, Sandbox, verify_generated};
use crate::mutate::{MUTANT_FILE, check_mutant, corpus_files, format_invariants, mutate};
use crate::rng::Rng;
use crate::shrink::{shrink_program, shrink_text};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// Semantic checks only; nothing executes.
    Check,
    /// Execute well-typed generated programs in the sandbox.
    Run,
    /// Alternate both.
    All,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub mode: Mode,
    pub seed: u64,
    pub iterations: Option<u64>,
    pub duration: Option<Duration>,
    pub jobs: usize,
    pub out: PathBuf,
    pub shrink: bool,
    pub timeout: Duration,
    pub corpus_root: PathBuf,
    /// Mutants per iteration, split between the generated program and a
    /// corpus file.
    pub mutants: usize,
    /// Formatter and lint invariants every Nth check iteration (0 disables).
    pub format_every: u64,
    pub exe: PathBuf,
    /// Iteration index of the first seed (`seed + first + i`).
    pub first: u64,
    /// Iterations per worker process; 0 runs in this process. Each worker
    /// exits after its batch, so state the frontend retains between checks
    /// (interned names, prepared modules) cannot accumulate.
    pub shard_size: u64,
    /// Footprint above which a worker process is killed and reported.
    pub worker_memory_limit: u64,
}

#[derive(Debug, Default)]
pub struct Stats {
    pub checked: AtomicUsize,
    pub mutants: AtomicUsize,
    pub formatted: AtomicUsize,
    pub ran: AtomicUsize,
    pub failures: AtomicUsize,
    pub check_time_us: AtomicU64,
    pub run_time_us: AtomicU64,
}

/// Minimizes a failing generated program to a smaller one that fails with
/// the same kind.
pub fn minimize(
    generated: &Generated,
    failure: &Failure,
    sandbox: Option<&Sandbox>,
    budget: usize,
) -> (Generated, Failure) {
    let kind = failure.kind();
    let mut best = (generated.clone(), failure.clone());
    quiet_panics(|| {
        shrink_program(
            &generated.program,
            |candidate| {
                // Candidates that delete a binding still in use make the
                // reference evaluator panic; those are simply not failures.
                let Ok(Some(candidate)) = catch_unwind(AssertUnwindSafe(|| {
                    finish(generated.seed, candidate.clone())
                })) else {
                    return false;
                };
                match verify_generated(&candidate.source, &candidate.expected, sandbox) {
                    Err(found) if found.kind() == kind && same_failure(failure, &found) => {
                        best = (candidate, found);
                        true
                    }
                    _ => false,
                }
            },
            budget,
        )
    });
    best
}

static PANIC_HOOK: Mutex<()> = Mutex::new(());

fn quiet_panics<T>(body: impl FnOnce() -> T) -> T {
    let _guard = PANIC_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = body();
    std::panic::set_hook(previous);
    result
}

/// A shrink must keep the failure's first diagnostic (code and message), so
/// it cannot wander to an unrelated mistake.
fn same_failure(original: &Failure, found: &Failure) -> bool {
    match (original, found) {
        (Failure::Rejected(a), Failure::Rejected(b))
        | (Failure::Internal(a), Failure::Internal(b)) => a.lines().next() == b.lines().next(),
        (Failure::Runtime(a), Failure::Runtime(b)) => runtime_head(a) == runtime_head(b),
        _ => true,
    }
}

fn runtime_head(text: &str) -> Option<&str> {
    text.lines()
        .find(|line| line.starts_with("err["))
        .map(|line| line.split(':').next().unwrap_or(line))
}

/// The first line of a failure with numbers (offsets, line:column) erased,
/// so a shrink keeps the same kind of failure while positions move.
pub fn signature(failure: &str) -> String {
    let line = failure.lines().next().unwrap_or("");
    let mut out = String::new();
    let mut in_number = false;
    for ch in line.chars() {
        if ch.is_ascii_digit() {
            if !in_number {
                out.push('#');
            }
            in_number = true;
        } else {
            in_number = false;
            out.push(ch);
        }
    }
    out
}

/// Shrinks a program whose formatter or lint invariant failed.
pub fn minimize_format(generated: &Generated, failure: &str, scratch: &Path) -> (String, String) {
    let head = signature(failure);
    let mut best = (generated.source.clone(), failure.to_string());
    quiet_panics(|| {
        shrink_program(
            &generated.program,
            |candidate| {
                let Ok(Some(candidate)) = catch_unwind(AssertUnwindSafe(|| {
                    finish(generated.seed, candidate.clone())
                })) else {
                    return false;
                };
                if verify_generated(&candidate.source, &candidate.expected, None).is_err() {
                    return false;
                }
                match catch_unwind(AssertUnwindSafe(|| {
                    format_invariants(&candidate.source, scratch)
                })) {
                    Ok(Err(found)) if signature(&found) == head => {
                        best = (candidate.source, found);
                        true
                    }
                    _ => false,
                }
            },
            150,
        )
    });
    best
}

/// Writes a reproducer to `dir/<name>.xsh` with the failure text as a leading
/// comment block, returning the path.
pub fn write_failure(
    dir: &Path,
    name: &str,
    source: &str,
    failure: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.xsh"));
    let mut text = String::new();
    for line in failure.lines().take(80) {
        text.push_str("# ");
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(source);
    std::fs::write(&path, text)?;
    Ok(path)
}

/// Generates and verifies one seed.
pub fn run_seed(
    seed: u64,
    config: &GenConfig,
    sandbox: Option<&Sandbox>,
) -> Result<Generated, (Generated, Failure)> {
    let generated = generate(seed, config);
    match verify_generated(&generated.source, &generated.expected, sandbox) {
        Ok(()) => Ok(generated),
        Err(failure) => Err((generated, failure)),
    }
}

struct Shared {
    options: Options,
    stats: Stats,
    next: AtomicU64,
    stop: AtomicBool,
    started: Instant,
    corpus: Vec<PathBuf>,
    /// What each worker is checking in-process, for the hang watchdog.
    current: Vec<Mutex<Option<(Instant, String, String)>>>,
    log: Mutex<()>,
}

impl Shared {
    fn report(&self, name: &str, source: &str, failure: &str) {
        self.stats.failures.fetch_add(1, Ordering::Relaxed);
        let path = write_failure(&self.options.out, name, source, failure);
        let _guard = self.log.lock();
        match path {
            Ok(path) => eprintln!(
                "FAIL {name}: {} -> {}",
                failure.lines().next().unwrap_or(""),
                path.display()
            ),
            Err(error) => eprintln!("FAIL {name}: {failure} (could not write reproducer: {error})"),
        }
    }

    /// Minimize failures unless disabled or the campaign is out of time.
    fn should_shrink(&self) -> bool {
        self.options.shrink
            && self
                .options
                .duration
                .is_none_or(|duration| self.started.elapsed() < duration)
    }

    fn take_seed(&self) -> Option<u64> {
        if self.stop.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(duration) = self.options.duration
            && self.started.elapsed() >= duration
        {
            return None;
        }
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        if let Some(iterations) = self.options.iterations
            && index >= iterations
        {
            return None;
        }
        Some(self.options.first + index)
    }
}

/// Runs a campaign and returns its statistics: in worker processes of
/// `shard_size` iterations each, or in this process when that is 0.
pub fn campaign(options: Options) -> Stats {
    if options.shard_size > 0 {
        return supervise(options);
    }
    in_process(options)
}

/// One line a worker process prints last, with its counts.
const STATS_PREFIX: &str = "xsh-fuzz-shard-stats";

fn print_shard_stats(stats: &Stats) {
    println!(
        "{STATS_PREFIX} {} {} {} {} {}",
        stats.checked.load(Ordering::Relaxed),
        stats.mutants.load(Ordering::Relaxed),
        stats.formatted.load(Ordering::Relaxed),
        stats.ran.load(Ordering::Relaxed),
        stats.failures.load(Ordering::Relaxed)
    );
}

/// The worker side of [`supervise`]: one batch in this process, then the
/// stats line.
pub fn run_shard(options: Options) {
    let stats = in_process(options);
    print_shard_stats(&stats);
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Check => "check",
        Mode::Run => "run",
        Mode::All => "all",
    }
}

/// Runs batches in at most `jobs` concurrent worker processes until the
/// iteration count or duration is reached, killing a worker whose footprint
/// exceeds `worker_memory_limit`.
fn supervise(options: Options) -> Stats {
    let stats = Stats::default();
    let next = AtomicU64::new(0);
    let started = Instant::now();
    let jobs = options.jobs.max(1);
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    if options
                        .duration
                        .is_some_and(|duration| started.elapsed() >= duration)
                    {
                        break;
                    }
                    let first = next.fetch_add(options.shard_size, Ordering::Relaxed);
                    let mut count = options.shard_size;
                    if let Some(iterations) = options.iterations {
                        if first >= iterations {
                            break;
                        }
                        count = count.min(iterations - first);
                    }
                    let remaining = options
                        .duration
                        .map(|duration| duration.saturating_sub(started.elapsed()));
                    if remaining.is_some_and(|remaining| remaining.is_zero()) {
                        break;
                    }
                    run_worker(&options, first, count, remaining, &stats);
                }
            });
        }
    });
    stats
}

fn run_worker(
    options: &Options,
    first: u64,
    count: u64,
    remaining: Option<Duration>,
    stats: &Stats,
) {
    let mut command = std::process::Command::new(&options.exe);
    command
        .arg("shard")
        .arg(mode_name(options.mode))
        .args(["--seed", &options.seed.to_string()])
        .args(["--first", &first.to_string()])
        .args(["--iterations", &count.to_string()])
        .args(["--timeout", &options.timeout.as_secs().to_string()])
        .args(["--mutants", &options.mutants.to_string()])
        .args(["--format-every", &options.format_every.to_string()])
        .arg("--out")
        .arg(&options.out)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped());
    if let Some(remaining) = remaining {
        command.args(["--duration", &remaining.as_secs().max(1).to_string()]);
    }
    if !options.shrink {
        command.arg("--no-shrink");
    }
    let Ok(mut child) = command.spawn() else {
        eprintln!("xsh-fuzz: could not start a worker process");
        stats.failures.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::BufReader::new(stdout), &mut text);
        text
    });
    let mut exceeded = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if let Some(footprint) = crate::harness::footprint(child.id())
            && footprint > options.worker_memory_limit
        {
            exceeded = Some(footprint);
            let _ = child.kill();
            break child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let output = reader.join().unwrap_or_default();
    match output
        .lines()
        .find_map(|line| line.strip_prefix(STATS_PREFIX))
    {
        Some(counts) => {
            let counts: Vec<usize> = counts
                .split_whitespace()
                .filter_map(|count| count.parse().ok())
                .collect();
            if let [checked, mutants, formatted, ran, failures] = counts[..] {
                stats.checked.fetch_add(checked, Ordering::Relaxed);
                stats.mutants.fetch_add(mutants, Ordering::Relaxed);
                stats.formatted.fetch_add(formatted, Ordering::Relaxed);
                stats.ran.fetch_add(ran, Ordering::Relaxed);
                stats.failures.fetch_add(failures, Ordering::Relaxed);
            }
        }
        None => {
            // The worker ended early: a crash, a hang the watchdog reported,
            // or the memory limit. Its seeds are in the reproducers it wrote.
            stats.failures.fetch_add(1, Ordering::Relaxed);
            let reason = match exceeded {
                Some(footprint) => format!("exceeded {} MiB", footprint >> 20),
                None => format!("ended with {status:?}"),
            };
            let name = format!("worker-{}-{first}", options.seed);
            let text = format!(
                "worker for seeds {}..{} {reason}\nrerun: xsh-fuzz shard {} --seed {} --first {first} --iterations {count} --jobs 1\n",
                options.seed + first,
                options.seed + first + count,
                mode_name(options.mode),
                options.seed
            );
            match write_failure(&options.out, &name, "", &text) {
                Ok(path) => eprintln!("FAIL {name}: {reason} -> {}", path.display()),
                Err(error) => eprintln!("FAIL {name}: {reason} ({error})"),
            }
        }
    }
}

fn in_process(options: Options) -> Stats {
    let jobs = options.jobs.max(1);
    let corpus = corpus_files(&options.corpus_root);
    let shared = Arc::new(Shared {
        current: (0..jobs).map(|_| Mutex::new(None)).collect(),
        options,
        stats: Stats::default(),
        next: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        started: Instant::now(),
        corpus,
        log: Mutex::new(()),
    });
    let watchdog = {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || watchdog(&shared))
    };
    let workers: Vec<_> = (0..jobs)
        .map(|worker| {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name(format!("fuzz-{worker}"))
                .stack_size(64 << 20)
                .spawn(move || work(&shared, worker))
                .expect("spawn fuzz worker")
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
    shared.stop.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    let shared = Arc::try_unwrap(shared).ok().expect("workers finished");
    shared.stats
}

/// Checking runs in-process and cannot be interrupted; a check that exceeds
/// the hang limit is reported and ends the campaign.
fn watchdog(shared: &Shared) {
    let limit = (shared.options.timeout * 3).max(Duration::from_secs(30));
    while !shared.stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(250));
        for slot in &shared.current {
            let current = slot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if let Some((started, name, source)) = current
                && started.elapsed() > limit
            {
                shared.report(
                    &name,
                    &source,
                    &format!("frontend hang: checking took over {}s", limit.as_secs()),
                );
                std::process::exit(1);
            }
        }
    }
}

fn work(shared: &Shared, worker: usize) {
    let config = GenConfig::default();
    let mut sandbox = Sandbox::new(shared.options.exe.clone());
    sandbox.timeout = shared.options.timeout;
    let scratch = tempfile::Builder::new()
        .prefix("xsh-fuzz-lint-")
        .tempdir()
        .expect("scratch directory");
    while let Some(index) = shared.take_seed() {
        let seed = shared.options.seed.wrapping_add(index);
        let mode = match shared.options.mode {
            Mode::All if index % 2 == 0 => Mode::Check,
            Mode::All => Mode::Run,
            mode => mode,
        };
        match mode {
            Mode::Run => run_iteration(shared, seed, &config, &sandbox),
            _ => check_iteration(shared, worker, seed, index, &config, scratch.path()),
        }
    }
}

fn guarded<T>(
    shared: &Shared,
    worker: usize,
    name: &str,
    source: &str,
    body: impl FnOnce() -> T,
) -> T {
    *shared.current[worker]
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
        Some((Instant::now(), name.to_string(), source.to_string()));
    let result = body();
    *shared.current[worker]
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    result
}

fn run_iteration(shared: &Shared, seed: u64, config: &GenConfig, sandbox: &Sandbox) {
    let started = Instant::now();
    let generated = generate(seed, config);
    let result = verify_generated(&generated.source, &generated.expected, Some(sandbox));
    shared.stats.ran.fetch_add(1, Ordering::Relaxed);
    shared
        .stats
        .run_time_us
        .fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
    if let Err(failure) = result {
        let (small, failure) = if shared.should_shrink() {
            minimize(&generated, &failure, Some(sandbox), 1500)
        } else {
            (generated, failure)
        };
        shared.report(
            &seed.to_string(),
            &small.source,
            &format!(
                "{} (run mode, seed {seed})\n{}",
                failure.kind(),
                failure.detail()
            ),
        );
    }
}

fn check_iteration(
    shared: &Shared,
    worker: usize,
    seed: u64,
    index: u64,
    config: &GenConfig,
    scratch: &Path,
) {
    let started = Instant::now();
    let generated = generate(seed, config);
    let name = seed.to_string();
    let result = guarded(shared, worker, &name, &generated.source, || {
        verify_generated(&generated.source, &generated.expected, None)
    });
    shared.stats.checked.fetch_add(1, Ordering::Relaxed);
    if let Err(failure) = result {
        let (small, failure) = if shared.should_shrink() {
            minimize(&generated, &failure, None, 1500)
        } else {
            (generated.clone(), failure)
        };
        shared.report(
            &name,
            &small.source,
            &format!(
                "{} (check mode, seed {seed})\n{}",
                failure.kind(),
                failure.detail()
            ),
        );
        return;
    }
    if shared.options.format_every > 0 && index.is_multiple_of(shared.options.format_every) {
        let outcome = guarded(
            shared,
            worker,
            &format!("{seed}-format"),
            &generated.source,
            || {
                catch_unwind(AssertUnwindSafe(|| {
                    format_invariants(&generated.source, scratch)
                }))
            },
        );
        shared.stats.formatted.fetch_add(1, Ordering::Relaxed);
        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(failure)) => {
                let (source, failure) = if shared.should_shrink() {
                    minimize_format(&generated, &failure, scratch)
                } else {
                    (generated.source.clone(), failure)
                };
                shared.report(
                    &format!("{seed}-format"),
                    &source,
                    &format!("format (seed {seed})\n{failure}"),
                );
            }
            Err(_) => shared.report(
                &format!("{seed}-format"),
                &generated.source,
                "format: formatter or linter panicked",
            ),
        }
    }
    let mut rng = Rng::new(seed ^ 0x6D75_7461_6E74);
    for mutant_index in 0..shared.options.mutants {
        let (origin, file, text) = if mutant_index % 2 == 0 || shared.corpus.is_empty() {
            (
                "generated".to_string(),
                "program.xsh".to_string(),
                generated.source.clone(),
            )
        } else {
            let path = shared.corpus[rng.below(shared.corpus.len())].clone();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            (path.display().to_string(), MUTANT_FILE.to_string(), text)
        };
        let mut mutant = mutate(&text, &mut rng);
        for _ in 0..rng.below(3) {
            mutant = mutate(&mutant, &mut rng);
        }
        let name = format!("{seed}-mutant-{mutant_index}");
        let outcome = guarded(shared, worker, &name, &mutant, || {
            check_mutant(&file, &mutant)
        });
        shared.stats.mutants.fetch_add(1, Ordering::Relaxed);
        if let Err(failure) = outcome {
            let small = if shared.should_shrink() {
                let head = signature(&failure);
                quiet_panics(|| {
                    shrink_text(
                        &mutant,
                        |candidate| matches!(check_mutant(&file, candidate), Err(found) if signature(&found) == head),
                        400,
                    )
                })
            } else {
                mutant
            };
            shared.report(
                &name,
                &small,
                &format!("frontend defect on a mutant of {origin} (seed {seed})\n{failure}"),
            );
        }
    }
    shared
        .stats
        .check_time_us
        .fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
}
