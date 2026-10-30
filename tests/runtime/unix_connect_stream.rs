use super::common::*;
use std::os::unix::net::UnixListener;

fn run(source: &str, path: &Path) -> std::process::Output {
    static NEXT_SCRIPT: AtomicUsize = AtomicUsize::new(0);
    let name = format!("unix-connect-stream-{}", NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed));
    let script = write_temp_script(&name, source);
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script).arg(path).arg(cargo_env!("CARGO_BIN_EXE_xsh-test-helper")).output().expect("run xsh");
    let _ = std::fs::remove_file(script);
    output
}

#[test]
fn unix_connect_stream_exchanges_bytes_and_observes_disconnect() {
    let root = PathBuf::from(format!(".work/tmp/u{} space", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("socket");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let peer = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(peer) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("accept: {error}"),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut request = [0; 4];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"ping");
        stream.write_all(b"pong").unwrap();
    });
    let output = run(r#"proc main(...argv: List[Str]) [process, net, error] {
  let fd = unix.connect_stream(Path(argv[0]), 2s)?
  defer unix.close_fd(fd)
  assert unix.read_fd(fd, 1) is Err(is HostIo)
  let helper = argv[1]
  let inherited = run.capture --text $helper show-fds
  assert inherited.status.exited_with(0)
  assert "socket local=" not in inherited.stdout
  assert unix.write_fd(fd, b"ping")? == 4
  assert "readable" in unix.poll_fd(fd, ["readable"], 2000)?
  assert unix.read_fd(fd, 4)? == b"pong"
  assert "hangup" in unix.poll_fd(fd, ["readable"], 2000)?
  assert unix.read_fd(fd, 1)? == b""
}
"#, &path);
    peer.join().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn unix_connect_stream_times_out_on_full_backlog_without_leaking_fds() {
    use std::os::unix::net::UnixStream;
    let root = PathBuf::from(format!(".work/tmp/b{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("socket");
    let listener = UnixListener::bind(&path).unwrap();
    rustix::net::listen(&listener, 0).unwrap();
    let filler = UnixStream::connect(&path).unwrap();
    let output = run(r#"proc main(...argv: List[Str]) [process, net, fs, error] {
  let socket_path = Path(argv[0])
  assert unix.connect_stream(socket_path, 0ms) is Err(is Timeout)
  let before = [entry.name for entry in fs.children(p"/proc/self/fd")?].len()
  for _ in [1, 2, 3, 4] {
    assert unix.connect_stream(socket_path, 30ms) is Err(is Timeout)
  }
  assert [entry.name for entry in fs.children(p"/proc/self/fd")?].len() == before
}
"#, &path);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    drop(filler);
    drop(listener);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unix_connect_stream_preserves_platform_path_limit() {
    let cwd = std::env::current_dir().unwrap();
    let prefix = cwd.as_os_str().as_bytes().len() + 1;
    // SAFETY: a zeroed sockaddr_un is valid for inspecting the size of its path field.
    let address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    let limit = address.sun_path.len();
    assert!(prefix < limit);
    let max = cwd.join("x".repeat(limit - prefix));
    let too_long = cwd.join("x".repeat(limit - prefix + 1));
    let output = run(r#"proc main(...argv: List[Str]) [process, net, error] {
  assert unix.connect_stream(Path(argv[0]), 1s) is Err(is NotFound)
}
"#, &max);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = run(r#"proc main(...argv: List[Str]) [process, net, error] {
  let result = unix.connect_stream(Path(argv[0]), 1s)
  assert result is Err(is HostIo)
  assert !(result is Err(is NotFound))
}
"#, &too_long);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}


#[cfg(target_os = "linux")]
#[test]
fn unix_connect_stream_cancels_a_waiting_connection() {
    use std::os::unix::net::UnixStream;
    let root = PathBuf::from(format!(".work/tmp/c{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("socket");
    let ready = root.join("ready");
    let listener = UnixListener::bind(&path).unwrap();
    rustix::net::listen(&listener, 0).unwrap();
    let filler = UnixStream::connect(&path).unwrap();
    let source = format!(r#"defer {{ print "cleanup" }}
Path({}).write("ready")?
let _ = unix.connect_stream(Path({}), 20s)?
"#, xsh_string_literal(ready.to_str().unwrap()), xsh_string_literal(path.to_str().unwrap()));
    let script = write_temp_script("unix-connect-cancel", &source);
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    wait_for_path(&ready, Duration::from_secs(5), &mut child);
    let deadline = Instant::now() + Duration::from_secs(5);
    let tasks = format!("/proc/{}/task", child.id());
    loop {
        let waiting = std::fs::read_dir(&tasks).ok().is_some_and(|tasks| tasks.flatten().any(|task| {
            std::fs::read_to_string(task.path().join("syscall")).ok().is_some_and(|line| {
                line.split_whitespace().next().and_then(|number| number.parse::<libc::c_long>().ok()) == Some(libc::SYS_ppoll)
            })
        }));
        if waiting { break; }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("connection never entered its interruptible wait");
        }
        std::thread::yield_now();
    }
    // SAFETY: child.id() identifies the live child observed above.
    assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) }, 0);
    let status = wait_child_status(&mut child, Duration::from_secs(5));
    let output = child.wait_with_output().unwrap();
    assert_eq!(status.code(), Some(3), "{:?}", output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "cleanup\n");
    drop(filler);
    drop(listener);
    std::fs::remove_file(script).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
