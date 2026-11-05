use super::common::*;

use std::fs;
use std::os::unix::process::{CommandExt, ExitStatusExt};

const TAG_VARIABLE: &str = "XSH_STORM_TAG";
const EXIT_BOUND: Duration = Duration::from_secs(10);
const STRAGGLER_BOUND: Duration = Duration::from_secs(2);
const FIXED_SEEDS: std::ops::Range<u64> = 0..96;

const PRELUDE: &str = r#"proc mark(dir: Path, name: Str) [fs, error] {
  let n = fs.children(dir)? |> count()
  fp"{dir}/e{n}-{name}".write("")?
}
proc ready(dir: Path) [fs, error] {
  fp"{dir}/ready".write("")?
}
"#;

const HANDLES_AND_PIPELINE: &str = r#"proc main(...argv: List[Str]) [fs, process, time, error] {
  let dir = Path(argv[0])
  defer { mark(dir, "outer")? }
  errdefer { mark(dir, "outer-err")? }
  let a = spawn run sleep 0.6 ?
  let b = spawn run sh -c "sleep 0.6 & wait" ?
  ready(dir)?
  run sleep 0.6 | run cat | run cat
}
"#;

const LIVE_STREAM: &str = r#"proc main(...argv: List[Str]) [fs, process, time, error] {
  let dir = Path(argv[0])
  defer { mark(dir, "outer")? }
  errdefer { mark(dir, "outer-err")? }
  let rows = run.stream --text sh -c "while :; do echo row; sleep 0.01; done"
  ready(dir)?
  var n = 0
  for row in rows {
    n = n + 1
    if n == 60 { break }
  }
}
"#;

const PIPELINE_STATUS: &str = r#"proc main(...argv: List[Str]) [fs, process, time, error] {
  let dir = Path(argv[0])
  defer { mark(dir, "outer")? }
  errdefer { mark(dir, "outer-err")? }
  ready(dir)?
  let failing = run.status sh -c "sleep 0.2; exit 7" | run sh -c "cat >/dev/null"
  let broken_pipe = run.status yes | run head -n 3
  print ${failing.success} ${broken_pipe.success}
}
"#;

const PTY_STDIN: &str = r#"proc main(...argv: List[Str]) [fs, process, time, error] {
  let dir = Path(argv[0])
  let pty = unix.open_pty()?
  defer { unix.close_fd(pty.replica)? }
  defer { unix.close_fd(pty.master)? }
  defer { mark(dir, "outer")? }
  errdefer { mark(dir, "outer-err")? }
  ready(dir)?
  let replica = Path(pty.name)
  run sleep 0.6 < $replica
}
"#;

const SIGNAL_HOOK: &str = r#"let dir = Path(args[0])
on INT --pre-cancel=0ms [fs, error] { mark(dir, "hook")? }
on TERM --pre-cancel=0ms [fs, error] { mark(dir, "hook")? }
defer { mark(dir, "outer")? }
errdefer { mark(dir, "outer-err")? }
ready(dir)?
run sleep 0.6
"#;

const CLOSED_STDOUT: &str = r#"proc main(...argv: List[Str]) [fs, io, process, time, error] {
  let dir = Path(argv[0])
  defer { mark(dir, "outer")? }
  errdefer { mark(dir, "outer-err")? }
  let rows = run.stream --text sh -c "while :; do echo row; sleep 0.005; done"
  ready(dir)?
  var n = 0
  for row in rows {
    n = n + 1
    print $row
    io.flush_stdout()?
    if n == 4000 { break }
  }
}
"#;

const ESCAPED_DESCENDANT: &str = r#"proc main(...argv: List[Str]) [fs, process, time, error] {
  let dir = Path(argv[0])
  ready(dir)?
  run.status sh -c "setsid sleep 30 </dev/null >/dev/null 2>&1 &"
}
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scenario {
    HandlesAndPipeline,
    LiveStream,
    PipelineStatus,
    PtyStdin,
    SignalHook,
    ClosedStdout,
}

const SCENARIOS: [Scenario; 6] = [
    Scenario::HandlesAndPipeline,
    Scenario::LiveStream,
    Scenario::PipelineStatus,
    Scenario::PtyStdin,
    Scenario::SignalHook,
    Scenario::ClosedStdout,
];

impl Scenario {
    fn body(self) -> &'static str {
        match self {
            Self::HandlesAndPipeline => HANDLES_AND_PIPELINE,
            Self::LiveStream => LIVE_STREAM,
            Self::PipelineStatus => PIPELINE_STATUS,
            Self::PtyStdin => PTY_STDIN,
            Self::SignalHook => SIGNAL_HOOK,
            Self::ClosedStdout => CLOSED_STDOUT,
        }
    }

    fn cancelled_sequence(self) -> &'static [&'static str] {
        match self {
            Self::SignalHook => &["hook", "outer-err", "outer"],
            _ => &["outer-err", "outer"],
        }
    }

    fn completed_sequence(self) -> &'static [&'static str] {
        &["outer"]
    }

    fn completed_stdout(self) -> Option<&'static str> {
        match self {
            Self::PipelineStatus => Some("y\ny\ny\nfalse true\n"),
            _ => None,
        }
    }
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

#[derive(Clone, Copy, Debug)]
struct Injection {
    signal: i32,
    after: Duration,
}

#[derive(Clone, Copy, Debug)]
struct Plan {
    seed: u64,
    scenario: Scenario,
    first: Option<Injection>,
    second: Option<Injection>,
    close_stdout_after: Option<Duration>,
}

impl Plan {
    fn from_seed(seed: u64) -> Self {
        let mut rng = SplitMix64(seed);
        let scenario = SCENARIOS[rng.below(SCENARIOS.len() as u64) as usize];
        let micros = |rng: &mut SplitMix64| {
            let ceiling = if rng.below(3) == 0 { 5_000 } else { 350_000 };
            Duration::from_micros(rng.below(ceiling))
        };
        let signal = |rng: &mut SplitMix64| {
            if rng.below(2) == 0 { libc::SIGINT } else { libc::SIGTERM }
        };
        let signalled = rng.below(5) != 0;
        let first = signalled.then(|| Injection {
            signal: signal(&mut rng),
            after: micros(&mut rng),
        });
        let second = (signalled && rng.below(4) == 0).then(|| Injection {
            signal: signal(&mut rng),
            after: Duration::from_micros(rng.below(80_000)),
        });
        let close_stdout_after = (scenario == Scenario::ClosedStdout).then(|| micros(&mut rng));
        let first = if scenario == Scenario::ClosedStdout { None } else { first };
        let second = if first.is_some() { second } else { None };
        Self { seed, scenario, first, second, close_stdout_after }
    }
}

#[derive(Debug)]
struct Outcome {
    exit: std::process::ExitStatus,
    stdout: Vec<u8>,
    ready: bool,
    markers: Vec<String>,
    elapsed: Duration,
}

fn tag_for(seed: u64) -> String {
    format!("{}-{seed}", std::process::id())
}

fn live_tagged_processes(tag: &str) -> Vec<(i32, String)> {
    let needle = format!("{TAG_VARIABLE}={tag}\0").into_bytes();
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        panic!("/proc is not readable");
    };
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<i32>().ok()) else {
            continue;
        };
        let Ok(environ) = fs::read(entry.path().join("environ")) else {
            continue;
        };
        if !environ.windows(needle.len()).any(|window| window == needle.as_slice()) {
            continue;
        }
        let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some(after_name) = stat.rsplit_once(')').map(|(_, rest)| rest.trim_start()) else {
            continue;
        };
        if after_name.starts_with('Z') {
            continue;
        }
        let comm = fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        found.push((pid, comm.trim().to_owned()));
    }
    found
}

fn kill_tagged(tag: &str) {
    for (pid, _) in live_tagged_processes(tag) {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

struct Bystander(Child);

impl Bystander {
    fn start() -> Self {
        let child = Command::new("sleep")
            .arg("120")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("start bystander");
        Self(child)
    }

    fn alive(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }
}

impl Drop for Bystander {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn execute(plan: &Plan, body: &str) -> Outcome {
    let dir = PathBuf::from(format!(".work/tmp/storm-{}-{}", std::process::id(), plan.seed));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create marker directory");
    let dir = dir.canonicalize().expect("canonical marker directory");
    let script = dir.join("scenario.xsh");
    let markers_dir = dir.join("marks");
    fs::create_dir_all(&markers_dir).expect("create marks");
    fs::write(&script, format!("{PRELUDE}{body}")).expect("write scenario");

    let tag = tag_for(plan.seed);
    let started = Instant::now();
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script)
        .arg(&markers_dir)
        .env(TAG_VARIABLE, &tag)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("start xsh");
    let pid = child.id() as libc::pid_t;
    let mut stdout = child.stdout.take().expect("xsh stdout");
    let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();
    let close_after = plan.close_stdout_after;
    let reader = std::thread::spawn(move || {
        let reader_started = Instant::now();
        let mut collected = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            if close_after.is_some_and(|after| reader_started.elapsed() >= after) {
                break;
            }
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(count) => collected.extend_from_slice(&chunk[..count]),
            }
        }
        drop(stdout);
        let _ = sender.send(collected);
    });

    let mut pending: Vec<Injection> = Vec::new();
    if let Some(first) = plan.first {
        pending.push(first);
        if let Some(second) = plan.second {
            pending.push(Injection { signal: second.signal, after: first.after + second.after });
        }
    }
    let exit = loop {
        if let Some(status) = child.try_wait().expect("poll xsh") {
            break status;
        }
        let elapsed = started.elapsed();
        if elapsed > EXIT_BOUND {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            kill_tagged(&tag);
            let status = child.wait().expect("reap killed xsh");
            reader.join().expect("join reader");
            panic!(
                "seed {} {:?} did not exit within {EXIT_BOUND:?} ({status}); plan {plan:?}",
                plan.seed, plan.scenario
            );
        }
        if let Some(position) = pending.iter().position(|injection| elapsed >= injection.after) {
            let injection = pending.remove(position);
            unsafe {
                libc::kill(pid, injection.signal);
            }
        }
        std::thread::sleep(Duration::from_micros(500));
    };
    let elapsed = started.elapsed();
    let stdout = receiver.recv_timeout(EXIT_BOUND).unwrap_or_default();
    let _ = reader.join();

    let ready = markers_dir.join("ready").exists();
    let mut numbered: Vec<(u64, String)> = Vec::new();
    for entry in fs::read_dir(&markers_dir).expect("read marks").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(rest) = name.strip_prefix('e')
            && let Some((index, label)) = rest.split_once('-')
        {
            numbered.push((index.parse().expect("marker index"), label.to_owned()));
        }
    }
    numbered.sort();
    let outcome = Outcome {
        exit,
        stdout,
        ready,
        markers: numbered.into_iter().map(|(_, label)| label).collect(),
        elapsed,
    };
    let _ = fs::remove_dir_all(&dir);
    outcome
}

fn check_cleanup_sequence(
    plan: &Plan,
    exited_cancelled: bool,
    ready: bool,
    markers: &[String],
) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for marker in markers {
        if !seen.insert(marker.as_str()) {
            return Err(format!("cleanup `{marker}` ran more than once: {markers:?}"));
        }
    }
    let expected = if exited_cancelled {
        plan.scenario.cancelled_sequence()
    } else {
        plan.scenario.completed_sequence()
    };
    let escalated = plan.second.is_some() || !ready;
    let matches_prefix = markers.len() <= expected.len()
        && markers.iter().zip(expected).all(|(marker, want)| marker == want);
    if !matches_prefix {
        return Err(format!("cleanup order {markers:?} is not a prefix of {expected:?}"));
    }
    let hook_may_be_absent = plan.scenario == Scenario::SignalHook;
    if !escalated && !hook_may_be_absent && markers.len() != expected.len() {
        return Err(format!("cleanup {markers:?} is incomplete against {expected:?}"));
    }
    if !escalated && hook_may_be_absent && markers.len() + 1 < expected.len() {
        return Err(format!("cleanup {markers:?} is incomplete against {expected:?}"));
    }
    Ok(())
}

fn check_outcome(plan: &Plan, tag: &str, outcome: &Outcome, bystander: &mut Bystander) -> Result<(), String> {
    let cancelled = outcome.exit.code() == Some(3);
    let completed = outcome.exit.code() == Some(0);
    let signalled_before_handlers = outcome.exit.signal().is_some() && !outcome.ready;
    let killed_after_teardown = outcome.ready
        && outcome.exit.signal().is_some_and(|signal| {
            plan.second.is_some_and(|second| second.signal == signal)
        });
    let closed_stdout_failure = plan.scenario == Scenario::ClosedStdout
        && matches!(outcome.exit.code(), Some(1 | 3));
    if !(cancelled || completed || signalled_before_handlers || killed_after_teardown || closed_stdout_failure) {
        return Err(format!(
            "unexpected exit {:?} (signal {:?}, ready {})",
            outcome.exit.code(),
            outcome.exit.signal(),
            outcome.ready
        ));
    }
    if outcome.elapsed > EXIT_BOUND {
        return Err(format!("cancellation took {:?}", outcome.elapsed));
    }

    let deadline = Instant::now() + STRAGGLER_BOUND;
    let mut survivors = live_tagged_processes(tag);
    while !survivors.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        survivors = live_tagged_processes(tag);
    }
    if !survivors.is_empty() {
        kill_tagged(tag);
        return Err(format!("xsh left live processes behind: {survivors:?}"));
    }
    if !bystander.alive() {
        return Err("a bystander outside the run was signalled".to_owned());
    }
    if signalled_before_handlers {
        if outcome.markers.is_empty() {
            return Ok(());
        }
        return Err("a run killed before readiness still ran cleanup".to_owned());
    }
    if !outcome.ready && !cancelled {
        return Err(format!("a script that never became ready exited {:?}", outcome.exit.code()));
    }
    if killed_after_teardown {
        let settled = Plan { second: None, ..*plan };
        return check_cleanup_sequence(&settled, true, true, &outcome.markers);
    }
    check_cleanup_sequence(plan, cancelled || closed_stdout_failure, outcome.ready, &outcome.markers)?;
    if completed
        && plan.close_stdout_after.is_none()
        && let Some(want) = plan.scenario.completed_stdout()
        && String::from_utf8_lossy(&outcome.stdout) != want
    {
        return Err(format!("pipeline output {:?} differs from {want:?}", String::from_utf8_lossy(&outcome.stdout)));
    }
    Ok(())
}

fn run_seed(seed: u64) {
    let plan = Plan::from_seed(seed);
    let mut bystander = Bystander::start();
    let outcome = execute(&plan, plan.scenario.body());
    if let Err(reason) = check_outcome(&plan, &tag_for(seed), &outcome, &mut bystander) {
        panic!("signal storm seed {seed} failed: {reason}\nplan {plan:?}\noutcome {outcome:?}");
    }
}

#[test]
fn signal_storm_leaves_no_orphans_and_runs_cleanup_once_in_order() {
    for seed in FIXED_SEEDS {
        run_seed(seed);
    }
}

#[test]
fn signal_storm_plans_are_a_function_of_the_seed() {
    for seed in FIXED_SEEDS {
        assert_eq!(format!("{:?}", Plan::from_seed(seed)), format!("{:?}", Plan::from_seed(seed)));
    }
    let scenarios: std::collections::BTreeSet<_> = FIXED_SEEDS
        .map(|seed| format!("{:?}", Plan::from_seed(seed).scenario))
        .collect();
    assert_eq!(scenarios.len(), SCENARIOS.len());
}

#[test]
fn signal_storm_oracle_rejects_a_descendant_that_escapes_cleanup() {
    let plan = Plan {
        seed: 9_000_001,
        scenario: Scenario::HandlesAndPipeline,
        first: None,
        second: None,
        close_stdout_after: None,
    };
    let mut bystander = Bystander::start();
    let outcome = execute(&plan, ESCAPED_DESCENDANT);
    let verdict = check_outcome(&plan, &tag_for(plan.seed), &outcome, &mut bystander);
    kill_tagged(&tag_for(plan.seed));
    let reason = verdict.expect_err("an escaped descendant must fail the oracle");
    assert!(reason.contains("live processes"), "{reason}");
}

#[test]
fn signal_storm_oracle_rejects_duplicated_and_reordered_cleanup() {
    let plan = Plan {
        seed: 9_000_002,
        scenario: Scenario::HandlesAndPipeline,
        first: None,
        second: None,
        close_stdout_after: None,
    };
    let owned = |labels: &[&str]| labels.iter().map(|label| (*label).to_owned()).collect::<Vec<_>>();
    let scenario = plan.scenario;
    assert!(check_cleanup_sequence(&plan, true, true, &owned(scenario.cancelled_sequence())).is_ok());
    assert!(check_cleanup_sequence(&plan, true, true, &owned(&["outer", "outer-err"])).is_err());
    assert!(check_cleanup_sequence(&plan, true, true, &owned(&["outer-err", "outer", "outer"])).is_err());
    assert!(check_cleanup_sequence(&plan, true, true, &owned(&["outer-err"])).is_err());
}
