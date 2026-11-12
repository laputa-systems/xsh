use super::common::*;

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
    let helper = cargo_env!("CARGO_BIN_EXE_xsh-test-helper");
    let source = format!(
        "\
let tty_dir = Path({})
let marker = Path({})
let command = process.command_argv(Path({}), [\"xsh-test-helper\", \"session\", marker.display()])
env ({{XSH_UNIX_TTY_DIR: tty_dir}}) {{
  let child = unix.spawn_with_tty(command, tty: \"tty-test\")?
  var tries = 0
  while ! marker.exists()? and tries < 100 {{
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
    // `unix.kill_all` signals every process with the name, and the helper
    // binary also serves tests running beside this one, so the target runs
    // under a name that only this test process uses.
    let root = temp_path("unix-killall-exact");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create kill_all root");
    let name = format!("xsh-killall-{}", std::process::id());
    let target = root.join(&name);
    std::os::unix::fs::symlink(cargo_env!("CARGO_BIN_EXE_xsh-test-helper"), &target)
        .expect("link helper under a unique name");
    let marker = root.join("ready");
    let mut child = Command::new(&target)
        .arg("ready-sleep")
        .arg(&marker)
        .spawn()
        .expect("spawn sleeping helper");
    wait_for_path(&marker, Duration::from_secs(3), &mut child);

    let output = run_temp_script(
        "unix-killall-exact",
        &format!(
            "\
let result = unix.kill_all({}, signal: \"TERM\")?
print ${{result.matched >= 1}} ${{result.signaled >= 1}}
",
            xsh_string_literal(&name)
        ),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true true\n");
    let status = wait_child_status(&mut child, Duration::from_secs(3));
    assert!(!status.success(), "{status}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unix_pid1_wake_descriptor_preserves_bytes_and_validates_readiness() {
    let output = run_temp_script("pid1-wake-readiness", r#"
unix.pid1_setup([], allow_non_pid1: true)
tempdir dir {
  let fifo = fp"{dir}/wake"
  fs.mkfifo(fifo, 0o600)
  let fd = unix.open_fd(fifo, write: true, nonblock: true)?
  defer unix.close_fd(fd)
  assert unix.wait_pid1_event(timeout: 0ms, wake_fd: fd)?.kind == "timeout"
  assert unix.write_fd(fd, b"wake")? == 4
  assert unix.wait_pid1_event(timeout: 1h, wake_fd: fd)?.kind == "poll"
  assert unix.wait_pid1_event(timeout: 0ms, wake_fd: fd)?.kind == "poll"
  assert unix.read_fd(fd, 4)? == b"wake"
  assert unix.wait_pid1_event(timeout: 0ms, wake_fd: null)?.kind == "timeout"
  assert unix.wait_pid1_event(timeout: 0ms, wake_fd: -1) is Err(_)
  let closed = unix.open_fd(fifo, write: true, nonblock: true)?
  unix.close_fd(closed)
  assert unix.wait_pid1_event(timeout: 0ms, wake_fd: closed) is Err(_)
  let hangup = fp"{dir}/hangup"
  fs.mkfifo(hangup, 0o600)
  let reader = unix.open_fd(hangup, nonblock: true)?
  let writer = unix.open_fd(hangup, write: true, nonblock: true)?
  unix.close_fd(writer)
  assert unix.wait_pid1_event(timeout: 0ms, wake_fd: reader) is Err(_)
  unix.close_fd(reader)
}
print "ready"
"#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ready\n");
}

#[test]
fn unix_pid1_wake_descriptor_preserves_signal_delivery() {
    let ready = temp_path("pid1-wake-signal-ready");
    let _ = std::fs::remove_file(&ready);
    let source = format!(r#"
unix.pid1_setup(["TERM"], allow_non_pid1: true)
tempdir dir {{
  let fifo = fp"{{dir}}/wake"
  fs.mkfifo(fifo, 0o600)
  let fd = unix.open_fd(fifo, write: true, nonblock: true)?
  defer unix.close_fd(fd)
  Path({ready}).write("ready")
  let event = unix.wait_pid1_event(timeout: 1h, wake_fd: fd)?
  assert event.kind == "signal"
  assert event.signal == "TERM"
}}
print "signal"
"#, ready = xsh_string_literal(ready.to_str().unwrap()));
    let output = run_cancelable_temp_script("pid1-wake-signal", &source, [], &ready, libc::SIGTERM);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "signal\n");
    let _ = std::fs::remove_file(ready);
}

#[test]
fn unix_pid1_wake_descriptor_preserves_child_reaping() {
    let output = run_temp_script("pid1-wake-child", r#"
unix.pid1_setup([], allow_non_pid1: true)
tempdir dir {
  let fifo = fp"{dir}/wake"
  fs.mkfifo(fifo, 0o600)
  let fd = unix.open_fd(fifo, write: true, nonblock: true)?
  defer unix.close_fd(fd)
  let child = unix.spawn_process_group(process.command_argv("/bin/true", ["true"]))?
  let event = unix.wait_pid1_event(timeout: 2s, wake_fd: fd)?
  assert event.kind == "children"
  assert event.children.len() == 1
  assert event.children[0].pid == child.pid
  assert event.children[0].status.exited_with(0)
}
print "child"
"#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "child\n");
}
