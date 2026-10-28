use super::common::*;

const SHUTDOWN_STATUS: i32 = 3;

fn idle_script(ready: &Path, hook: &str) -> String {
    format!(
        "\
{hook}
defer {{ print \"cleanup\" }}
let fifo = Path({ready} + \".fifo\")
fs.mkfifo(fifo, mode: 384)
let reader = unix.open_fd(fifo, nonblock: true)?
let writer = unix.open_fd(fifo, write: true, nonblock: true)?
defer {{ unix.close_fd(reader); unix.close_fd(writer); fifo.remove() }}
Path({ready}).write(\"ready\")?
let seen = unix.poll_fds([{{fd: reader, events: [\"readable\"]}}])?
print \"unreachable ${{seen.len()}}\"
",
        ready = xsh_string_literal(ready.to_str().unwrap()),
    )
}

#[test]
fn poll_fds_sigterm_ends_an_indefinite_wait_and_runs_defers() {
    let ready = temp_path("poll-fds-term-ready");
    let _ = std::fs::remove_file(&ready);
    let started = Instant::now();
    let output = run_cancelable_temp_script(
        "poll-fds-term",
        &idle_script(&ready, ""),
        [],
        &ready,
        libc::SIGTERM,
    );
    assert_eq!(output.status.code(), Some(SHUTDOWN_STATUS), "{:?}", output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "cleanup\n");
    assert!(started.elapsed() < Duration::from_secs(3));
    let _ = std::fs::remove_file(ready);
}

#[test]
fn poll_fds_signal_hook_runs_once_before_the_wait_fails() {
    let ready = temp_path("poll-fds-hook-ready");
    let _ = std::fs::remove_file(&ready);
    let output = run_cancelable_temp_script(
        "poll-fds-hook",
        &idle_script(&ready, "on TERM [io] { print \"hook\" }"),
        [],
        &ready,
        libc::SIGTERM,
    );
    assert_eq!(output.status.code(), Some(SHUTDOWN_STATUS), "{:?}", output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hook\ncleanup\n");
    let _ = std::fs::remove_file(ready);
}

#[test]
fn poll_fds_second_signal_interrupts_a_wait_inside_cleanup() {
    let ready = temp_path("poll-fds-escalate-ready");
    let _ = std::fs::remove_file(&ready);
    let source = format!(
        "\
defer {{
  print \"cleanup\"
  let _ = unix.poll_fds([{{fd: 0, events: [\"readable\"]}}])
}}
Path({}).write(\"ready\")?
let _ = unix.poll_fds([{{fd: 0, events: [\"readable\"]}}])?
",
        xsh_string_literal(ready.to_str().unwrap()),
    );
    let path = write_temp_script("poll-fds-escalate", &source);
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn script");
    wait_for_path(&ready, Duration::from_secs(3), &mut child);
    let pid = child.id() as libc::pid_t;
    assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
    std::thread::sleep(Duration::from_millis(300));
    assert!(child.try_wait().unwrap().is_none(), "cleanup wait must outlive the first signal");
    assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
    let status = wait_child_status(&mut child, Duration::from_secs(5));
    assert_eq!(status.code(), Some(SHUTDOWN_STATUS));
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(ready);
}

#[cfg(target_os = "linux")]
#[test]
fn poll_fds_idle_wait_makes_no_wakeups() {
    let ready = temp_path("poll-fds-idle-ready");
    let _ = std::fs::remove_file(&ready);
    let path = write_temp_script("poll-fds-idle", &idle_script(&ready, ""));
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn script");
    wait_for_path(&ready, Duration::from_secs(3), &mut child);
    let switches = |pid: u32| -> u64 {
        std::fs::read_dir(format!("/proc/{pid}/task"))
            .expect("list threads")
            .map(|task| {
                let status = std::fs::read_to_string(task.expect("task").path().join("status"))
                    .unwrap_or_default();
                status
                    .lines()
                    .find_map(|line| line.strip_prefix("voluntary_ctxt_switches:"))
                    .and_then(|count| count.trim().parse().ok())
                    .unwrap_or(0)
            })
            .sum()
    };
    std::thread::sleep(Duration::from_millis(200));
    let before = switches(child.id());
    std::thread::sleep(Duration::from_secs(2));
    let after = switches(child.id());
    let _ = child.kill();
    let _ = child.wait();
    assert!(after - before <= 1, "idle wait woke {} times in 2 s", after - before);
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(ready);
}
