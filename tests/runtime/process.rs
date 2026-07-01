use super::common::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn os_probe() -> &'static str {
    env!("CARGO_BIN_EXE_xsh-test-os-probe")
}

#[test]
fn process_list_includes_current_process_and_typed_fields() {
    let output = run_temp_script(
        "process-list-fields",
        "\
let typed = process.list()
|> where {
  .pid > 0 and .parent_pid >= 0 and .argv0 != \"\" and .uid >= 0 and .start_time_ms > 0 and .runtime_seconds >= 0
}
|> count()
let current = process.list()
|> where {
  \"xsh\" in .command or \"xsh\" in .argv
}
|> count()
print ${typed > 0} ${current > 0}
",
    );

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true true\n");
}

#[test]
fn process_threads_include_current_process_and_typed_fields() {
    let output = run_temp_script(
        "process-threads-fields",
        "\
let typed = process.threads()
|> where {
  .pid > 0 and .owner_pid > 0 and .thread_id > 0 and .parent_pid >= 0 and .argv0 != \"\" and .uid >= 0 and .start_time_ms > 0 and .runtime_seconds >= 0
}
|> count()
let current = process.threads()
|> where {
  \"xsh\" in .command or \"xsh\" in .argv
}
|> count()
print ${typed > 0} ${current > 0}
",
    );

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true true\n");
}

#[test]
fn process_time_system_user_group_modules_execute() {
    let output = run_temp_script(
        "process-time-system-user-group",
        "\
let sh_path = process.which(\"sh\")?
env PATH=\"/bin:/usr/bin\" {
  match process.which(\"xsh-definitely-missing-command\") {
    Err(e) => print ${sh_path.name == \"sh\"} ${e.kind}
  }
}
let term = process.signal(\"TERM\")?
let command = process.command {
  run true
}
let measured = time.measure(command)?
time.sleep(1ms)?
let host = system.hostname()?
let os = system.uname()?
let me = user.current()?
let same_user = user.by_uid(me.uid)?
let same_group = group.by_gid(me.gid)?
print ${term.number > 0} ${time.format(0, \"%Y\", utc: true)?} ${measured.status.ok} ${measured.duration_ms >= 0}
print ${host != \"\"} ${os.sysname != \"\"} ${same_user.uid == me.uid} ${same_group.gid == me.gid} ${me.name != \"\"}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true not-found\ntrue 1970 true true\ntrue true true true true\n"
    );
}

#[test]
fn user_group_file_editing_uses_configured_databases() {
    let root = temp_path("user-group-edit");
    let passwd = root.join("passwd");
    let shadow = root.join("shadow");
    let group = root.join("group");
    std::fs::create_dir_all(&root).expect("create temp root");
    std::fs::write(&passwd, "root:x:0:0:root:/root:/bin/sh\n").expect("write passwd");
    std::fs::write(&shadow, "root:*:0:0:99999:7:::\n").expect("write shadow");
    std::fs::write(&group, "root:x:0:\n").expect("write group");
    let script = write_temp_script(
        "user-group-edit",
        "\
let added_group = group.add(\"builders\", gid: 2000)?
let added_user = user.add(\"builder\", uid: 2000, gid: added_group.gid, home: /tmp/builder, shell: /bin/sh, gecos: \"Builder\")?
group.remove(\"builders\")?
user.remove(\"builder\")?
print ${added_group.gid} ${added_user.uid} ${added_user.gid} ${added_user.home.name()}
",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&script)
        .env("XSH_PASSWD_FILE", &passwd)
        .env("XSH_SHADOW_FILE", &shadow)
        .env("XSH_GROUP_FILE", &group)
        .output()
        .expect("run xsh");

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "2000 2000 2000 builder\n"
    );
    let passwd_text = std::fs::read_to_string(&passwd).expect("read passwd");
    let group_text = std::fs::read_to_string(&group).expect("read group");
    assert!(!passwd_text.contains("builder"));
    assert!(!group_text.contains("builders"));
    let _ = std::fs::remove_file(script);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn process_argv_words_command_argv_and_run_execute() {
    let output = run_temp_script(
        "process-argv-command-run",
        &format!(
            "\
let show_argv = Path({})
let show_env = Path({})
let words = process.argv_words(\"show ignored 'two words' escaped\\\\ space\")?
let command = process.command_argv(show_argv, words)
let status = process.run(command)?
let env_command = process.command_argv(show_env, [\"show_env\", \"XSH_PLAN\"], Path(\".\"), {{XSH_PLAN: \"ready\"}})
let env_status = process.run(env_command)?
let false_status = process.run(process.command_argv(\"false\", [\"false\"]))?
print ${{status.ok}} ${{env_status.ok}} ${{false_status.exited_with(1)}}
",
            xsh_string_literal(env!("CARGO_BIN_EXE_xsh-test-show-argv")),
            xsh_string_literal(env!("CARGO_BIN_EXE_xsh-test-show-env")),
        ),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "69676e6f726564\n74776f20776f726473\n65736361706564207370616365\nXSH_PLAN=7265616479\ntrue true true\n"
    );
}

#[test]
fn process_errors_match_nominal_timeout_variant() {
    let output = run_temp_script(
        "process-error-timeout-pattern",
        "\
let command = process.command_argv(\"sh\", [\"sh\", \"-c\", \"sleep 1\"], timeout: 10ms)
match process.run(command) {
  Err(ProcessError.Timeout { message }) => print ${\"timed out\" in message}
  Err(is Timeout) => print \"facet\"
  Err(_) => print \"other\"
  Ok(_) => print \"ok\"
}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
}

#[test]
fn spawn_wait_returns_status_data() {
    let output = run_temp_script(
        "spawn-wait-status",
        "\
let ok = spawn run true ?
let ok_status = wait ok?
let bad = spawn run false ?
let bad_status = wait bad?
print ${ok_status.ok} ${bad_status.ok} ${bad_status.exited()} ${bad_status.exit_code()?}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true false true 1\n"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn spawn_and_command_plan_cpumax_use_fake_cgroup_scope() {
    let root = temp_path("spawn-cpumax-cgroup");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fake cgroup root");
    let script = write_temp_script(
        "spawn-cpumax-cgroup",
        "\
let first = spawn run --cpumax=80 true ?
let first_status = wait first?
let command = process.command {
  cpu_max = 80
  run true
}
let second = spawn command?
let second_status = wait second?
print ${first_status.ok} ${second_status.ok}
",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&script)
        .env("XSH_CGROUP_ROOT", &root)
        .output()
        .expect("run xsh");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true true\n");
    let entries = std::fs::read_dir(&root)
        .expect("read fake cgroup root")
        .collect::<Result<Vec<_>, _>>()
        .expect("read cgroup entries");
    assert!(entries.is_empty(), "{entries:?}");
    let _ = std::fs::remove_file(script);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn process_handle_fields_are_stable_metadata() {
    let output = run_temp_script(
        "spawn-handle-fields",
        "\
let h = spawn run true ?
let status = wait h?
print ${h.pid > 0} ${h.command == \"true\"} ${h.argv[0] == \"true\"} ${h.detached == false} ${status.ok}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true true true\n"
    );
}

#[test]
fn spawn_wait_list_preserves_order_and_rejects_duplicates() {
    let output = run_temp_script(
        "spawn-wait-list",
        "\
let first = spawn run false ?
let second = spawn run true ?
let statuses = wait [first, second]?
print ${statuses[0].exit_code()?} ${statuses[1].exit_code()?}
let dup = spawn run true ?
match wait [dup, dup] {
  Err(ProcessError.Unknown { message }) => print ${\"already requested\" in message}
  Err(_) => print \"wrong-error\"
  Ok(_) => print \"ok\"
}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1 0\ntrue\n");
}

#[test]
fn process_handle_alias_double_wait_is_invalid() {
    let output = run_temp_script(
        "spawn-double-wait",
        "\
let h = spawn run true ?
let alias = h
let _status = wait h?
match wait alias {
  Err(ProcessError.Unknown { message }) => print ${\"no longer live\" in message}
  Err(_) => print \"wrong-error\"
  Ok(_) => print \"ok\"
}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
}

#[test]
fn process_handle_cancel_stops_child() {
    let marker = temp_path("spawn-cancel-marker");
    let _ = std::fs::remove_file(&marker);
    let script = format!(
        "\
let marker = Path({})
let command = process.command_argv(\"sh\", [\"sh\", \"-c\", {}])
let h = spawn command?
h.cancel(kill_after: 0ms)?
time.sleep(50ms)?
print ${{marker.exists()? == false}}
",
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(&format!("sleep 1; touch {}", marker.display()))
    );
    let output = run_temp_script("spawn-cancel", &script);
    let _ = std::fs::remove_file(&marker);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
}

#[test]
fn spawn_trace_raw_text_includes_wait_and_cancel_events() {
    let output = run_temp_script_with_args(
        "spawn-trace-text",
        "\
let h = spawn run sh -c \"exit 7\" ?
let status = wait h?
let c = spawn run sleep 1 ?
c.cancel(signal: \"TERM\", kill_after: 0ms)?
print ${status.exit_code()?}
",
        ["--trace", "--raw"],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "7\n");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("kind=spawn.start"));
    assert!(stderr.contains("kind=spawn.ready"));
    assert!(stderr.contains("kind=wait.start"));
    assert!(stderr.contains("kind=wait.end"));
    assert!(stderr.contains("kind=spawn.cancel"));
    assert!(stderr.contains("argv=["));
    assert!(stderr.contains("b\"exit 7\""));
    assert!(stderr.contains("status={kind:exit success:false code:7}"));
    assert!(stderr.contains("signal=b\"TERM\""));
    assert!(stderr.contains("handle_id=1"));
    assert!(stderr.contains("handle_id=2"));
}

#[test]
fn spawn_trace_json_correlates_handle_ids() {
    let output = run_temp_script_with_args(
        "spawn-trace-json",
        "\
let h = spawn run sh -c \"exit 7\" ?
let status = wait h?
let c = spawn run sleep 1 ?
c.cancel(signal: \"TERM\", kill_after: 0ms)?
print ${status.exit_code()?}
",
        ["--trace", "--raw", "--trace-format", "jsonl"],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "7\n");
    let stderr = String::from_utf8(output.stderr).unwrap();
    let values = json_parse_lines(&stderr);
    let first_spawn = values
        .iter()
        .find(|value| json_str(json_field(value, "kind")) == "spawn.start")
        .expect("spawn.start event");
    let argv = json_array(json_field(json_field(first_spawn, "payload"), "argv"));
    assert!(
        argv.iter()
            .any(|arg| json_str(json_field(arg, "display")) == "-c")
    );
    assert!(
        argv.iter()
            .any(|arg| json_str(json_field(arg, "display")) == "exit 7")
    );

    let ready_ids = values
        .iter()
        .filter(|value| json_str(json_field(value, "kind")) == "spawn.ready")
        .map(|value| json_u64(json_field(json_field(value, "payload"), "handle_id")))
        .collect::<Vec<_>>();
    assert_eq!(ready_ids, vec![1, 2]);

    let wait_end = values
        .iter()
        .find(|value| json_str(json_field(value, "kind")) == "wait.end")
        .expect("wait.end event");
    let wait_payload = json_field(wait_end, "payload");
    assert_eq!(json_u64(json_field(wait_payload, "handle_id")), 1);
    assert_eq!(
        json_str(json_field(json_field(wait_payload, "status"), "kind")),
        "exit"
    );
    assert_eq!(
        json_u64(json_field(json_field(wait_payload, "status"), "code")),
        7
    );

    let cancel = values
        .iter()
        .find(|value| json_str(json_field(value, "kind")) == "spawn.cancel")
        .expect("spawn.cancel event");
    let cancel_payload = json_field(cancel, "payload");
    assert_eq!(json_u64(json_field(cancel_payload, "handle_id")), 2);
    assert_eq!(json_str(json_field(cancel_payload, "signal")), "TERM");
}

#[test]
fn spawn_reports_setup_errors_as_process_errors() {
    let output = run_temp_script(
        "spawn-setup-errors",
        "\
env PATH=\"/bin:/usr/bin\" {
  match spawn run xsh-definitely-missing-command {
    Err(ProcessError.NotFound { message }) => print ${\"not found\" in message}
    Err(_) => print \"wrong-error\"
    Ok(_) => print \"ok\"
  }
}
match spawn run true > /definitely/missing/xsh-spawn-output {
  Err(ProcessError.Redirection { message }) => print ${message != \"\"}
  Err(_) => print \"wrong-error\"
  Ok(_) => print \"ok\"
}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\ntrue\n");
}

#[test]
fn spawn_timeout_is_measured_from_spawn_time() {
    let output = run_temp_script(
        "spawn-timeout",
        "\
let command = process.command_argv(\"sh\", [\"sh\", \"-c\", \"sleep 1\"], timeout: 10ms)
let h = spawn command?
time.sleep(50ms)?
match wait h {
  Err(ProcessError.Timeout { message }) => print ${\"timed out\" in message}
  Err(_) => print \"wrong-error\"
  Ok(_) => print \"ok\"
}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
}

#[test]
fn process_handle_returned_from_proc_remains_live() {
    let output = run_temp_script(
        "spawn-return-transfer",
        "\
proc start() [process, error] -> Result[ProcessHandle] {
  return spawn run true ?
}

proc start_record() [process, error] -> Result[Record] {
  let h = spawn run true ?
  return {h}
}

proc start_ok() [process, error] -> Result[ProcessHandle] {
  let h = spawn run true ?
  return Ok(h)
}

proc start_list() [process, error] -> Result[List[ProcessHandle]] {
  let h = spawn run true ?
  return [h]
}

let h = start()?
let status = wait h?
let bundle = start_record()?
let record_status = wait bundle.h?
let ok_handle = start_ok()?
let ok_status = wait ok_handle?
let handles = start_list()?
let list_status = wait handles?
print ${status.ok} ${record_status.ok} ${ok_status.ok} ${list_status[0].ok}
",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true true\n"
    );
}

#[test]
fn dropped_non_detached_process_is_canceled_before_defer_runs() {
    let marker = temp_path("spawn-scope-cleanup-marker");
    let _ = std::fs::remove_file(&marker);
    let script = format!(
        "\
proc observe(marker: Path) [fs, time, error] -> Result[Unit] {{
  time.sleep(50ms)?
  print ${{marker.exists()? == false}}
  return Ok()
}}

let marker = Path({})
proc scoped(marker: Path) [process, fs, time, error] -> Result[Unit] {{
  let command = process.command_argv(\"sh\", [\"sh\", \"-c\", {}])
  let h = spawn command?
  defer observe(marker)
  return Ok()
}}
scoped(marker)?
",
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(&format!("sleep 1; touch {}", marker.display()))
    );
    let output = run_temp_script("spawn-scope-cleanup", &script);
    let _ = std::fs::remove_file(&marker);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sigterm_cancels_live_spawned_process_handles() {
    let root = temp_path("spawn-signal-cancel-root");
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let shell = "trap '' TERM; (sleep 2; printf leaked > \"$2\") & printf ready > \"$1\"; wait";
    let source = format!(
        "\
let ready = Path({})
let leaked = Path({})
let command = process.command_argv(\"sh\", [\"sh\", \"-c\", {}, \"sh\", ready.display(), leaked.display()])
let h = spawn command?
while true {{
  time.sleep(50ms)?
}}
",
        xsh_string_literal(ready.to_str().unwrap()),
        xsh_string_literal(leaked.to_str().unwrap()),
        xsh_string_literal(shell)
    );

    let output =
        run_cancelable_temp_script("cancel-live-spawn", &source, [], &ready, libc::SIGTERM);

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("canceled"), "{stderr}");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dropped_detached_process_is_released_to_reaper() {
    let marker = temp_path("spawn-detached-marker");
    let _ = std::fs::remove_file(&marker);
    let script = format!(
        "\
let marker = Path({})
proc scoped() [process, error] -> Result[Unit] {{
  let command = process.command_argv(\"sh\", [\"sh\", \"-c\", {}], detach: true)
  let h = spawn command?
  return Ok()
}}
scoped()?
time.sleep(300ms)?
print ${{marker.exists()?}}
",
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(&format!("sleep 0.1; touch {}", marker.display()))
    );
    let output = run_temp_script("spawn-detached", &script);
    let _ = std::fs::remove_file(&marker);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
}

#[test]
fn process_argv_words_fixture_executes() {
    let output = xsh(["tests/fixtures/runtime/process-argv-words.xsh"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "4 cmd two words double quoted escaped space\nshell syntax character `|` is not accepted\n"
    );
}

#[test]
fn process_port_finds_visible_listener_and_example_prints_table() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().expect("listener addr").port();
    let pid = std::process::id();
    let source = format!(
        "\
type PortProcess = {{
  pid: Int,
  parent_pid: Int,
  command: Str,
  argv: Str,
  argv0: Str,
  user: Str,
  uid: Int,
  protocol: Str,
  local_address: Str,
  local_port: Int,
  local: Str,
  remote_address: Str,
  remote_port: Int,
  remote: Str,
  state: Str,
  fd: Int,
  inode: Int,
}}

let rows: List[PortProcess] = process.port({port})?
|> where .pid == {pid}
let listeners: List[PortProcess] = process.ports()?
|> where .pid == {pid} and .local_port == {port}
let pid_listeners: List[PortProcess] = process.ports({pid})?
|> where .local_port == {port}
let count = rows |> count()
let listener_count = listeners |> count()
let pid_listener_count = pid_listeners |> count()
if count == 0 or listener_count == 0 or pid_listener_count == 0 {{
  print false
}} else {{
  let row = rows[0]
  let listener = listeners[0]
  let pid_listener = pid_listeners[0]
  print ${{row.pid == {pid}}} ${{row.local_port == {port}}} ${{row.local != \"\"}} ${{row.command != \"\"}} ${{row.fd >= 0}} ${{listener.local_port == {port}}} ${{pid_listener.local_port == {port}}}
}}
"
    );
    let output = run_temp_script("process-port", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true true true true true\n"
    );

    let example = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args(["showcase/px.xsh", "--", "-p", &port.to_string()])
        .output()
        .expect("run px showcase");
    assert!(
        example.status.success(),
        "{}",
        String::from_utf8_lossy(&example.stderr)
    );
    let stdout = String::from_utf8(example.stdout).unwrap();
    assert!(stdout.contains("pid"));
    assert!(stdout.contains("user"));
    assert!(stdout.contains("ports"));
    assert!(stdout.contains(&pid.to_string()));
    assert!(stdout.contains(&port.to_string()));
}

#[test]
fn process_spawn_options_and_kill_are_observable() {
    let marker = temp_path("spawn-ready");
    let source = format!(
        "\
let marker = Path({})
fs.remove(marker, missing_ok: true)?
let command = process.command {{
  detach = true
  new_session = true
  ignore_hup = true
  run sh -c \"printf ready > \\\"$1\\\"; exec sleep 10\" sh (marker)
}}
let spawned = process.spawn(command)?
var tries = 0
while ! fs.exists(marker)? and tries < 100 {{
  time.sleep(10ms)?
  tries += 1
}}
process.kill(spawned.pid, signal: \"TERM\")?
match process.kill(2147483647, signal: \"0\") {{
  Err(e) => print ${{spawned.detach}} ${{spawned.new_session}} ${{spawned.ignore_hup}} ${{fs.exists(marker)?}} ${{e.kind}}
}}
",
        xsh_string_literal(marker.to_str().unwrap())
    );

    let output = run_temp_script("process-spawn-kill", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true true process-missing\n"
    );
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sigterm_cancels_scoped_run_and_process_tree_without_losing_cwd_trace() {
    let root = temp_path("cancel-run-root");
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let source = format!(
        "\
let root = Path({})
let ready = Path({})
let leaked = Path({})
let helper = Path({})
cd (root) {{
  let command = process.command_argv(helper, [\"os-probe\", \"group-leak\", ready.display(), leaked.display()])
  process.run(command)?
}} ?
",
        xsh_string_literal(root.to_str().unwrap()),
        xsh_string_literal(ready.to_str().unwrap()),
        xsh_string_literal(leaked.to_str().unwrap()),
        xsh_string_literal(os_probe())
    );

    let output = run_cancelable_temp_script(
        "cancel-scoped-run",
        &source,
        ["--trace", "--raw"],
        &ready,
        libc::SIGTERM,
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("canceled"));
    assert!(stderr.contains("kind=run.end"));
    assert!(stderr.contains("kind=cwd.exit"));
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sigint_cancels_byte_pipeline_and_process_tree() {
    let root = temp_path("cancel-pipeline-root");
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let output_path = root.join("pipeline.out");
    let source = format!(
        "\
let helper = Path({})
let ready = Path({})
let leaked = Path({})
let output = Path({})
run ${{helper}} group-leak ${{ready}} ${{leaked}} | run cat > (output) ?
",
        xsh_string_literal(os_probe()),
        xsh_string_literal(ready.to_str().unwrap()),
        xsh_string_literal(leaked.to_str().unwrap()),
        xsh_string_literal(output_path.to_str().unwrap())
    );

    let output = run_cancelable_temp_script(
        "cancel-pipeline",
        &source,
        ["--trace", "--raw"],
        &ready,
        libc::SIGINT,
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("canceled"));
    assert!(stderr.contains("kind=pipeline.segment.end"));
    assert!(stderr.contains("kind=pipeline.exit"));
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_usr1_abort_exits_with_requested_status() {
    let source = "\
on USR1 [] {
  print \"hook\"
  abort(0)
}

run sh -c r\"kill -USR1 $PPID; sleep 1\" ?
print \"after\"
";

    let output = run_temp_script("signal-hook-usr1-abort", source);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hook\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_usr1_default_status_uses_signal_exit_convention() {
    let source = "\
on USR1 [] {
  print \"hook\"
}

run sh -c r\"kill -USR1 $PPID; sleep 1\" ?
print \"after\"
";

    let output = run_temp_script("signal-hook-usr1-default-status", source);

    assert_eq!(output.status.code(), Some(128 + libc::SIGUSR1));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hook\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_trace_records_shutdown_path() {
    let source = "\
on USR1 [] {
  abort(0)
}

run sh -c r\"kill -USR1 $PPID; sleep 1\" ?
";

    let output = run_temp_script_with_args("signal-hook-trace", source, ["--trace", "--raw"]);

    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("kind=signal.received"));
    assert!(stderr.contains("kind=signal.hook.enter"));
    assert!(stderr.contains("kind=signal.hook.exit"));
    assert!(stderr.contains("kind=signal.forward"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_repeated_signal_emits_escalation_once() {
    let source = "\
on USR1 [process, time, error] {
  let _hook_sender = process.spawn(process.command_argv(\"sh\", [\"sh\", \"-c\", r\"sleep 0.05; kill -USR1 $PPID\"]))?
  time.sleep(1s)?
  abort(0)
}

let _outer_sender = process.spawn(process.command_argv(\"sh\", [\"sh\", \"-c\", r\"sleep 0.05; kill -USR1 $PPID\"]))?
time.sleep(5s)?
";

    let output = run_temp_script_with_args("signal-hook-escalation", source, ["--trace", "--raw"]);

    assert_eq!(output.status.code(), Some(128 + libc::SIGUSR1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("kind=signal.received"));
    assert!(stderr.contains("kind=signal.escalate"));
    assert_eq!(stderr.matches("kind=signal.hook.enter").count(), 1);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_interrupts_time_sleep_promptly() {
    let source = "\
on USR1 [] {
  print \"hook\"
  abort(0)
}

let _sender = process.spawn(process.command_argv(\"sh\", [\"sh\", \"-c\", r\"sleep 0.05; kill -USR1 $PPID\"]))?
time.sleep(5s)?
print \"after\"
";
    let started = Instant::now();

    let output = run_temp_script("signal-hook-sleep", source);

    assert_eq!(output.status.code(), Some(0));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "sleep did not observe signal promptly"
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hook\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_local_defers_run_at_hook_exit() {
    let marker = temp_path("signal-hook-local-defer");
    let _ = std::fs::remove_file(&marker);
    let source = format!(
        r#"
let marker = Path({})

on USR1 [fs, error] {{
  defer marker.write("defer")?
  abort(0)
}}

run sh -c r"kill -USR1 $PPID; sleep 1" ?
"#,
        xsh_string_literal(marker.to_str().unwrap())
    );

    let output = run_temp_script("signal-hook-local-defer", &source);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "defer");
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_force_abort_skips_hook_and_outer_defers() {
    let hook_marker = temp_path("signal-hook-force-hook-defer");
    let outer_marker = temp_path("signal-hook-force-outer-defer");
    let _ = std::fs::remove_file(&hook_marker);
    let _ = std::fs::remove_file(&outer_marker);
    let source = format!(
        r#"
let hook_marker = Path({})
let outer_marker = Path({})

on USR1 [fs, error] {{
  defer hook_marker.write("hook")
  abort(0, force: true)
}}

defer outer_marker.write("outer")
run sh -c r"kill -USR1 $PPID; sleep 1" ?
"#,
        xsh_string_literal(hook_marker.to_str().unwrap()),
        xsh_string_literal(outer_marker.to_str().unwrap())
    );

    let output = run_temp_script("signal-hook-force-abort", &source);

    assert_eq!(output.status.code(), Some(0));
    assert!(!hook_marker.exists());
    assert!(!outer_marker.exists());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_runs_during_outer_defer_then_cleanup_resumes() {
    let hook_marker = temp_path("signal-hook-outer-defer-hook");
    let cleanup_marker = temp_path("signal-hook-outer-defer-cleanup");
    let _ = std::fs::remove_file(&hook_marker);
    let _ = std::fs::remove_file(&cleanup_marker);
    let source = format!(
        r#"
let hook_marker = Path({})
let cleanup_marker = Path({})

on USR1 [fs, error] {{
  hook_marker.write("hook")?
  abort(0)
}}

defer cleanup_marker.write("cleanup")?
defer time.sleep(300ms)?
let _sender = process.spawn(process.command_argv("sh", ["sh", "-c", r"sleep 0.05; kill -USR1 $PPID"]))?
"#,
        xsh_string_literal(hook_marker.to_str().unwrap()),
        xsh_string_literal(cleanup_marker.to_str().unwrap())
    );

    let output = run_temp_script("signal-hook-outer-defer", &source);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&hook_marker).unwrap(), "hook");
    assert_eq!(std::fs::read_to_string(&cleanup_marker).unwrap(), "cleanup");
    let _ = std::fs::remove_file(hook_marker);
    let _ = std::fs::remove_file(cleanup_marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_owned_process_work_ignores_primary_signal() {
    let source = "\
on USR1 [process, error] {
  run sh -c \"printf hook\" ?
  abort(0)
}

run sh -c r\"kill -USR1 $PPID; sleep 1\" ?
";

    let output = run_temp_script("signal-hook-process-work", source);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hook");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_pre_cancel_forwards_to_active_child_before_hook_finishes() {
    let marker = temp_path("signal-hook-pre-cancel-forwarded");
    let _ = std::fs::remove_file(&marker);
    let source = format!(
        r#"
let marker = Path({})

on USR1 --pre-cancel=0ms [time, error] {{
  time.sleep(300ms)?
  abort(0)
}}

let command = process.command_argv("sh", ["sh", "-c", r"trap 'printf forwarded > $1; exit 0' USR1; kill -USR1 $PPID; while :; do sleep 1; done", "sh", marker.display()])
process.run(command)?
"#,
        xsh_string_literal(marker.to_str().unwrap())
    );

    let output = run_temp_script("signal-hook-pre-cancel", &source);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "forwarded");
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_abort_status_survives_time_measure_child_cancellation() {
    let source = "\
on USR1 [] {
  abort(0)
}

let command = process.command_argv(\"sh\", [\"sh\", \"-c\", r\"kill -USR1 $PPID; sleep 1\"])
time.measure(command)?
print \"after\"
";

    let output = run_temp_script("signal-hook-time-measure", source);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_failure_does_not_orphan_active_child_processes() {
    let leaked = temp_path("signal-hook-failure-leak");
    let _ = std::fs::remove_file(&leaked);
    let source = format!(
        r#"
let leaked = Path({})

error HookFailed = failed(message: Str)

on USR1 [error] {{
  Err(HookFailed.failed(message: "boom"))?
}}

run sh -c r"trap '' USR1; (sleep 2; printf leaked > $1) & kill -USR1 $PPID; wait" sh (leaked.display()) ?
"#,
        xsh_string_literal(leaked.to_str().unwrap())
    );

    let output = run_temp_script("signal-hook-failure-child", &source);

    assert_eq!(output.status.code(), Some(3));
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists());
    let _ = std::fs::remove_file(leaked);
}
