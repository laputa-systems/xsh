use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use xsh_fuzz::driver::{Mode, Options, campaign};
use xsh_fuzz::generator::{GenConfig, generate};
use xsh_fuzz::harness::Sandbox;

const USAGE: &str = "usage: xsh-fuzz [check|run|all] [OPTIONS]
       xsh-fuzz probes
       xsh-fuzz print SEED

modes:
  check   generate well-typed programs (must check), mutate them and corpus
          files (frontend must not panic, hang, or report internal errors),
          and check formatter/lint invariants; nothing executes
  run     execute well-typed generated programs in the sandbox and compare
          stdout with the reference evaluator
  all     alternate check and run (default)

options:
  --seed N          first seed (default: from the clock); iteration i uses N+i
  --iterations N    stop after N programs
  --duration SECS   stop after SECS seconds (default 60 without --iterations)
  --jobs N          worker processes (default: half the CPUs, at most 4)
  --out DIR         failure reproducers (default target/fuzz/failures)
  --timeout SECS    per-program execution timeout (default 10)
  --mutants N       mutants per check iteration (default 4)
  --format-every N  formatter/lint invariants every N check iterations (default 4, 0 = off)
  --shard-size N    iterations per worker process (default 500; 0 = this process)
  --worker-memory M kill a worker process above M MiB (default 384)
  --no-shrink       write failures without minimizing them

Each program runs in its own child, killed above 256 MiB or after
--timeout; each worker process handles one batch and exits, so at most
--jobs workers (plus one program child each) are alive at a time.";

fn fail(message: &str) -> ! {
    eprintln!("xsh-fuzz: {message}\n{USAGE}");
    std::process::exit(2)
}

fn number<T: std::str::FromStr>(args: &mut impl Iterator<Item = String>, flag: &str) -> T {
    args.next().and_then(|text| text.parse().ok()).unwrap_or_else(|| fail(&format!("{flag} needs a number")))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    let mode = match args.peek().map(String::as_str) {
        Some("exec") => {
            args.next();
            let script = args.next().unwrap_or_else(|| fail("exec needs a script"));
            xsh_fuzz::harness::exec_worker(std::path::Path::new(&script));
        }
        Some("print") => {
            args.next();
            let seed = number(&mut args, "print");
            let generated = generate(seed, &GenConfig::default());
            print!("{}", generated.source);
            eprint!("{}", generated.expected);
            return;
        }
        Some("probes") => {
            let sandbox = Sandbox::new(std::env::current_exe().expect("current executable"));
            let started = Instant::now();
            let summary = xsh_fuzz::probes::run_corpus(&xsh_fuzz::probes::corpus(), &sandbox, 40);
            for (probe, failure) in &summary.failures {
                println!("FAIL {}: {}\n  {}", probe.label, probe.call, failure.lines().take(8).collect::<Vec<_>>().join("\n  "));
            }
            println!(
                "{} probes, {} accepted, {} rejected, {} failures in {:.1}s",
                summary.probes,
                summary.accepted,
                summary.rejected,
                summary.failures.len(),
                started.elapsed().as_secs_f64()
            );
            std::process::exit(i32::from(!summary.failures.is_empty()));
        }
        Some("reduce") => {
            args.next();
            let path = args.next().unwrap_or_else(|| fail("reduce needs a file"));
            let text = std::fs::read_to_string(&path).unwrap_or_else(|error| fail(&format!("{path}: {error}")));
            let Err(failure) = xsh_fuzz::mutate::check_mutant(&path, &text) else {
                println!("{path}: no frontend defect to reduce");
                return;
            };
            let head = xsh_fuzz::driver::signature(&failure);
            let reduced = xsh_fuzz::shrink::shrink_text(
                &text,
                |candidate| matches!(xsh_fuzz::mutate::check_mutant(&path, candidate), Err(found) if xsh_fuzz::driver::signature(&found) == head),
                5000,
            );
            print!("{reduced}");
            eprintln!("{failure}");
            return;
        }
        Some("shard") => {
            args.next();
            let mode = match args.next().as_deref() {
                Some("check") => Mode::Check,
                Some("run") => Mode::Run,
                Some("all") => Mode::All,
                _ => fail("shard needs a mode"),
            };
            let mut options = default_options(mode);
            options.jobs = 1;
            options.shard_size = 0;
            parse_options(&mut args, &mut options);
            xsh_fuzz::driver::run_shard(options);
            return;
        }
        Some("check") => Mode::Check,
        Some("run") => Mode::Run,
        Some("all") => Mode::All,
        Some(flag) if flag.starts_with("--") => Mode::All,
        None => Mode::All,
        Some(other) => fail(&format!("unknown mode `{other}`")),
    };
    if args.peek().is_some_and(|arg| !arg.starts_with("--")) {
        args.next();
    }
    let mut options = default_options(mode);
    parse_options(&mut args, &mut options);
    if options.iterations.is_none() && options.duration.is_none() {
        options.duration = Some(Duration::from_secs(60));
    }
    eprintln!(
        "xsh-fuzz: mode {:?}, seed {}, {} worker processes of {} iterations, failures in {}",
        options.mode,
        options.seed,
        options.jobs,
        options.shard_size,
        options.out.display()
    );
    let started = Instant::now();
    let stats = campaign(options);
    let elapsed = started.elapsed().as_secs_f64();
    let checked = stats.checked.load(Ordering::Relaxed);
    let ran = stats.ran.load(Ordering::Relaxed);
    let mutants = stats.mutants.load(Ordering::Relaxed);
    let failures = stats.failures.load(Ordering::Relaxed);
    eprintln!(
        "xsh-fuzz: {checked} checked ({:.0}/s), {mutants} mutants, {} format checks, {ran} run ({:.0}/s), {failures} failures in {elapsed:.1}s",
        checked as f64 / elapsed,
        stats.formatted.load(Ordering::Relaxed),
        ran as f64 / elapsed,
    );
    std::process::exit(i32::from(failures > 0));
}

fn default_options(mode: Mode) -> Options {
    let cpus = std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get);
    Options {
        mode,
        seed: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |elapsed| elapsed.as_secs()),
        iterations: None,
        duration: None,
        jobs: (cpus / 2).clamp(1, 4),
        out: repo_root().join("target/fuzz/failures"),
        shrink: true,
        timeout: Duration::from_secs(10),
        corpus_root: repo_root(),
        mutants: 4,
        format_every: 4,
        exe: std::env::current_exe().expect("current executable"),
        first: 0,
        shard_size: 500,
        worker_memory_limit: 384 << 20,
    }
}

fn parse_options(args: &mut impl Iterator<Item = String>, options: &mut Options) {
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => options.seed = number(args, "--seed"),
            "--iterations" => options.iterations = Some(number(args, "--iterations")),
            "--duration" => options.duration = Some(Duration::from_secs(number(args, "--duration"))),
            "--jobs" => options.jobs = number(args, "--jobs"),
            "--out" => options.out = PathBuf::from(args.next().unwrap_or_else(|| fail("--out needs a directory"))),
            "--timeout" => options.timeout = Duration::from_secs(number(args, "--timeout")),
            "--mutants" => options.mutants = number(args, "--mutants"),
            "--format-every" => options.format_every = number(args, "--format-every"),
            "--no-shrink" => options.shrink = false,
            "--first" => options.first = number(args, "--first"),
            "--shard-size" => options.shard_size = number(args, "--shard-size"),
            "--worker-memory" => options.worker_memory_limit = number::<u64>(args, "--worker-memory") << 20,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => fail(&format!("unknown option `{other}`")),
        }
    }
}
