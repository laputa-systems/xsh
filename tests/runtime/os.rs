use super::common::*;

const OS_FIXTURE_DIR: &str = "tests/fixtures/runtime/os";

fn os_fixture(name: &str) -> PathBuf {
    Path::new(OS_FIXTURE_DIR).join(name)
}

fn os_probe() -> &'static str {
    env!("CARGO_BIN_EXE_xsh-test-os-probe")
}

fn run_os_fixture(name: &str, args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xsh"));
    command.arg(os_fixture(name));
    if !args.is_empty() {
        command.arg("--").args(args);
    }
    command.output().expect("run OS fixture")
}

fn trace_os_fixture(name: &str, args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xsht"));
    command
        .args(["trace", "--raw", "--trace-format", "jsonl"])
        .arg(os_fixture(name));
    if !args.is_empty() {
        command.arg("--").args(args);
    }
    command.output().expect("trace OS fixture")
}

fn spawn_os_fixture(name: &str, args: &[&str]) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xsh"));
    command
        .arg(os_fixture(name))
        .arg("--")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.spawn().expect("spawn OS fixture")
}

fn terminate_process(pid: u32, signal: i32) {
    let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
    assert_eq!(
        result,
        0,
        "kill failed: {}",
        std::io::Error::last_os_error()
    );
}

fn read_child_output(mut child: Child, timeout: Duration) -> std::process::Output {
    let status = wait_child_status(&mut child, timeout);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .expect("child stdout")
        .read_to_end(&mut stdout)
        .expect("read child stdout");
    child
        .stderr
        .take()
        .expect("child stderr")
        .read_to_end(&mut stderr)
        .expect("read child stderr");
    std::process::Output {
        status,
        stdout,
        stderr,
    }
}

fn kill_pid_from_file(path: &Path) {
    let text = std::fs::read_to_string(path).expect("read pid marker");
    let pid = text.trim().parse::<libc::pid_t>().expect("parse pid");
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
}

fn wait_for_marker(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for marker {}", path.display());
}

fn trace_values(output: &std::process::Output) -> Vec<JsonValue> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(json_parse)
        .collect()
}

fn trace_events<'a>(values: &'a [JsonValue], kind: &str) -> Vec<&'a JsonValue> {
    values
        .iter()
        .filter(|value| json_str(json_field(value, "kind")) == kind)
        .collect()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_signal_hook_scope_snapshot_uses_registration_bindings() {
    let marker = temp_path("os-signal-scope-snapshot");
    let _ = std::fs::remove_file(&marker);

    let output = run_os_fixture(
        "signal-scope-snapshot.xsh",
        &[marker.to_str().unwrap(), os_probe()],
    );

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "before");
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_child_signal_disposition_is_reset_before_exec() {
    let marker = temp_path("os-child-signal-reset");
    let _ = std::fs::remove_file(&marker);

    let output = run_os_fixture(
        "signal-child-disposition-reset.xsh",
        &[os_probe(), marker.to_str().unwrap()],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true\n"
    );
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_signal_hooks_run_from_loop_sleep_defer_and_wait_checkpoints() {
    let loop_output = run_os_fixture("signal-checkpoint-loop.xsh", &[os_probe()]);
    assert_eq!(loop_output.status.code(), Some(0), "{loop_output:?}");
    assert_eq!(String::from_utf8(loop_output.stdout).unwrap(), "hook\n");

    let sleep_output = run_os_fixture("signal-checkpoint-sleep.xsh", &[os_probe()]);
    assert_eq!(sleep_output.status.code(), Some(0), "{sleep_output:?}");
    assert_eq!(String::from_utf8(sleep_output.stdout).unwrap(), "hook\n");

    let marker = temp_path("os-signal-defer-checkpoint");
    let _ = std::fs::remove_file(&marker);
    let defer_output = run_os_fixture(
        "signal-checkpoint-defer.xsh",
        &[marker.to_str().unwrap(), os_probe()],
    );
    assert_eq!(defer_output.status.code(), Some(0), "{defer_output:?}");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "hook");
    let _ = std::fs::remove_file(marker);

    let wait_output = run_os_fixture("signal-checkpoint-wait.xsh", &[os_probe()]);
    assert_eq!(wait_output.status.code(), Some(0), "{wait_output:?}");
    assert_eq!(String::from_utf8(wait_output.stdout).unwrap(), "hook\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "flaky on macOS: signal escalation timing is nondeterministic"
)]
fn os_first_signal_wins_and_different_signal_escalates_without_reentry() {
    let output = trace_os_fixture("signal-first-wins-escalation.xsh", &[os_probe()]);

    assert_eq!(
        output.status.code(),
        Some(128 + libc::SIGUSR1),
        "{output:?}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let values = trace_values(&output);
    assert_eq!(trace_events(&values, "signal.hook.enter").len(), 1);
    let enter = trace_events(&values, "signal.hook.enter")
        .into_iter()
        .next()
        .expect("signal.hook.enter");
    assert_eq!(
        json_str(json_field(json_field(enter, "payload"), "signal_name")),
        "USR1"
    );
    let escalation = trace_events(&values, "signal.escalate")
        .into_iter()
        .next()
        .expect("signal.escalate");
    let escalation_payload = json_field(escalation, "payload");
    assert_eq!(
        json_str(json_field(escalation_payload, "signal_name")),
        "USR1"
    );
    assert_eq!(
        json_str(json_field(escalation_payload, "escalation_signal_name")),
        "USR2"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_hook_owned_wait_ignores_primary_signal_but_escalation_kills_it() {
    let ready = temp_path("os-hook-owned-wait-ready");
    let _ = std::fs::remove_file(&ready);

    let output = trace_os_fixture(
        "signal-hook-owned-wait-escalates.xsh",
        &[os_probe(), ready.to_str().unwrap()],
    );

    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(ready.exists(), "hook-owned child never reached ready state");
    let values = trace_values(&output);
    assert_eq!(trace_events(&values, "signal.hook.enter").len(), 1);
    assert_eq!(trace_events(&values, "signal.escalate").len(), 1);
    let hook_exit = trace_events(&values, "signal.hook.exit")
        .into_iter()
        .next()
        .expect("signal.hook.exit");
    assert_eq!(
        json_str(json_field(
            json_field(json_field(hook_exit, "payload"), "hook_error"),
            "kind"
        )),
        "canceled"
    );
    let _ = std::fs::remove_file(ready);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_signal_hook_failure_trace_payload_is_json() {
    let output = trace_os_fixture("signal-hook-failure.xsh", &[os_probe()]);

    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let values = trace_values(&output);
    let exit = values
        .iter()
        .find(|value| json_str(json_field(value, "kind")) == "signal.hook.exit")
        .expect("signal.hook.exit event");
    let exit_payload = json_field(exit, "payload");
    assert_eq!(json_str(json_field(exit_payload, "signal_name")), "USR1");
    assert_eq!(
        json_str(json_field(json_field(exit_payload, "hook_error"), "kind")),
        "HookFailed.Failed"
    );
    assert_eq!(
        json_str(json_field(
            json_field(exit_payload, "hook_error"),
            "message"
        )),
        "boom"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_byte_pipeline_cancellation_kills_owned_process_group() {
    let root = temp_path("os-pipeline-cancel");
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let output_path = root.join("pipeline.out");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create pipeline cancel root");
    let mut child = spawn_os_fixture(
        "process-cancel-pipeline.xsh",
        &[
            ready.to_str().unwrap(),
            leaked.to_str().unwrap(),
            output_path.to_str().unwrap(),
            os_probe(),
        ],
    );

    wait_for_path(&ready, Duration::from_secs(3), &mut child);
    terminate_process(child.id(), libc::SIGINT);
    let output = read_child_output(child, Duration::from_secs(5));

    assert_eq!(output.status.code(), Some(3), "{output:?}");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists(), "pipeline process-group child leaked");
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_process_run_cancellation_kills_owned_process_group() {
    let root = temp_path("os-process-run-cancel");
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create OS cancel root");
    let mut child = spawn_os_fixture(
        "process-cancel-run.xsh",
        &[
            ready.to_str().unwrap(),
            leaked.to_str().unwrap(),
            os_probe(),
        ],
    );

    wait_for_path(&ready, Duration::from_secs(3), &mut child);
    terminate_process(child.id(), libc::SIGTERM);
    let output = read_child_output(child, Duration::from_secs(5));

    assert_eq!(output.status.code(), Some(3), "{output:?}");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists(), "owned process-group child leaked");
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_nested_proc_scopes_cleanup_multiple_live_handles() {
    let root = temp_path("os-nested-handle-cleanup");
    let ready1 = root.join("ready1");
    let leaked1 = root.join("leaked1");
    let ready2 = root.join("ready2");
    let leaked2 = root.join("leaked2");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create nested handle root");

    let output = run_os_fixture(
        "process-nested-handles.xsh",
        &[
            ready1.to_str().unwrap(),
            leaked1.to_str().unwrap(),
            ready2.to_str().unwrap(),
            leaked2.to_str().unwrap(),
            os_probe(),
        ],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "done\n");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked1.exists(), "outer ProcessHandle child leaked");
    assert!(!leaked2.exists(), "inner ProcessHandle child leaked");
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_spawn_scope_cleanup_kills_live_handle_tree() {
    let root = temp_path("os-spawn-scope-cleanup");
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create OS spawn cleanup root");

    let output = run_os_fixture(
        "process-cancel-spawn-scope.xsh",
        &[
            ready.to_str().unwrap(),
            leaked.to_str().unwrap(),
            os_probe(),
        ],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "done\n");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists(), "scoped ProcessHandle child leaked");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn os_wait_list_drains_after_timeout_and_duplicate_errors() {
    let root = temp_path("os-wait-list-drain");
    let slow_ready = root.join("slow-ready");
    let fast_marker = root.join("fast-marker");
    let dup_marker = root.join("dup-marker");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create wait-list root");

    let output = run_os_fixture(
        "process-wait-list-drain.xsh",
        &[
            os_probe(),
            slow_ready.to_str().unwrap(),
            fast_marker.to_str().unwrap(),
            dup_marker.to_str().unwrap(),
        ],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true\ntrue\ntrue true\ntrue\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn os_detached_process_is_released_to_background_reaper() {
    let marker = temp_path("os-detached-release");
    let _ = std::fs::remove_file(&marker);

    let output = run_os_fixture(
        "process-detached-release.xsh",
        &[marker.to_str().unwrap(), os_probe()],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "done\n");
    wait_for_marker(&marker, Duration::from_secs(3));
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_process_group_boundary_leaves_new_session_to_harness_cleanup() {
    let root = temp_path("os-process-group-boundary");
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create OS group boundary root");
    let mut child = spawn_os_fixture(
        "process-group-boundary.xsh",
        &[
            ready.to_str().unwrap(),
            leaked.to_str().unwrap(),
            os_probe(),
        ],
    );

    wait_for_path(&ready, Duration::from_secs(3), &mut child);
    terminate_process(child.id(), libc::SIGTERM);
    let status = wait_child_status(&mut child, Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(900));
    kill_pid_from_file(&ready);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .expect("child stdout")
        .read_to_end(&mut stdout)
        .expect("read child stdout");
    child
        .stderr
        .take()
        .expect("child stderr")
        .read_to_end(&mut stderr)
        .expect("read child stderr");
    let output = std::process::Output {
        status,
        stdout,
        stderr,
    };

    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(
        leaked.exists(),
        "new-session child should be outside XSH ownership"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn os_trace_json_correlates_signal_spawn_wait_and_cancel_payloads() {
    let root = temp_path("os-trace-correlation");
    let ready = root.join("ready");
    let caught = root.join("caught");
    let cancel_ready = root.join("cancel-ready");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create OS trace root");

    let signal_output = trace_os_fixture(
        "trace-correlation.xsh",
        &[
            os_probe(),
            ready.to_str().unwrap(),
            caught.to_str().unwrap(),
        ],
    );
    assert_eq!(signal_output.status.code(), Some(0), "{signal_output:?}");
    let signal_values = trace_values(&signal_output);
    for kind in [
        "signal.received",
        "signal.hook.enter",
        "signal.hook.exit",
        "signal.forward",
        "spawn.ready",
        "wait.end",
    ] {
        assert!(
            signal_values
                .iter()
                .any(|value| json_str(json_field(value, "kind")) == kind),
            "missing {kind}: {signal_values:#?}"
        );
    }
    assert!(
        caught.exists(),
        "forwarded signal did not reach active child"
    );
    let received = trace_events(&signal_values, "signal.received")
        .into_iter()
        .next()
        .expect("signal.received");
    let received_payload = json_field(received, "payload");
    assert_eq!(
        json_str(json_field(received_payload, "signal_name")),
        "USR1"
    );
    assert!(json_bool(json_field(received_payload, "matching_hook")));
    assert_eq!(json_u64(json_field(received_payload, "pre_cancel_ms")), 0);
    let forward = trace_events(&signal_values, "signal.forward")
        .into_iter()
        .next()
        .expect("signal.forward");
    let forward_payload = json_field(forward, "payload");
    assert!(json_bool(json_field(forward_payload, "forwarded")));
    assert_eq!(json_str(json_field(forward_payload, "signal_name")), "USR1");
    let wait_end = trace_events(&signal_values, "wait.end")
        .into_iter()
        .next()
        .expect("wait.end");
    let wait_payload = json_field(wait_end, "payload");
    assert_eq!(json_u64(json_field(wait_payload, "handle_id")), 1);
    assert_eq!(
        json_str(json_field(json_field(wait_payload, "status"), "kind")),
        "exit"
    );

    let cancel_output = trace_os_fixture(
        "trace-spawn-cancel.xsh",
        &[os_probe(), cancel_ready.to_str().unwrap()],
    );
    assert!(cancel_output.status.success(), "{cancel_output:?}");
    let cancel_values = trace_values(&cancel_output);
    let ready_event = cancel_values
        .iter()
        .find(|value| json_str(json_field(value, "kind")) == "spawn.ready")
        .expect("spawn.ready event");
    let cancel_event = cancel_values
        .iter()
        .find(|value| json_str(json_field(value, "kind")) == "spawn.cancel")
        .expect("spawn.cancel event");
    assert_eq!(
        json_u64(json_field(json_field(ready_event, "payload"), "handle_id")),
        json_u64(json_field(json_field(cancel_event, "payload"), "handle_id"))
    );
    assert_eq!(
        json_u64(json_field(json_field(ready_event, "payload"), "pid")),
        json_u64(json_field(json_field(cancel_event, "payload"), "pid"))
    );
    let cancel_payload = json_field(cancel_event, "payload");
    assert_eq!(json_str(json_field(cancel_payload, "signal")), "TERM");
    assert_eq!(json_u64(json_field(cancel_payload, "kill_after_ms")), 0);
    let _ = std::fs::remove_dir_all(root);
}

/*
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[ignore]
fn os_stress_signal_hooks_and_process_cancellation() {
    let repeat = std::env::var("XSH_OS_STRESS_REPEAT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(25);

    for index in 0..repeat {
        let marker = temp_path(&format!("os-stress-signal-{index}"));
        let _ = std::fs::remove_file(&marker);
        let output = run_os_fixture(
            "signal-scope-snapshot.xsh",
            &[marker.to_str().unwrap(), os_probe()],
        );
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "before");
        let _ = std::fs::remove_file(marker);

        let root = temp_path(&format!("os-stress-cancel-{index}"));
        let ready = root.join("ready");
        let leaked = root.join("leaked");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create stress root");
        let mut child = spawn_os_fixture(
            "process-cancel-run.xsh",
            &[
                ready.to_str().unwrap(),
                leaked.to_str().unwrap(),
                os_probe(),
            ],
        );
        wait_for_path(&ready, Duration::from_secs(3), &mut child);
        terminate_process(child.id(), libc::SIGTERM);
        let output = read_child_output(child, Duration::from_secs(5));
        assert_eq!(output.status.code(), Some(3), "{output:?}");
        std::thread::sleep(Duration::from_millis(2300));
        assert!(!leaked.exists(), "owned process-group child leaked");
        let _ = std::fs::remove_dir_all(root);

        let output = trace_os_fixture("signal-first-wins-escalation.xsh", &[os_probe()]);
        assert_eq!(
            output.status.code(),
            Some(128 + libc::SIGUSR1),
            "{output:?}"
        );
    }
}
*/
