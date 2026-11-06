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

#[cfg(target_os = "linux")]
#[test]
fn unix_stream_short_write_preserves_prefix_and_reports_backpressure() {
    let root = tempfile::Builder::new().prefix("w").tempdir_in(".work/tmp").unwrap();
    let path = root.path().join("socket");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (release, wait) = std::sync::mpsc::channel();
    let peer = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(peer) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::yield_now(),
                Err(error) => return Err(error),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let mut received = Vec::new();
        stream.read_to_end(&mut received).unwrap();
        Ok(received)
    });
    let script = write_temp_script("unix-stream-backpressure", r#"proc main(...argv: List[Str]) [process, net, error] {
  let fd = unix.connect_stream(Path(argv[0]), 2s)?
  defer unix.close_fd(fd)
  let payload = bytes.concat([b"prefix", bytes.zero(4194304)?])
  let written = unix.write_fd(fd, payload)?
  assert written > 0 and written < payload.len()
  assert unix.poll_fd(fd, ["writable"], 0)? == []
  assert unix.write_fd(fd, payload.slice(written)) is Err(is HostIo)
  print $written
}
"#);
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script).arg(&path).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    wait_child_status(&mut child, Duration::from_secs(8));
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(script).unwrap();
    let _ = release.send(());
    let received = peer.join().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let received = received.unwrap();
    let written: usize = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
    assert_eq!(received.len(), written);
    assert!(received.starts_with(b"prefix"));
    assert!(received[6..].iter().all(|byte| *byte == 0));
    root.close().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn unix_stream_retries_short_writes_after_peer_drains() {
    let root = tempfile::Builder::new().prefix("r").tempdir_in(".work/tmp").unwrap();
    let path = root.path().join("socket");
    let ready = root.path().join("ready");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let peer_ready = ready.clone();
    let peer = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(peer) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::yield_now(),
                Err(error) => return Err(error),
            }
        };
        while !peer_ready.exists() {
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "client never reached backpressure"));
            }
            std::thread::yield_now();
        }
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut received = Vec::new();
        stream.read_to_end(&mut received)?;
        Ok(received)
    });
    let script = write_temp_script("unix-stream-retry", r#"proc main(...argv: List[Str]) [process, net, fs, time, error] {
  let fd = unix.connect_stream(Path(argv[0]), 2s)?
  defer unix.close_fd(fd)
  let payload = bytes.concat([b"prefix", bytes.zero(4194304)?, b"suffix"])
  var offset = unix.write_fd(fd, payload)?
  assert offset > 0 and offset < payload.len()
  assert unix.poll_fd(fd, ["writable"], 0)? == []
  Path(argv[1]).write("ready")?
  let deadline = time.monotonic()? + 5000000000
  while offset < payload.len() {
    let remaining = deadline - time.monotonic()?
    assert remaining > 0
    assert "writable" in unix.poll_fd(fd, ["writable"], (remaining + 999999) / 1000000)?
    let written = unix.write_fd(fd, payload.slice(offset))?
    assert written > 0
    offset += written
  }
  print $offset
}
"#);
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script).arg(&path).arg(&ready).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    wait_child_status(&mut child, Duration::from_secs(8));
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(script).unwrap();
    let received = peer.join().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let received = received.unwrap();
    let written: usize = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
    assert_eq!(written, 4194316);
    assert_eq!(received.len(), written);
    assert!(received.starts_with(b"prefix"));
    assert!(received[6..4194310].iter().all(|byte| *byte == 0));
    assert!(received.ends_with(b"suffix"));
    root.close().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn unix_stream_deadline_closes_socket_before_caller_returns() {
    let root = tempfile::Builder::new().prefix("d").tempdir_in(".work/tmp").unwrap();
    let path = root.path().join("socket");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let peer = std::thread::spawn(move || {
        let result = (|| -> std::io::Result<usize> {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(peer) => break peer,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::yield_now(),
                    Err(error) => return Err(error),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.read(&mut [0; 1])
        })();
        let _ = send.send(result);
    });
    let script = write_temp_script("unix-stream-deadline", r#"proc wait_socket(socket: Path) -> Result[Unit, Error] {
  let fd = unix.connect_stream(socket, 2s)?
  defer unix.close_fd(fd)
  assert unix.poll_fd(fd, ["readable"], 200)? == []
}
proc main(...argv: List[Str]) [process, net, io, error] {
  wait_socket(Path(argv[0]))?
  assert io.stdin_line()? == "release"
  print "done"
}
"#);
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script).arg(&path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let eof = receive.recv_timeout(Duration::from_secs(6));
    let alive = child.try_wait().unwrap().is_none();
    if eof.as_ref().is_ok_and(|result| result.as_ref().is_ok_and(|count| *count == 0)) && alive {
        child.stdin.take().unwrap().write_all(b"release\n").unwrap();
    } else {
        let _ = child.kill();
    }
    wait_child_status(&mut child, Duration::from_secs(3));
    let output = child.wait_with_output().unwrap();
    peer.join().unwrap();
    std::fs::remove_file(script).unwrap();
    assert!(alive, "client exited before peer observed EOF: {:?}", output);
    assert_eq!(eof.unwrap().unwrap(), 0, "peer did not observe EOF");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"done\n");
    root.close().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn unix_stream_signal_during_deadline_wait_closes_peer() {
    let root = tempfile::Builder::new().prefix("s").tempdir_in(".work/tmp").unwrap();
    let path = root.path().join("socket");
    let ready = root.path().join("ready");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let peer = std::thread::spawn(move || -> std::io::Result<usize> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(peer) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::yield_now(),
                Err(error) => return Err(error),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.read(&mut [0; 1])
    });
    let script = write_temp_script("unix-stream-signal", r#"proc main(...argv: List[Str]) [process, net, fs, io, error] {
  let fd = unix.connect_stream(Path(argv[0]), 2s)?
  defer unix.close_fd(fd)
  defer { print "cleanup" }
  Path(argv[1]).write("ready")?
  let _ = unix.poll_fd(fd, ["readable"], 1000)?
  print "unexpected"
}
"#);
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script).arg(&path).arg(&ready).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() && Instant::now() < deadline && child.try_wait().unwrap().is_none() {
        std::thread::yield_now();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let tasks = format!("/proc/{}/task", child.id());
    let waiting = ready.exists() && loop {
        if std::fs::read_dir(&tasks).ok().is_some_and(|tasks| tasks.flatten().any(|task| {
            std::fs::read_to_string(task.path().join("syscall")).ok().is_some_and(|line| {
                line.split_whitespace().next().and_then(|number| number.parse::<libc::c_long>().ok()) == Some(libc::SYS_ppoll)
            })
        })) { break true; }
        if Instant::now() >= deadline || child.try_wait().unwrap().is_some() { break false; }
        std::thread::yield_now();
    };
    if waiting {
        // SAFETY: the child was observed alive in its poll wait above.
        assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) }, 0);
    } else {
        let _ = child.kill();
    }
    wait_child_status(&mut child, Duration::from_secs(3));
    let output = child.wait_with_output().unwrap();
    let received = peer.join().unwrap();
    std::fs::remove_file(script).unwrap();
    assert!(waiting, "client did not enter poll: {:?}", output);
    assert_eq!(output.status.code(), Some(3), "{:?}", output);
    assert_eq!(output.stdout, b"cleanup\n");
    assert_eq!(received.unwrap(), 0);
    root.close().unwrap();
}

#[cfg(target_os = "linux")]
mod line_protocol {
    use super::*;
    use std::os::unix::net::UnixStream;

    const CLIENT: &str = r#"use lib.monotonic

error FrameError = Timeout(remaining_ms: Int) | Oversize(limit: Int) | Closed(pending: Int)

type Frame = {line: Str, pending: Bytes}

proc await_readable(fd: Int, deadline: monotonic.Deadline) [process, fs, time, net, error] -> Result[Unit, Error] {
  let remaining = monotonic.remaining_ms(deadline, monotonic.now()?)
  if remaining == 0 { return Err(FrameError.Timeout(remaining)) }
  let ready = unix.poll_fds([{fd, events: ["readable"]}], timeout: time.millis(remaining))?
  if ready.is_empty() { return Err(FrameError.Timeout(remaining)) }
}

proc read_frame(fd: Int, pending: Bytes, deadline: monotonic.Deadline, limit: Int) [process, fs, time, net, error] -> Result[Frame, Error] {
  var buffer = pending
  loop {
    let endings = regex.find_bytes("\n", buffer)?
    if !endings.is_empty() {
      let end = endings[0].start
      if end > limit { return Err(FrameError.Oversize(limit)) }
      return {line: buffer.slice(0, end).utf8()?, pending: buffer.slice(end + 1)}
    }
    if buffer.len() > limit { return Err(FrameError.Oversize(limit)) }
    await_readable(fd, deadline)?
    let part = unix.read_fd(fd, 7)?
    if part.len() == 0 { return Err(FrameError.Closed(buffer.len())) }
    buffer = bytes.concat([buffer, part])
  }
}

proc kind(result: Result[Frame, Error]) -> Str {
  match result {
    Ok(_) => "ok"
    Err(failure) => match failure {
      FrameError.Timeout {remaining_ms} => "timeout"
      FrameError.Oversize {limit} => "oversize"
      FrameError.Closed {pending} => f"closed:{pending}"
      else => "other"
    }
  }
}

proc open_descriptors() [process, fs, time, net, error] -> Result[Int, Error] {
  Ok([entry.name for entry in fs.children(p"/proc/self/fd")?].len())
}

proc connect(socket: Path) [process, fs, time, net, error] -> Result[Int, Error] {
  unix.connect_stream(socket, 2s)
}

proc deadline_in(budget_ms: Int) [process, fs, time, net, error] -> Result[monotonic.Deadline, Error] {
  Ok(monotonic.deadline_after(monotonic.now()?, budget_ms))
}

proc fragmented_and_coalesced(socket: Path) [process, fs, time, net, error] -> Result[Unit, Error] {
  let fd = connect(socket)?
  defer unix.close_fd(fd)
  let deadline = deadline_in(5000)?
  var pending = b""
  for expected in ["greeting-one", "second", "third", "fourth-after-a-gap"] {
    let frame = read_frame(fd, pending, deadline, 1024)?
    assert frame.line == expected
    pending = frame.pending
  }
  assert pending == b""
  assert kind(read_frame(fd, pending, deadline, 1024)) == "closed:0"
}

proc stalled_peer(socket: Path) [process, fs, time, net, error] -> Result[Unit, Error] {
  let fd = connect(socket)?
  defer unix.close_fd(fd)
  let started = monotonic.now()?
  assert kind(read_frame(fd, b"", deadline_in(300)?, 1024)) == "timeout"
  let elapsed = monotonic.since_ms(started, monotonic.now()?)
  assert elapsed >= 250 and elapsed < 3000
}

proc oversized_peer(socket: Path) [process, fs, time, net, error] -> Result[Unit, Error] {
  let fd = connect(socket)?
  defer unix.close_fd(fd)
  assert kind(read_frame(fd, b"", deadline_in(5000)?, 64)) == "oversize"
}

proc oversized_terminated_frame(socket: Path) [process, fs, time, net, error] -> Result[Unit, Error] {
  let fd = connect(socket)?
  defer unix.close_fd(fd)
  assert kind(read_frame(fd, b"", deadline_in(5000)?, 8)) == "oversize"
}

proc abrupt_close(socket: Path) [process, fs, time, net, error] -> Result[Unit, Error] {
  for _ in [1] {
    let fd = connect(socket)?
    assert kind(read_frame(fd, b"", deadline_in(5000)?, 1024)) == "closed:11"
    unix.close_fd(fd)?
  }
  let before = open_descriptors()?
  for _ in [1, 2, 3] {
    let fd = connect(socket)?
    let outcome = kind(read_frame(fd, b"", deadline_in(5000)?, 1024))
    unix.close_fd(fd)?
    assert outcome == "closed:11"
  }
  assert open_descriptors()? == before
}

proc cancelled_mid_frame(socket: Path, ready: Path) [process, fs, time, net, error] -> Result[Unit, Error] {
  let fd = connect(socket)?
  defer unix.close_fd(fd)
  defer { print "cleanup" }
  let first = read_frame(fd, b"", deadline_in(5000)?, 1024)?
  assert first.line == "partial-sent"
  ready.write("ready")?
  let _ = read_frame(fd, b"", deadline_in(20000)?, 1024)?
  print "unexpected"
}

proc main(...argv: List[Str]) [process, net, time, fs, error] {
  let socket = Path(argv[1])
  match argv[0] {
    "fragmented" => fragmented_and_coalesced(socket)?
    "stalled" => stalled_peer(socket)?
    "oversized" => oversized_peer(socket)?
    "oversized-terminated" => oversized_terminated_frame(socket)?
    "abrupt" => abrupt_close(socket)?
    "cancelled" => cancelled_mid_frame(socket, Path(argv[2]))?
    else => assert false
  }
}
"#;

    fn accept(listener: &UnixListener) -> UnixStream {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    return stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::yield_now(),
                Err(error) => panic!("accept: {error}"),
            }
        }
    }

    fn client_script(root: &Path) -> PathBuf {
        std::os::unix::fs::symlink(Path::new(env!("CARGO_MANIFEST_DIR")).join("core/lib"), root.join("lib")).unwrap();
        let script = root.join("client.xsh");
        std::fs::write(&script, CLIENT).unwrap();
        script
    }

    fn pause() {
        std::thread::sleep(Duration::from_millis(30));
    }

    fn expect_closed(stream: &mut UnixStream) {
        let mut byte = [0; 1];
        match stream.read(&mut byte) {
            Ok(0) => {}
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
            other => panic!("client left the socket open: {other:?}"),
        }
    }

    fn scenario<P>(name: &str, connections: usize, peer: P) -> std::process::Output
    where
        P: Fn(usize, UnixStream) + Send + Clone + 'static,
    {
        let root = tempfile::Builder::new().prefix("l").tempdir_in(".work/tmp").unwrap();
        let path = root.path().join("socket");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let acceptor = std::thread::spawn(move || {
            let handles: Vec<_> = (0..connections).map(|index| {
                let stream = accept(&listener);
                let peer = peer.clone();
                std::thread::spawn(move || peer(index, stream))
            }).collect();
            for handle in handles { handle.join().unwrap(); }
        });
        let script = client_script(root.path());
        let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
            .arg(&script).arg(name).arg(&path).output().unwrap();
        acceptor.join().unwrap();
        root.close().unwrap();
        output
    }

    fn assert_success(output: &std::process::Output) {
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    fn fragmented_and_coalesced_frames_arrive_in_order_then_eof() {
        let output = scenario("fragmented", 1, |_, mut stream| {
            for byte in b"greeting-one\n" {
                stream.write_all(&[*byte]).unwrap();
                pause();
            }
            stream.write_all(b"second\nthird\nfourth").unwrap();
            pause();
            stream.write_all(b"-after-a-gap\n").unwrap();
        });
        assert_success(&output);
    }

    #[test]
    fn stalled_peer_times_out_within_the_deadline_and_the_socket_closes() {
        let output = scenario("stalled", 1, |_, mut stream| {
            stream.write_all(b"half a fra").unwrap();
            expect_closed(&mut stream);
        });
        assert_success(&output);
    }

    #[test]
    fn unterminated_oversized_input_is_refused_and_the_socket_closes() {
        let output = scenario("oversized", 1, |_, mut stream| {
            let chunk = [b'x'; 4096];
            let mut sent = 0usize;
            while sent < 1 << 22 {
                match stream.write(&chunk) {
                    Ok(count) => sent += count,
                    Err(_) => return,
                }
            }
            panic!("client accepted {sent} bytes without a frame boundary");
        });
        assert_success(&output);
    }

    #[test]
    fn oversized_terminated_frame_is_refused() {
        let output = scenario("oversized-terminated", 1, |_, mut stream| {
            stream.write_all(b"much longer than eight bytes\n").unwrap();
            expect_closed(&mut stream);
        });
        assert_success(&output);
    }

    #[test]
    fn abrupt_close_mid_frame_is_reported_and_leaks_no_descriptors() {
        let output = scenario("abrupt", 4, |_, mut stream| {
            stream.write_all(b"{\"partial\":").unwrap();
        });
        assert_success(&output);
    }

    #[test]
    fn sigterm_mid_frame_runs_cleanup_and_closes_the_socket() {
        let root = tempfile::Builder::new().prefix("k").tempdir_in(".work/tmp").unwrap();
        let path = root.path().join("socket");
        let ready = root.path().join("ready");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let peer = std::thread::spawn(move || {
            let mut stream = accept(&listener);
            stream.write_all(b"partial-sent\nrest of a fra").unwrap();
            expect_closed(&mut stream);
        });
        let script = client_script(root.path());
        let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
            .arg(&script).arg("cancelled").arg(&path).arg(&ready)
            .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        wait_for_path(&ready, Duration::from_secs(5), &mut child);
        let tasks = format!("/proc/{}/task", child.id());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !std::fs::read_dir(&tasks).ok().is_some_and(|tasks| tasks.flatten().any(|task| {
            std::fs::read_to_string(task.path().join("syscall")).ok().is_some_and(|line| {
                line.split_whitespace().next().and_then(|number| number.parse::<libc::c_long>().ok()) == Some(libc::SYS_ppoll)
            })
        })) {
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!("client never entered its poll");
            }
            std::thread::yield_now();
        }
        // SAFETY: child.id() identifies the live child observed in its poll above.
        assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) }, 0);
        let status = wait_child_status(&mut child, Duration::from_secs(5));
        let output = child.wait_with_output().unwrap();
        peer.join().unwrap();
        assert_eq!(status.code(), Some(3), "{:?}", output);
        assert_eq!(output.stdout, b"cleanup\n");
        root.close().unwrap();
    }
}
