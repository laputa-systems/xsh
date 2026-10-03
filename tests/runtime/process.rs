use super::common::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn os_probe() -> &'static str {
    cargo_env!("CARGO_BIN_EXE_xsh-test-os-probe")
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
            xsh_string_literal(cargo_env!("CARGO_BIN_EXE_xsh-test-show-argv")),
            xsh_string_literal(cargo_env!("CARGO_BIN_EXE_xsh-test-show-env")),
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
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
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

#[cfg(all(feature = "net", any(target_os = "linux", target_os = "macos")))]
#[test]
fn sigterm_cancels_live_net_jobs_without_a_signal_hook() {
    let root = temp_path("net-job-signal-cancel-root");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create net signal test root");
    let request_started = root.join("request-started");
    let ready = root.join("ready");
    let server = SignalNetServer::spawn(request_started.clone(), None);
    let source = format!(
        "\
let request_started = Path({})
let ready = Path({})
let job = net.start({{
  method: \"GET\",
  url: {},
}})?
while ! request_started.exists()? {{
  time.sleep(1ms)?
}}
fs.write(ready, \"ready\")?
while true {{
  time.sleep(20ms)?
}}
",
        xsh_string_literal(request_started.to_str().expect("UTF-8 request marker")),
        xsh_string_literal(ready.to_str().expect("UTF-8 ready marker")),
        xsh_string_literal(&server.url),
    );

    let output = run_cancelable_temp_script(
        "cancel-live-net-job-no-hook",
        &source,
        [],
        &ready,
        libc::SIGTERM,
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 stderr");
    assert!(stderr.contains("canceled"), "{stderr}");
    server.join();
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(all(feature = "net", any(target_os = "linux", target_os = "macos")))]
#[test]
fn signal_hook_finishes_before_canceling_a_net_job_that_completed_during_the_hook() {
    let root = temp_path("net-job-signal-hook-root");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create net hook test root");
    let request_started = root.join("request-started");
    let hook_started = root.join("hook-started");
    let hook_ran = root.join("hook-ran");
    let ready = root.join("ready");
    let server = SignalNetServer::spawn(request_started.clone(), Some(hook_started.clone()));
    let source = format!(
        "\
let request_started = Path({})
let hook_started = Path({})
let hook_ran = Path({})
let ready = Path({})
on TERM [fs, time, error] {{
  fs.write(hook_started, \"started\")?
  fs.write(hook_ran, \"ran\")?
  time.sleep(100ms)?
}}
let job = net.start({{
  method: \"GET\",
  url: {},
}})?
while ! request_started.exists()? {{
  time.sleep(1ms)?
}}
fs.write(ready, \"ready\")?
while true {{
  time.sleep(20ms)?
}}
",
        xsh_string_literal(request_started.to_str().expect("UTF-8 request marker")),
        xsh_string_literal(hook_started.to_str().expect("UTF-8 hook marker")),
        xsh_string_literal(hook_ran.to_str().expect("UTF-8 hook result marker")),
        xsh_string_literal(ready.to_str().expect("UTF-8 ready marker")),
        xsh_string_literal(&server.url),
    );

    let output = run_cancelable_temp_script(
        "cancel-live-net-job-hook",
        &source,
        ["--trace", "--raw"],
        &ready,
        libc::SIGTERM,
    );

    assert_eq!(output.status.code(), Some(3));
    assert!(hook_ran.exists(), "signal hook did not run");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 stderr");
    let hook_exit = stderr
        .find("kind=signal.hook.exit")
        .expect("signal hook exit trace");
    let net_cancel = stderr
        .find("kind=net.job.shutdown_cancel")
        .expect("net shutdown cancellation trace");
    assert!(
        hook_exit < net_cancel,
        "net job canceled before hook exited:\n{stderr}"
    );
    server.join();
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(all(feature = "net", any(target_os = "linux", target_os = "macos")))]
#[test]
fn active_networking_does_not_block_process_spawn_boundaries() {
    let root = temp_path("net-process-spawn-boundaries-root");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create net process test root");
    let request_started = root.join("request-started");
    let ready = root.join("ready");
    let server = SignalNetServer::spawn(request_started.clone(), None);
    let source = format!(
        "\
let request_started = Path({})
let ready = Path({})
let job = net.start({{
  method: \"GET\",
  url: {},
}})?
while ! request_started.exists()? {{
  time.sleep(1ms)?
}}
var attempts = 0
while attempts < 8 {{
  process.run(process.command_argv(\"true\", [\"true\"]))?
  let spawned = spawn process.command_argv(\"true\", [\"true\"]) ?
  wait spawned?
  run true | run true ?
  process.run(process.command_argv(\"true\", [\"true\"], new_session: true))?
  attempts += 1
}}
fs.write(ready, \"ready\")?
while true {{
  time.sleep(20ms)?
}}
",
        xsh_string_literal(request_started.to_str().expect("UTF-8 request marker")),
        xsh_string_literal(ready.to_str().expect("UTF-8 ready marker")),
        xsh_string_literal(&server.url),
    );

    let output = run_cancelable_temp_script(
        "active-net-process-spawn-boundaries",
        &source,
        [],
        &ready,
        libc::SIGTERM,
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 stderr");
    assert!(stderr.contains("canceled"), "{stderr}");
    server.join();
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(all(feature = "net", any(target_os = "linux", target_os = "macos")))]
#[test]
fn sigterm_drains_process_net_job_and_parallel_workers_with_trace_parentage() {
    let root = tempfile::TempDir::new().expect("create mixed work root");
    let request_started = root.path().join("request-started");
    let process_ready = root.path().join("process-ready");
    let stream_ready = root.path().join("stream-ready");
    let process_leaked = root.path().join("process-leaked");
    let worker_leaked = root.path().join("worker-leaked");
    let server = SignalNetServer::spawn(request_started.clone(), None);
    let shell = "trap '' TERM; (sleep 2; printf leaked > \"$2\") & printf ready > \"$1\"; wait";
    // The HTTP server and signal injection are host boundaries; the script
    // asserts the resource lifecycle through XSH's public operations.
    let source = format!(
        "\
let request_started = Path({})
let process_ready = Path({})
let stream_ready = Path({})
let process_leaked = Path({})
let worker_leaked = Path({})
let child = spawn process.command_argv(\"sh\", [\"sh\", \"-c\", {}, \"sh\", process_ready.display(), process_leaked.display()]) ?
let job = net.start({{method: \"GET\", url: {}}})?
while ! request_started.exists()? or ! process_ready.exists()? {{
  time.sleep(1ms)?
}}
let values = [1, 2] |> par-map(jobs: 2) {{ |value|
  fs.write(stream_ready, \"ready\")?
  time.sleep(2s)?
  fs.write(worker_leaked, \"leaked\")?
  value
}} |> collect()
print ${{values.len()}}
",
        xsh_string_literal(request_started.to_str().expect("UTF-8 request marker")),
        xsh_string_literal(process_ready.to_str().expect("UTF-8 process marker")),
        xsh_string_literal(stream_ready.to_str().expect("UTF-8 stream marker")),
        xsh_string_literal(process_leaked.to_str().expect("UTF-8 process leak marker")),
        xsh_string_literal(worker_leaked.to_str().expect("UTF-8 worker leak marker")),
        xsh_string_literal(shell),
        xsh_string_literal(&server.url),
    );

    let output = run_cancelable_temp_script(
        "cancel-mixed-owned-work",
        &source,
        ["--trace", "--raw"],
        &stream_ready,
        libc::SIGTERM,
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 trace");
    assert!(stderr.contains("canceled"), "{stderr}");
    let mut trace = BTreeMap::new();
    for line in stderr.lines().filter(|line| line.starts_with("id=")) {
        let fields = line.split_whitespace().take(4).collect::<Vec<_>>();
        let id = fields[0]
            .strip_prefix("id=")
            .expect("trace event id")
            .parse::<u64>()
            .expect("numeric trace event id");
        let parent = fields[1].strip_prefix("parent=").expect("trace parent");
        let parent = (parent != "-").then(|| parent.parse::<u64>().expect("numeric trace parent"));
        let kind = fields[3].strip_prefix("kind=").expect("trace kind");
        trace.insert(id, (parent, kind.to_string()));
    }
    let root_id = trace
        .iter()
        .find_map(|(id, (parent, kind))| {
            (parent.is_none() && kind == "script.enter").then_some(*id)
        })
        .expect("script trace root");
    for kind in [
        "spawn.start",
        "net.job.accepted",
        "net.job.shutdown_cancel",
        "parallel.job.start",
        "parallel.job.end",
    ] {
        let id = trace
            .iter()
            .find_map(|(id, (_, event_kind))| (event_kind == kind).then_some(*id))
            .unwrap_or_else(|| panic!("missing {kind}:\n{stderr}"));
        let mut cursor = Some(id);
        for _ in 0..trace.len() {
            if cursor == Some(root_id) {
                break;
            }
            cursor = cursor.and_then(|event_id| trace.get(&event_id).and_then(|event| event.0));
        }
        assert_eq!(cursor, Some(root_id), "orphaned {kind}:\n{stderr}");
    }
    assert!(server.join(), "network connection survived cancellation");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(
        !process_leaked.exists(),
        "spawned process survived cancellation"
    );
    assert!(
        !worker_leaked.exists(),
        "parallel worker survived cancellation"
    );
}

#[cfg(all(feature = "net", any(target_os = "linux", target_os = "macos")))]
struct SignalNetServer {
    url: String,
    handle: std::thread::JoinHandle<bool>,
}

#[cfg(all(feature = "net", any(target_os = "linux", target_os = "macos")))]
impl SignalNetServer {
    fn spawn(request_started: PathBuf, release_response: Option<PathBuf>) -> Self {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind signal HTTP listener");
        let addr = listener.local_addr().expect("signal HTTP listener address");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept signal HTTP request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set signal HTTP read timeout");
            let reader_stream = stream.try_clone().expect("clone signal HTTP stream");
            let mut reader = BufReader::new(reader_stream);
            let mut request_line = String::new();
            reader
                .read_line(&mut request_line)
                .expect("read signal HTTP request line");
            assert!(request_line.starts_with("GET "), "{request_line:?}");
            loop {
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .expect("read signal HTTP header");
                if line == "\r\n" || line == "\n" || line.is_empty() {
                    break;
                }
            }
            std::fs::write(&request_started, "started").expect("write signal request marker");

            if let Some(release_response) = release_response {
                let deadline = Instant::now() + Duration::from_secs(3);
                while !release_response.exists() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(2));
                }
                assert!(
                    release_response.exists(),
                    "signal hook did not release HTTP response"
                );
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .expect("write signal HTTP response");
                stream.flush().expect("flush signal HTTP response");
            }

            let mut byte = [0_u8; 1];
            match stream.read(&mut byte) {
                Ok(0) => true,
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => true,
                _ => false,
            }
        });
        Self {
            url: format!("http://{addr}"),
            handle,
        }
    }

    fn join(self) -> bool {
        self.handle.join().expect("join signal HTTP server")
    }
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

    let example = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
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
  Err(e) => {{
    test.error_kind(e, \"process-missing\")?
    print ${{spawned.detach}} ${{spawned.new_session}} ${{spawned.ignore_hup}} ${{fs.exists(marker)?}} \"process-missing\"
  }}
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

// Raw child stdio and bounded pipe delivery belong to the host process boundary.
#[test]
fn bytes_stdin_redirection_preserves_arbitrary_and_empty_input() {
    let output = run_temp_script("bytes-stdin-exact", r#"
let payload = b"a\0\xff\n"
let copied = run.bytes cat < (payload) ?
io.write_stdout_bytes(copied)?
let empty = run.bytes cat < b"" ?
print empty.len()
"#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"a\0\xff\n0\n");
}

#[test]
fn bytes_stdin_capture_drains_both_outputs_while_feeding_large_input() {
    let output = run_temp_script("bytes-stdin-large-capture", &format!(r#"
let payload = bytes.concat([b"a\0\xff\n", bytes.zero(2097152)?])
let copied = run.capture --bytes --timeout=3s ({}) bytes-echo stderr < (payload) ?
if copied.stderr != payload {{ error.fail("stderr payload changed")? }}
io.write_stdout_bytes(copied.stdout)?
"#, xsh_string_literal(os_probe())));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut expected = b"a\0\xff\n".to_vec(); expected.resize(2097156, 0);
    assert_eq!(output.stdout, expected);
}

#[test]
fn bytes_stdin_early_close_and_first_pipeline_keep_success_status() {
    let output = run_temp_script("bytes-stdin-early-close-pipeline", &format!(r#"
let payload = bytes.zero(1048576)?
let copied = run.bytes --timeout=3s ({probe}) bytes-prefix 1 < (payload) ?
io.write_stdout_bytes(copied)?
run ({probe}) bytes-echo < b"pipe\0\xff" | run cat
"#, probe = xsh_string_literal(os_probe())));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"\0pipe\0\xff");
}

#[test]
fn bytes_stdin_command_routes_and_streams_deliver_exact_input() {
    let output = run_temp_script("bytes-stdin-command-stream", &format!(r#"
let payload = b"cmd\0\xff"
let command = process.command {{ stdin = payload; run cat }}
process.run(command)?
let explicit = process.command_argv("cat", ["cat"], stdin: payload)
let handle = spawn explicit?
let status = wait handle?
if status.ok == false {{ error.fail("command failed")? }}
let streamed = run.stream --bytes ({probe}) bytes-echo < b"stream\n"
for chunk in streamed {{ io.write_stdout_bytes(chunk)? }}
"#, probe = xsh_string_literal(os_probe())));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"cmd\0\xffcmd\0\xffstream\n");
}

#[test]
fn bytes_stdin_owned_spawn_feeds_before_wait_and_closes_on_timeout_or_cancel() {
    let marker = temp_path("bytes-stdin-spawn-marker");
    let ready = temp_path("bytes-stdin-timeout-ready");
    let output = run_temp_script("bytes-stdin-spawn-owners", &format!(r#"
let payload = bytes.zero(262144)?
let marker = Path({marker})
let ready = Path({ready})
let handle = spawn run --timeout=3s ({probe}) bytes-sink-marker (marker) < (payload) ?
var attempts = 0
while marker.exists()? == false and attempts < 200 {{ time.sleep(10ms)?; attempts += 1 }}
let contents = marker.read_text()?
print $contents
let completed = wait handle?
let timed = spawn run --timeout=20ms ({probe}) ready-sleep (ready) < (payload) ?
let failure = wait timed
if failure is Ok(_) {{ error.fail("timeout expected")? }}
let cancelled = spawn run ({probe}) ready-sleep (ready) < (payload) ?
cancelled.cancel(kill_after: 0ms)?
"#, probe=xsh_string_literal(os_probe()), marker=xsh_string_literal(marker.to_str().unwrap()), ready=xsh_string_literal(ready.to_str().unwrap())));
    let _ = std::fs::remove_file(marker); let _ = std::fs::remove_file(ready);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"262144\n");
}

#[test]
fn bytes_stdin_scope_cleanup_reaps_an_unread_input_owner() {
    let ready = temp_path("bytes-stdin-cleanup-ready");
    let output = run_temp_script("bytes-stdin-scope-cleanup", &format!(r#"
proc abandon() [process, error] -> Int {{
  let payload = bytes.zero(1048576)?
  let handle = spawn run ({probe}) ready-sleep ({ready}) < (payload) ?
  return handle.pid
}}
let pid = abandon()
print $pid
"#, probe=xsh_string_literal(os_probe()), ready=xsh_string_literal(ready.to_str().unwrap())));
    let _ = std::fs::remove_file(ready);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let pid: i32 = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "child remains live after scope cleanup");
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
}

#[test]
fn bytes_stdin_empty_payload_closes_input_without_consuming_inherited_stdin() {
    let script = write_temp_script("bytes-stdin-empty-is-explicit", "let empty = run.bytes cat < b\"\" ?\nprint ${empty.len()}\nio.write_stdout_bytes(io.stdin_bytes()?)?\n");
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh")).arg(&script)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"inherited").unwrap();
    let output = child.wait_with_output().unwrap();
    let _ = std::fs::remove_file(script);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"0\ninherited");
}

#[test]
fn accepted_process_stream_keeps_exact_bytes_and_late_policy_failure() {
    let output = run_temp_script("accept-stream-bytes", r#"
let rows = run.stream --bytes --accept=[0] sh -c "printf '\\000\\377'; exit 1" ?
for row in rows { run cat < (row) }
"#);
    assert!(!output.status.success());
    assert_eq!(output.stdout, [0, 255], "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected-exit"));
}

#[test]
fn accepted_process_capture_limit_remains_an_error() {
    let output = run_temp_script("accept-capture-limit", &format!(r#"
let helper = Path({})
let result = run.bytes --accept=[0] ${{helper}} completion-output 0 16777217
match result {{
  Err(ProcessError.CaptureLimit {{message: message}}) => print "limited"
  Err(error) => test.fail(error.message)?
  Ok(_) => test.fail("capture limit was accepted")?
}}
"#, xsh_string_literal(os_probe())));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"limited\n");
}

#[test]
fn accept_policy_expression_runs_once_before_child_spawn() {
    let output = run_temp_script("accept-eval-once", r#"
proc accepted_codes() [process, error] -> List[Int] {
  run printf "option\n"
  return [0, 1]
}
run --accept=accepted_codes() sh -c "printf child; exit 1"
"#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"option\nchild");
}

#[test]
fn accepted_process_stream_early_consumer_stops_and_reaps_child() {
    let marker = temp_path("accept-stream-cancel-marker");
    let _ = std::fs::remove_file(&marker);
    let output = run_temp_script("accept-stream-cancel", &format!(r#"
let marker = Path({})
let rows = run.stream --text --accept=[0] sh -c "printf 'ready\n'; sleep 2; touch ${{marker.display()}}" ?
for row in rows {{
  print $row
  break
}}
time.sleep(100ms)?
print ${{marker.exists()?}}
"#, xsh_string_literal(marker.to_str().unwrap())));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"ready\nfalse\n");
    std::thread::sleep(std::time::Duration::from_millis(2100));
    assert!(!marker.exists(), "early consumer left its child alive");
}

#[test]
fn accepted_process_stream_feeds_bytes_while_draining_large_output() {
    let output = run_temp_script("accept-stream-bytes-feed", &format!(r#"
let payload = bytes.concat([b"a\0\xff\n", bytes.zero(2097152)?])
let rows = run.stream --bytes --timeout=3s --accept=[0] ({}) bytes-echo < (payload) ?
for row in rows {{ io.write_stdout_bytes(row)? }}
"#, xsh_string_literal(os_probe())));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut expected = b"a\0\xff\n".to_vec();
    expected.resize(2097156, 0);
    assert_eq!(output.stdout, expected);
}

#[test]
fn accepted_process_stream_signal_cancellation_reaps_between_pulls() {
    let root = temp_path("accept-stream-signal-owner");
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let shell = "trap '' TERM; (sleep 2; printf leaked > \"$2\") & printf ready > \"$1\"; wait";
    let source = format!(r#"
let rows = run.stream --text --accept=[0,143] sh -c {} sh {} {} ?
while true {{ time.sleep(50ms)? }}
"#, xsh_string_literal(shell), xsh_string_literal(ready.to_str().unwrap()), xsh_string_literal(leaked.to_str().unwrap()));
    let output = run_cancelable_temp_script("accept-stream-signal", &source, [], &ready, libc::SIGTERM);
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("canceled"), "{stderr}");
    assert!(!stderr.contains("unexpected-exit"), "{stderr}");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn accepted_process_stream_early_consumer_stops_descendants_after_child_exit() {
    let marker = temp_path("accept-stream-exited-child-marker");
    let _ = std::fs::remove_file(&marker);
    let shell = "(sleep 0.2; printf 'ready\\n'; sleep 2; touch \"$1\") & exit 0";
    let output = run_temp_script("accept-stream-exited-child", &format!(r#"
let rows = run.stream --text --accept=[0] sh -c {} sh {} ?
for row in rows {{
  print $row
  break
}}
"#, xsh_string_literal(shell), xsh_string_literal(marker.to_str().unwrap())));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"ready\n");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!marker.exists(), "stream cancellation left its exited child's descendant alive");
}
