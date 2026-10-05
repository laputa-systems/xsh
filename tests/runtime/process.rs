use super::common::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn test_helper() -> &'static str {
    cargo_env!("CARGO_BIN_EXE_xsh-test-helper")
}

// The command runs under an argv[0] (`show`) that differs from its path, and
// the helper prints each argument's exact bytes in hex. A standard tool cannot
// stand in: a multi-call `sh` selects its applet by argv[0].
#[test]
fn process_argv_words_command_argv_and_run_execute() {
    let output = run_temp_script(
        "process-argv-command-run",
        &format!(
            "\
let show_argv = Path({})
let words = process.argv_words(\"show show-argv ignored 'two words' escaped\\\\ space\")?
let command = process.command_argv(show_argv, words)
let status = process.run(command)?
let env_command = process.command_argv(\"printenv\", [\"printenv\", \"XSH_PLAN\"], Path(\".\"), {{XSH_PLAN: \"ready\"}})
let env_status = process.run(env_command)?
let false_status = process.run(process.command_argv(\"false\", [\"false\"]))?
print ${{status.ok}} ${{env_status.ok}} ${{false_status.exited_with(1)}}
",
            xsh_string_literal(test_helper()),
        ),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "69676e6f726564\n74776f20776f726473\n65736361706564207370616365\nready\ntrue true true\n"
    );
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
ready.write(\"ready\")?
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
  hook_started.write(\"started\")?
  hook_ran.write(\"ran\")?
  time.sleep(100ms)?
}}
let job = net.start({{
  method: \"GET\",
  url: {},
}})?
while ! request_started.exists()? {{
  time.sleep(1ms)?
}}
ready.write(\"ready\")?
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
  let _ = process.run(process.command_argv(\"true\", [\"true\"]))?
  let spawned = spawn process.command_argv(\"true\", [\"true\"]) ?
  let _ = wait spawned?
  run true | run true ?
  let _ = process.run(process.command_argv(\"true\", [\"true\"], new_session: true))?
  attempts += 1
}}
ready.write(\"ready\")?
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
  stream_ready.write(\"ready\")?
  time.sleep(2s)?
  worker_leaked.write(\"leaked\")?
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
  let _ = process.run(command)?
}} ?
",
        xsh_string_literal(root.to_str().unwrap()),
        xsh_string_literal(ready.to_str().unwrap()),
        xsh_string_literal(leaked.to_str().unwrap()),
        xsh_string_literal(test_helper())
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
        xsh_string_literal(test_helper()),
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

// Raw child stdio and bounded pipe delivery belong to the host process boundary.
#[test]
fn bytes_stdin_capture_drains_both_outputs_while_feeding_large_input() {
    let output = run_temp_script(
        "bytes-stdin-large-capture",
        &format!(
            r#"
let payload = bytes.concat([b"a\0\xff\n", bytes.zero(2097152)?])
let copied = run.capture --bytes --timeout=3s ({}) bytes-echo stderr < (payload) ?
if copied.stderr != payload {{ error.fail("stderr payload changed")? }}
io.write_stdout_bytes(copied.stdout)?
"#,
            xsh_string_literal(test_helper())
        ),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = b"a\0\xff\n".to_vec();
    expected.resize(2097156, 0);
    assert_eq!(output.stdout, expected);
}

#[test]
fn bytes_stdin_early_close_and_first_pipeline_keep_success_status() {
    let output = run_temp_script(
        "bytes-stdin-early-close-pipeline",
        &format!(
            r#"
let payload = bytes.zero(1048576)?
let copied = run.bytes --timeout=3s ({probe}) bytes-prefix 1 < (payload) ?
io.write_stdout_bytes(copied)?
run ({probe}) bytes-echo < b"pipe\0\xff" | run cat
"#,
            probe = xsh_string_literal(test_helper())
        ),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"\0pipe\0\xff");
}

#[test]
fn bytes_stdin_owned_spawn_feeds_before_wait_and_closes_on_timeout_or_cancel() {
    let marker = temp_path("bytes-stdin-spawn-marker");
    let ready = temp_path("bytes-stdin-timeout-ready");
    let output = run_temp_script(
        "bytes-stdin-spawn-owners",
        &format!(
            r#"
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
"#,
            probe = xsh_string_literal(test_helper()),
            marker = xsh_string_literal(marker.to_str().unwrap()),
            ready = xsh_string_literal(ready.to_str().unwrap())
        ),
    );
    let _ = std::fs::remove_file(marker);
    let _ = std::fs::remove_file(ready);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"262144\n");
}

#[test]
fn accepted_process_stream_signal_cancellation_reaps_between_pulls() {
    let root = temp_path("accept-stream-signal-owner");
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let leaked = root.join("leaked");
    let shell = "trap '' TERM; (sleep 2; printf leaked > \"$2\") & printf ready > \"$1\"; wait";
    let source = format!(
        r#"
let rows = run.stream --text --accept=[0,143] sh -c {} sh {} {} ?
while true {{ time.sleep(50ms)? }}
"#,
        xsh_string_literal(shell),
        xsh_string_literal(ready.to_str().unwrap()),
        xsh_string_literal(leaked.to_str().unwrap())
    );
    let output =
        run_cancelable_temp_script("accept-stream-signal", &source, [], &ready, libc::SIGTERM);
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("canceled"), "{stderr}");
    assert!(!stderr.contains("unexpected-exit"), "{stderr}");
    std::thread::sleep(Duration::from_millis(2300));
    assert!(!leaked.exists());
    let _ = std::fs::remove_dir_all(root);
}
