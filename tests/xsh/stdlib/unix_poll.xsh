test test_unix_poll_fds_reports_only_ready_entries_in_input_order { |ctx|
  let result = test.expect(ctx, r"""tempdir root {
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  fs.mkfifo(first, mode: 384)
  fs.mkfifo(second, mode: 384)
  let first_reader = unix.open_fd(first, nonblock: true)?
  let second_reader = unix.open_fd(second, nonblock: true)?
  let first_writer = unix.open_fd(first, write: true, nonblock: true)?
  let second_writer = unix.open_fd(second, write: true, nonblock: true)?
  defer { unix.close_fd(first_reader); unix.close_fd(second_reader); unix.close_fd(first_writer); unix.close_fd(second_writer) }
  let entries = [{fd: first_reader, events: ["readable"]}, {fd: second_reader, events: ["readable"]}]
  assert unix.poll_fds(entries, 5ms)? == []
  let _ = unix.write_fd(second_writer, b"x")?
  assert unix.poll_fds(entries, 1s)? == [{fd: second_reader, events: ["readable"]}]
  let _ = unix.write_fd(first_writer, b"y")?
  assert unix.poll_fds(entries, 1s)? == entries
  assert unix.read_fd(first_reader, 8)? == b"y"
  assert unix.poll_fds(entries, 0ms)? == [{fd: second_reader, events: ["readable"]}]
  assert unix.poll_fds([{fd: first_writer, events: ["writable"]}, {fd: first_reader, events: []}], 0ms)? == [{fd: first_writer, events: ["writable"]}]
}
""", status: 0)?
  assert result.stdout == "" and result.stderr == ""
}

test test_unix_poll_fds_reports_hangup_and_invalid_descriptors { |ctx|
  let result = test.expect(ctx, r"""tempdir root {
  let fifo = fp"{root}/pipe"
  fs.mkfifo(fifo, mode: 384)
  let reader = unix.open_fd(fifo, nonblock: true)?
  {
    let writer = unix.open_fd(fifo, write: true, nonblock: true)?
    unix.close_fd(writer)
  }
  defer unix.close_fd(reader)
  let seen = unix.poll_fds([{fd: reader, events: ["readable"]}, {fd: 2147483647, events: ["readable", "writable"]}], 1s)?
  assert seen[0].fd == reader and "hangup" in seen[0].events
  assert seen[1] == {fd: 2147483647, events: ["invalid"]}
  assert seen.len() == 2
}
""", status: 0)?
  assert result.stdout == "" and result.stderr == ""
}

test test_unix_poll_fds_validates_requests {
  test.error_kind(unix.poll_fds([{fd: -1, events: ["readable"]}]), "unix-poll-fds")
  test.error_kind(unix.poll_fds([{fd: 2147483648, events: ["readable"]}]), "unix-poll-fds")
  test.error_kind(unix.poll_fds([{fd: 0, events: ["unknown"]}]), "unix-poll-fds")
  test.error_kind(unix.poll_fds([{fd: 0, events: ["hangup"]}]), "unix-poll-fds")
  test.error_kind(unix.poll_fds([{fd: 0, events: []}, {fd: 0, events: ["readable"]}]), "unix-poll-fds")
}

test test_unix_poll_fds_admits_64_descriptors_and_rejects_65 {
  let admitted: List[UnixPollEntry] = [{fd: 100 + n, events: []} for n in range(64)]
  assert unix.poll_fds(admitted, 0ms)?.len() == 64
  let oversized: List[UnixPollEntry] = [{fd: 100 + n, events: []} for n in range(65)]
  test.error_kind(unix.poll_fds(oversized, 0ms), "unix-poll-fds")
}

test test_unix_poll_fds_zero_timeout_never_waits { |ctx|
  let result = test.expect(ctx, r"""let started = time.monotonic()?
for _ in range(200) { let _ = unix.poll_fds([{fd: 0, events: []}], 0ms)? }
assert unix.poll_fds([], 0ms)? == []
assert time.monotonic()? - started < 1000000000
""", status: 0)?
  assert result.stdout == "" and result.stderr == ""
}

test test_unix_poll_fds_timeout_elapses_before_returning_empty { |ctx|
  let result = test.expect(ctx, r"""tempdir root {
  let fifo = fp"{root}/pipe"
  fs.mkfifo(fifo, mode: 384)
  let reader = unix.open_fd(fifo, nonblock: true)?
  let writer = unix.open_fd(fifo, write: true, nonblock: true)?
  defer { unix.close_fd(reader); unix.close_fd(writer) }
  let started = time.monotonic()?
  assert unix.poll_fds([{fd: reader, events: ["readable"]}], 120ms)? == []
  assert time.monotonic()? - started >= 120000000
}
""", status: 0)?
  assert result.stdout == "" and result.stderr == ""
}

test test_unix_poll_fds_signal_ends_an_indefinite_wait_with_the_shutdown_status { |ctx|
  let result = test.expect(ctx, r"""defer { print "cleanup" }
let me = process.current_pid()?
let signaller = spawn run sh -c "sleep 0.2; kill -TERM \"$1\"" sh $me ?
let seen = unix.poll_fds([], null)?
print "unreachable ${seen.len()}"
""", status: 3, stdout: ["cleanup"])?
  assert "unreachable" not in result.stdout
}
