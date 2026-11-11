use super::common::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use xsh::process::{ChildWaitOutcome, ProcessGroup, ProcessInvocation, ProcessStatusKind};

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn test_helper() -> &'static str {
    cargo_env!("CARGO_BIN_EXE_xsh-test-helper")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct ProcessGroupGuard(Option<libc::pid_t>);

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if let Some(pgid) = self.0
            && process_group_exists(pgid)
        {
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
            let _ = wait_for_process_group_exit(pgid, Duration::from_secs(2));
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct ScriptChild(Child);

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl ScriptChild {
    fn child(&mut self) -> &mut Child {
        &mut self.0
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for ScriptChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn process_group_exists(pgid: libc::pid_t) -> bool {
    if unsafe { libc::kill(-pgid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn wait_for_process_group_exit(pgid: libc::pid_t, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !process_group_exists(pgid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    !process_group_exists(pgid)
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sigterm_during_script_source_read_is_not_lost_before_execution() {
    use std::os::unix::fs::OpenOptionsExt;

    let source = temp_path("signal-during-source-read");
    let source_c = std::ffi::CString::new(source.as_os_str().as_bytes())
        .expect("FIFO path has no NUL bytes");
    assert_eq!(unsafe { libc::mkfifo(source_c.as_ptr(), 0o600) }, 0);

    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&source)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn xsh on FIFO source");

    // Opening the FIFO proves xsh has installed its handlers and entered the
    // source read. Keep the writer open after the complete source bytes so
    // preparation cannot finish until after SIGTERM is delivered.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut writer = loop {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).custom_flags(libc::O_NONBLOCK);
        match options.open(&source) {
            Ok(writer) => break writer,
            Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&source);
                    panic!("xsh did not open the FIFO source");
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&source);
                panic!("open FIFO source writer: {error}");
            }
        }
    };
    writer.write_all(b"print \"ran\"\n").expect("write script source");
    let result = unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    assert_eq!(result, 0, "send SIGTERM during source read");
    drop(writer);

    let status = wait_child_status(&mut child, Duration::from_secs(5));
    let mut stdout = Vec::new();
    child.stdout.take().expect("child stdout").read_to_end(&mut stdout).unwrap();
    let mut stderr = Vec::new();
    child.stderr.take().expect("child stderr").read_to_end(&mut stderr).unwrap();
    let _ = std::fs::remove_file(&source);

    assert_eq!(status.code(), Some(3), "{stderr:?}");
    assert!(stdout.is_empty(), "script body ran after SIGTERM: {stdout:?}");
    assert!(stderr.is_empty(), "unexpected idle cancellation diagnostic: {stderr:?}");
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sigterm_after_stream_output_cancels_descendants_and_reports_cleanup_failure() {
    let root = temp_path("stream-signal-cleanup-root");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let group_id = root.join("group-id");
    let descendant_ready = root.join("descendant-ready");
    let shell = "printf '%s\\n' \"$$\" > \"$1.tmp\"; mv \"$1.tmp\" \"$1\"; printf 'partial\\n'; (printf ready > \"$2\"; exec sleep 30) & wait";
    let source = format!(
        r#"
let ready = Path({})
let group_id = Path({})
let descendant_ready = Path({})
defer {{ error.fail("secondary cleanup failure")? }}
let lines = run.stream --text sh -c {} sh {} {} ?
for line in lines {{
  print $line
  while ! group_id.exists()? {{ time.sleep(1ms)? }}
  while ! descendant_ready.exists()? {{ time.sleep(1ms)? }}
  ready.write("observed")?
}}
"#,
        xsh_string_literal(ready.to_str().unwrap()),
        xsh_string_literal(group_id.to_str().unwrap()),
        xsh_string_literal(descendant_ready.to_str().unwrap()),
        xsh_string_literal(shell),
        xsh_string_literal(group_id.to_str().unwrap()),
        xsh_string_literal(descendant_ready.to_str().unwrap())
    );

    let script = write_temp_script("stream-signal-cleanup", &source);
    let mut child = ScriptChild(
        Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn stream cancellation script"),
    );
    let group_id_path = group_id;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut group = None;
    loop {
        if group.is_none()
            && let Ok(text) = std::fs::read_to_string(&group_id_path)
            && let Ok(group_id) = text.trim().parse::<libc::pid_t>()
        {
            if group_id <= 1 {
                unsafe {
                    libc::kill(child.child().id() as libc::pid_t, libc::SIGTERM);
                }
                let _ = wait_child_status(child.child(), Duration::from_secs(5));
                let _ = std::fs::remove_file(&script);
                panic!("invalid process group leader id {group_id}");
            }
            let actual_pgid = unsafe { libc::getpgid(group_id) };
            if actual_pgid != group_id {
                unsafe {
                    libc::kill(child.child().id() as libc::pid_t, libc::SIGTERM);
                }
                let _ = wait_child_status(child.child(), Duration::from_secs(5));
                let _ = std::fs::remove_file(&script);
                panic!("run.stream child process group {actual_pgid} does not match its leader {group_id}");
            }
            group = Some(ProcessGroupGuard(Some(actual_pgid)));
        }
        if ready.exists() {
            break;
        }
        if let Some(status) = child
            .child()
            .try_wait()
            .expect("poll stream cancellation script")
        {
            let mut stdout = String::new();
            let mut stderr = String::new();
            child
                .child()
                .stdout
                .take()
                .expect("stream script stdout")
                .read_to_string(&mut stdout)
                .expect("read stream script stdout");
            child
                .child()
                .stderr
                .take()
                .expect("stream script stderr")
                .read_to_string(&mut stderr)
                .expect("read stream script stderr");
            let _ = std::fs::remove_file(&script);
            panic!("stream cancellation script exited before readiness: {status}; stdout: {stdout}; stderr: {stderr}");
        }
        if Instant::now() >= deadline {
            unsafe {
                libc::kill(child.child().id() as libc::pid_t, libc::SIGTERM);
            }
            let _ = std::fs::remove_file(&script);
            let _ = wait_child_status(child.child(), Duration::from_secs(5));
            panic!("stream cancellation script did not reach readiness");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if group.is_none() {
        let text = std::fs::read_to_string(&group_id_path).expect("read process group marker");
        let group_id = text.trim().parse::<libc::pid_t>().expect("parse process group marker");
        let actual_pgid = unsafe { libc::getpgid(group_id) };
        if group_id <= 1 || actual_pgid != group_id {
            unsafe {
                libc::kill(child.child().id() as libc::pid_t, libc::SIGTERM);
            }
            let _ = wait_child_status(child.child(), Duration::from_secs(5));
            let _ = std::fs::remove_file(&script);
            panic!("run.stream child process group {actual_pgid} does not match its leader {group_id}");
        }
        group = Some(ProcessGroupGuard(Some(actual_pgid)));
    }
    let mut group = group.expect("ready process group guard");
    assert!(descendant_ready.exists(), "descendant did not reach readiness");
    let signal_result = unsafe { libc::kill(child.child().id() as libc::pid_t, libc::SIGTERM) };
    assert_eq!(signal_result, 0, "send SIGTERM to stream cancellation script");
    let status = wait_child_status(child.child(), Duration::from_secs(5));
    assert!(
        wait_for_process_group_exit(
            group.0.expect("owned process group id"),
            Duration::from_secs(3)
        ),
        "owned process group was not reaped"
    );
    group.0 = None;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .child()
        .stdout
        .take()
        .expect("stream script stdout")
        .read_to_end(&mut stdout)
        .expect("read stream script stdout");
    child
        .child()
        .stderr
        .take()
        .expect("stream script stderr")
        .read_to_end(&mut stderr)
        .expect("read stream script stderr");
    let _ = std::fs::remove_file(&script);
    let output = std::process::Output {
        status,
        stdout,
        stderr,
    };

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(output.stdout, b"partial\n");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("canceled"), "{stderr}");
    assert!(stderr.contains("cleanup error ["), "{stderr}");
    assert!(stderr.contains("secondary cleanup failure"), "{stderr}");
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn write_pipeline_job_stage(
    root: &Path,
    name: &str,
    ready: &Path,
    go: &Path,
    done: Option<&Path>,
) -> PathBuf {
    let done = done.map_or_else(String::new, |done| {
        format!(
            "Path({}).write(\"done\")?\n",
            xsh_string_literal(done.to_str().unwrap())
        )
    });
    let source = format!(
        "\
let ready = Path({})
ready.write(\"ready\")?
let go = Path({})
while !go.exists()? {{ time.sleep(5ms)? }}
{done}",
        xsh_string_literal(ready.to_str().unwrap()),
        xsh_string_literal(go.to_str().unwrap())
    );
    let path = root.join(name);
    std::fs::write(&path, source).expect("write pipeline job stage script");
    path
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn pipeline_job_invocation(script: &Path) -> ProcessInvocation {
    ProcessInvocation {
        target: cargo_env!("CARGO_BIN_EXE_xsh").as_bytes().to_vec(),
        argv: vec![script.as_os_str().as_bytes().to_vec()],
        cwd: PathBuf::from("/"),
        env: BTreeMap::new(),
        env_overlay: BTreeMap::new(),
        redirections: Vec::new(),
        timeout: None,
        cpu_max: None,
        accepted_exit_codes: None,
        namespaces: None,
        confinement: None,
        rooted_executable: None,
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn wait_for_pipeline_job_ready(paths: &[PathBuf]) {
    let deadline = Instant::now() + Duration::from_secs(10);
    for path in paths {
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "pipeline job stage did not become ready: {}",
                path.display()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_pipeline_job_stages_exited(outcomes: &[ChildWaitOutcome]) {
    for outcome in outcomes {
        let ChildWaitOutcome::Exited(status) = outcome else {
            panic!("expected exited pipeline job stage, got {outcome:?}");
        };
        assert!(status.success, "pipeline job stage failed: {status:?}");
        assert_eq!(status.kind, ProcessStatusKind::Exit);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_pipeline_job_reaped(
    pgid: libc::pid_t,
    pids: &[u32],
    outcomes: &[ChildWaitOutcome],
) {
    assert_eq!(outcomes.len(), pids.len());
    for pid in pids {
        let mut status = 0;
        let waited = unsafe { libc::waitpid(*pid as libc::pid_t, &mut status, libc::WNOHANG) };
        assert_eq!(waited, -1, "stage {pid} was not reaped");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD),
            "stage {pid} is not an exited child"
        );
    }
    assert_eq!(
        unsafe { libc::kill(-pgid, 0) },
        -1,
        "job group {pgid} still exists"
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH),
        "job group {pgid} is not empty"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn pipeline_job_stops_and_resumes_two_stages() {
    let root = temp_path("pipeline-job-two");
    std::fs::create_dir_all(&root).unwrap();
    let go = root.join("go");
    let scripts: Vec<_> = (0..2)
        .map(|index| {
            write_pipeline_job_stage(
                &root,
                &format!("stage-{index}.xsh"),
                &root.join(format!("ready-{index}")),
                &go,
                None,
            )
        })
        .collect();
    let invocations: Vec<_> = scripts
        .iter()
        .map(|script| pipeline_job_invocation(script))
        .collect();
    let mut job = ProcessGroup::spawn_pipeline_job(&invocations).expect("spawn two-stage job");
    let pids = job.pids();
    let pgid = job.process_group().id();
    assert_eq!(pids.len(), 2);
    for pid in &pids {
        assert_eq!(
            unsafe { libc::getpgid(*pid as libc::pid_t) },
            pgid,
            "stage {pid} is not in the job process group"
        );
    }
    wait_for_pipeline_job_ready(&[root.join("ready-0"), root.join("ready-1")]);
    assert_eq!(
        unsafe { libc::kill(-pgid, libc::SIGTSTP) },
        0,
        "stop the job process group: {}",
        std::io::Error::last_os_error()
    );
    let stopped = job.wait().expect("wait for the stopped two-stage job");
    assert_eq!(stopped.len(), 2);
    for outcome in &stopped {
        assert!(
            matches!(outcome, ChildWaitOutcome::Stopped { signal } if *signal == libc::SIGTSTP),
            "expected a SIGTSTP stop, got {outcome:?}"
        );
    }
    std::fs::write(&go, b"").expect("release the stopped stages");
    job.resume();
    let completed = job.wait().expect("wait for the resumed two-stage job");
    assert_eq!(completed.len(), 2);
    assert_pipeline_job_stages_exited(&completed);
    assert_pipeline_job_reaped(pgid, &pids, &completed);
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn pipeline_job_stops_and_resumes_three_stages() {
    let root = temp_path("pipeline-job-three");
    std::fs::create_dir_all(&root).unwrap();
    let go = root.join("go");
    let scripts: Vec<_> = (0..3)
        .map(|index| {
            write_pipeline_job_stage(
                &root,
                &format!("stage-{index}.xsh"),
                &root.join(format!("ready-{index}")),
                &go,
                None,
            )
        })
        .collect();
    let invocations: Vec<_> = scripts
        .iter()
        .map(|script| pipeline_job_invocation(script))
        .collect();
    let mut job = ProcessGroup::spawn_pipeline_job(&invocations).expect("spawn three-stage job");
    let pids = job.pids();
    let pgid = job.process_group().id();
    assert_eq!(pids.len(), 3);
    wait_for_pipeline_job_ready(&[
        root.join("ready-0"),
        root.join("ready-1"),
        root.join("ready-2"),
    ]);
    assert_eq!(
        unsafe { libc::kill(-pgid, libc::SIGTSTP) },
        0,
        "stop the job process group: {}",
        std::io::Error::last_os_error()
    );
    let stopped = job.wait().expect("wait for the stopped three-stage job");
    assert_eq!(stopped.len(), 3);
    for outcome in &stopped {
        assert!(
            matches!(outcome, ChildWaitOutcome::Stopped { signal } if *signal == libc::SIGTSTP),
            "expected a SIGTSTP stop, got {outcome:?}"
        );
    }
    std::fs::write(&go, b"").expect("release the stopped stages");
    job.resume();
    let completed = job.wait().expect("wait for the resumed three-stage job");
    assert_eq!(completed.len(), 3);
    assert_pipeline_job_stages_exited(&completed);
    assert_pipeline_job_reaped(pgid, &pids, &completed);
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn pipeline_job_reports_exit_while_another_stage_stays_stopped() {
    let root = temp_path("pipeline-job-mixed");
    std::fs::create_dir_all(&root).unwrap();
    let go = root.join("go");
    let go_fast = root.join("go-fast");
    let done_fast = root.join("done-fast");
    let fast = write_pipeline_job_stage(
        &root,
        "fast.xsh",
        &root.join("ready-fast"),
        &go_fast,
        Some(&done_fast),
    );
    let waiting =
        write_pipeline_job_stage(&root, "waiting.xsh", &root.join("ready-waiting"), &go, None);
    let invocations = vec![
        pipeline_job_invocation(&fast),
        pipeline_job_invocation(&waiting),
    ];
    let mut job = ProcessGroup::spawn_pipeline_job(&invocations).expect("spawn mixed job");
    let pids = job.pids();
    let pgid = job.process_group().id();
    wait_for_pipeline_job_ready(&[root.join("ready-fast"), root.join("ready-waiting")]);
    assert_eq!(
        unsafe { libc::kill(-pgid, libc::SIGTSTP) },
        0,
        "stop the job process group: {}",
        std::io::Error::last_os_error()
    );
    let stopped = job.wait().expect("wait for the stopped mixed job");
    assert_eq!(stopped.len(), 2);
    for outcome in &stopped {
        assert!(
            matches!(outcome, ChildWaitOutcome::Stopped { signal } if *signal == libc::SIGTSTP),
            "expected a SIGTSTP stop, got {outcome:?}"
        );
    }
    std::fs::write(&go_fast, b"").expect("release the first stage");
    assert_eq!(
        unsafe { libc::kill(pids[0] as libc::pid_t, libc::SIGCONT) },
        0,
        "continue only the first stage: {}",
        std::io::Error::last_os_error()
    );
    wait_for_pipeline_job_ready(&[done_fast]);
    let mixed = job.wait().expect("wait for the mixed job");
    assert_eq!(mixed.len(), 2);
    assert!(
        matches!(&mixed[0], ChildWaitOutcome::Exited(status) if status.success),
        "first stage should have exited while the second stayed stopped: {mixed:?}"
    );
    assert!(
        matches!(&mixed[1], ChildWaitOutcome::Stopped { signal } if *signal == libc::SIGTSTP),
        "second stage should have stayed stopped: {mixed:?}"
    );
    std::fs::write(&go, b"").expect("release the waiting stage");
    job.resume();
    let completed = job.wait().expect("wait for the completed mixed job");
    assert_eq!(completed.len(), 2);
    assert_pipeline_job_stages_exited(&completed);
    assert_pipeline_job_reaped(pgid, &pids, &completed);
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn pipeline_job_killed_while_stopped_reports_signaled_and_reaps() {
    let root = temp_path("pipeline-job-killed");
    std::fs::create_dir_all(&root).unwrap();
    let go = root.join("go");
    let scripts: Vec<_> = (0..2)
        .map(|index| {
            write_pipeline_job_stage(
                &root,
                &format!("stage-{index}.xsh"),
                &root.join(format!("ready-{index}")),
                &go,
                None,
            )
        })
        .collect();
    let invocations: Vec<_> = scripts
        .iter()
        .map(|script| pipeline_job_invocation(script))
        .collect();
    let mut job = ProcessGroup::spawn_pipeline_job(&invocations).expect("spawn job to kill");
    let pids = job.pids();
    let pgid = job.process_group().id();
    wait_for_pipeline_job_ready(&[root.join("ready-0"), root.join("ready-1")]);
    assert_eq!(
        unsafe { libc::kill(-pgid, libc::SIGTSTP) },
        0,
        "stop the job process group: {}",
        std::io::Error::last_os_error()
    );
    let stopped = job.wait().expect("wait for the stopped job");
    assert_eq!(stopped.len(), 2);
    for outcome in &stopped {
        assert!(
            matches!(outcome, ChildWaitOutcome::Stopped { signal } if *signal == libc::SIGTSTP),
            "expected a SIGTSTP stop, got {outcome:?}"
        );
    }
    job.kill();
    let killed = job.wait().expect("reap the killed job");
    assert_eq!(killed.len(), 2);
    for outcome in &killed {
        let ChildWaitOutcome::Signaled(status) = outcome else {
            panic!("expected a signaled stage, got {outcome:?}");
        };
        assert_eq!(status.kind, ProcessStatusKind::Signal);
        assert_eq!(status.code, Some(libc::SIGKILL));
        assert!(!status.success);
    }
    assert_pipeline_job_reaped(pgid, &pids, &killed);
    let _ = std::fs::remove_dir_all(root);
}
