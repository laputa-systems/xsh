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
env XSH_UNIX_TTY_DIR=(tty_dir) {{
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
