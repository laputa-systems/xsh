use super::common::*;

#[test]
fn unix_module_dry_run_primitives_are_observable() {
    let root = temp_path("unix-dry-run");
    let log = root.join("unix.jsonl");
    let source = format!(
        "\
let root = Path({})
let log = fp\"${{root}}/unix.jsonl\"
fs.mkdir(root, parents: true)?
let command = process.command_argv(\"demo\", [\"demo\", \"arg\"])
env XSH_UNIX_DRY_RUN=1 XSH_UNIX_DRY_RUN_SIGNAL=USR1 XSH_UNIX_DRY_RUN_PID=42 XSH_UNIX_UPTIME_SECONDS=17 XSH_UNIX_DRY_RUN_LOG=(log) {{
  let reaped = unix.reap_child_events()?.collect()
  let uptime = unix.uptime_seconds()?
  let tty = unix.tty()?
  let identity = unix.id()?
  let attrs = unix.tty_attrs()?
  unix.set_tty_attrs(attrs)?
  unix.set_hostname(\"xsh\")?
  let child = unix.spawn_process_group(command)?
  let logged_child = unix.spawn_logged_process_group(command, command)?
  let tty_child = unix.spawn_with_tty(command, tty: \"tty1\")?
  unix.kill_process_group(child.pid, \"TERM\")?
  unix.exec(command)?
  print ${{reaped.len()}} ${{uptime}} ${{tty}} ${{identity.groups[0].name}} ${{attrs.raw}} ${{child.pid}} ${{child.new_session}} ${{logged_child.pid}} ${{logged_child.log_pid}} ${{tty_child.pid}} ${{tty_child.new_session}}
}} ?
let log_text = fs.read_text(log)?
print ${{\"set_hostname\" in log_text}} ${{\"spawn_process_group\" in log_text}} ${{\"spawn_logged_process_group\" in log_text}} ${{\"exec\" in log_text}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("unix-dry-run", &source);

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "0 17 /dev/tty root true 1000 false 1001 1002 1003 true\ntrue true true true\n"
    );
    let log_text = std::fs::read_to_string(log).expect("read unix dry-run log");
    assert!(
        log_text.contains("\"op\":\"reap_child_events\""),
        "{log_text}"
    );
    assert!(
        log_text.contains("\"op\":\"kill_process_group\""),
        "{log_text}"
    );
    assert!(log_text.contains("\"op\":\"tty\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"set_tty_attrs\""), "{log_text}");
    assert!(log_text.contains("\"new_session\":\"false\""), "{log_text}");
    assert!(log_text.contains("\"new_session\":\"true\""), "{log_text}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unix_module_dry_run_child_events_are_typed() {
    let output = run_temp_script(
        "unix-dry-run-child-events",
        "\
type ChildEvent = {pid: Int, status: Status}
env XSH_UNIX_DRY_RUN=1 XSH_UNIX_DRY_RUN_EVENT_KIND=child XSH_UNIX_DRY_RUN_PID=42 XSH_UNIX_DRY_RUN_CHILD_PID=43 XSH_UNIX_DRY_RUN_STATUS_KIND=signal XSH_UNIX_DRY_RUN_STATUS_CODE=15 {
  let child_events: List[ChildEvent] = unix.reap_child_events()?.collect()
  print ${child_events[0].pid} ${child_events[0].status.signaled()} ${child_events[0].status.signal_number()?}
} ?
",
    );

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "43 true 15\n");
}

#[test]
fn unix_uptime_seconds_is_real_by_default() {
    let output = run_temp_script(
        "unix-real-uptime",
        "\
let uptime = unix.uptime_seconds()?
print ${uptime >= 0}
",
    );

    if cfg!(any(target_os = "linux", target_os = "macos")) {
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
    } else {
        assert_eq!(output.status.code(), Some(3));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("unix-unsupported"), "{stderr}");
    }
}

#[test]
fn unix_set_hostname_requires_dry_run_or_real_mode() {
    let output = run_temp_script(
        "unix-set-hostname-gated",
        "\
match unix.set_hostname(\"xsh\") {
  Err(e) => {
    test.error_kind(e, \"unix-real-required\")?
    print \"unix-real-required\"
  }
}
",
    );

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "unix-real-required\n"
    );
}

#[test]
fn unix_exec_replaces_child_xsh_process() {
    let helper = cargo_env!("CARGO_BIN_EXE_xsh-test-show-argv");
    let source = format!(
        "\
let command = process.command_argv(Path({}), [\"show-argv\", \"ok\"])
unix.exec(command)?
print \"not-reached\"
",
        xsh_string_literal(helper)
    );

    let output = run_temp_script("unix-exec", &source);

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "6f6b\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_reap_child_events_reports_exit_status() {
    let source = "\
type ChildEvent = {pid: Int, status: Status}
let command = process.command_argv(\"false\", [\"false\"])
let child = unix.spawn_process_group(command)?
var events: List[ChildEvent] = []
var tries = 0
while events.len() == 0 and tries < 100 {
  time.sleep(10ms)?
  events = unix.reap_child_events()?.collect()
  tries += 1
}
print ${events[0].pid == child.pid} ${events[0].status.exited_with(1)}
";

    let output = run_temp_script("unix-reap-child-exit-status", source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true true\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_reap_child_events_reports_signal_status() {
    let marker = temp_path("unix-child-signal-ready");
    let _ = std::fs::remove_file(&marker);
    let sleeper = cargo_env!("CARGO_BIN_EXE_xsh-test-sleeper");
    let source = format!(
        "\
type ChildEvent = {{pid: Int, status: Status}}
let marker = Path({})
let term = process.signal(\"TERM\")?
let command = process.command_argv(Path({}), [\"sleeper\", marker.display()])
let child = unix.spawn_process_group(command)?
var ready_tries = 0
while ! fs.exists(marker)? and ready_tries < 100 {{
  time.sleep(10ms)?
  ready_tries += 1
}}
unix.kill_process_group(child.pid, \"TERM\")?
var events: List[ChildEvent] = []
var tries = 0
while events.len() == 0 and tries < 100 {{
  time.sleep(10ms)?
  events = unix.reap_child_events()?.collect()
  tries += 1
}}
print ${{events[0].pid == child.pid}} ${{events[0].status.signaled()}} ${{events[0].status.signal_number()? == term.number}}
",
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(sleeper)
    );

    let output = run_temp_script("unix-reap-child-signal-status", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true\n"
    );
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_spawn_process_group_can_be_signaled_without_killing_parent() {
    let marker = temp_path("unix-process-group-ready");
    let _ = std::fs::remove_file(&marker);
    let sleeper = cargo_env!("CARGO_BIN_EXE_xsh-test-sleeper");
    let source = format!(
        "\
let marker = Path({})
let command = process.command_argv(Path({}), [\"sleeper\", marker.display()])
let child = unix.spawn_process_group(command)?
var tries = 0
while ! fs.exists(marker)? and tries < 100 {{
  time.sleep(10ms)?
  tries += 1
}}
unix.kill_process_group(child.pid, \"TERM\")?
time.sleep(50ms)?
let reaped = unix.reap_child_events()?.collect()
match process.kill(child.pid, signal: \"0\") {{
  Err(e) => {{
    test.error_kind(e, \"process-missing\")?
    print ${{child.detach}} ${{child.new_session}} ${{child.ignore_hup}} ${{fs.exists(marker)?}} ${{reaped.len() >= 0}} \"process-missing\"
  }}
}}
",
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(sleeper)
    );

    let output = run_temp_script("unix-process-group", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true false true true true process-missing\n"
    );
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_spawn_logged_process_group_pipes_stdout_and_stderr() {
    let root = temp_path("unix-logged-process-group");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create log root");
    let log = root.join("service.log");
    let source = format!(
        "\
let log = Path({})
let command = process.command_argv(\"sh\", [\"sh\", \"-c\", \"printf service-out; printf service-err >&2\"])
let logger = process.command_argv(\"sh\", [\"sh\", \"-c\", f\"cat > ${{log.display()}}\"] )
let child = unix.spawn_logged_process_group(command, logger)?
var events: List[Record] = []
var tries = 0
while events.len() < 2 and tries < 100 {{
  time.sleep(10ms)?
  events = events.extend(unix.reap_child_events()?.collect())
  tries += 1
}}
let log_text = fs.read_text(log)?
print ${{child.pid > 0}} ${{child.log_pid > 0}} ${{\"service-out\" in log_text}} ${{\"service-err\" in log_text}}
",
        xsh_string_literal(log.to_str().unwrap())
    );

    let output = run_temp_script("unix-logged-process-group", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true true\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_spawn_with_tty_uses_tty_dir_and_new_session() {
    let root = temp_path("unix-spawn-tty");
    let tty_dir = root.join("tty");
    let marker = root.join("session");
    let tty_file = tty_dir.join("tty-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&tty_dir).expect("create tty dir");
    std::fs::write(&tty_file, "").expect("create tty file");
    let helper = cargo_env!("CARGO_BIN_EXE_xsh-test-session");
    let source = format!(
        "\
let tty_dir = Path({})
let marker = Path({})
let command = process.command_argv(Path({}), [\"session\", marker.display()])
env XSH_UNIX_TTY_DIR=(tty_dir) {{
  let child = unix.spawn_with_tty(command, tty: \"tty-test\")?
  var tries = 0
  while ! fs.exists(marker)? and tries < 100 {{
    time.sleep(10ms)?
    tries += 1
  }}
  let _reaped = unix.reap_child_events()?
  print ${{child.detach}} ${{child.new_session}} ${{child.ignore_hup}}
}} ?
",
        xsh_string_literal(tty_dir.to_str().unwrap()),
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(helper)
    );

    let output = run_temp_script("unix-spawn-tty", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true\n"
    );
    let session_text = std::fs::read_to_string(&marker).expect("read session marker");
    let fields = session_text.split_whitespace().collect::<Vec<_>>();
    assert_eq!(fields.len(), 2, "{session_text:?}");
    assert_eq!(fields[0], fields[1], "{session_text:?}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unix_kill_all_signals_exact_process_name() {
    let marker = temp_path("unix-killall-ready");
    let _ = std::fs::remove_file(&marker);
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh-test-sleeper"))
        .arg(&marker)
        .spawn()
        .expect("spawn sleeper helper");
    wait_for_path(&marker, Duration::from_secs(3), &mut child);

    let output = run_temp_script(
        "unix-killall-exact",
        "\
let result = unix.kill_all(\"xsh-test-sleeper\", signal: \"TERM\")?
print ${result.matched >= 1} ${result.signaled >= 1}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true true\n");
    let status = wait_child_status(&mut child, Duration::from_secs(3));
    assert!(!status.success(), "{status}");
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_kill_all_does_not_match_wrapper_shell_argv() {
    let root = temp_path("unix-killall-wrapper");
    let script = root.join("wrapper-killall-target");
    let marker = root.join("ready");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create wrapper root");
    std::fs::write(
        &script,
        "\
#!/bin/sh
printf ready > \"$1\"
sleep 30 &
child=$!
trap 'kill \"$child\" 2>/dev/null || true; wait \"$child\" 2>/dev/null || true' EXIT TERM INT
wait \"$child\"
",
    )
    .expect("write wrapper script");
    let mut child = Command::new("sh")
        .arg(&script)
        .arg(&marker)
        .spawn()
        .expect("spawn wrapper shell");
    wait_for_path(&marker, Duration::from_secs(3), &mut child);

    let output = run_temp_script(
        "unix-killall-wrapper",
        "\
match unix.kill_all(\"wrapper-killall-target\") {
  Err(e) => {
    test.error_kind(e, \"process-missing\")?
    print \"process-missing\"
  }
}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "process-missing\n"
    );
    assert!(child.try_wait().expect("poll wrapper").is_none());
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn core_pstree_prints_spawned_parent_before_child() {
    let mut parent = Command::new("sh")
        .args(["-c", "sleep 10 & wait"])
        .spawn()
        .expect("spawn parent shell");
    let parent_pid = parent.id();
    let parent_pid_arg = parent_pid.to_string();

    let mut output = None;
    for _ in 0..40 {
        let candidate = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
            .args(["core/pstree.xsh", "--", "-p", &parent_pid_arg])
            .output()
            .expect("run core pstree");
        if candidate.status.success()
            && pstree_parent_child_order(&String::from_utf8_lossy(&candidate.stdout), parent_pid)
                .is_some()
        {
            output = Some(candidate);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = parent.kill();
    let _ = parent.wait();
    let output = output.expect("pstree output with parent and child");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let (parent_line, child_line) =
        pstree_parent_child_order(&stdout, parent_pid).expect("parent and child rows");
    assert!(parent_line < child_line, "{stdout}");
}
