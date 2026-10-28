use support.syslog_service as svc

test test_readiness_never_precedes_the_published_socket { |ctx|
  for round in range(5) {
    let lay = svc.layout(ctx, f"ready-{round}")?
    let launch = svc.prepare(ctx, lay)?
    let handle_command = launch.command
  let handle = spawn handle_command?
    svc.attach(launch)
    svc.await_ready(launch)
    svc.signal(lay, "TERM")?
    let status = wait handle?
    assert status.exited_with(0), svc.log_text(lay)?
    svc.finish(launch)
  }
}

test test_the_start_record_states_the_limits_and_that_kernel_loss_is_unobservable { |ctx|
  let lay = svc.layout(ctx, "start-record")?
  let launch = svc.prepare(ctx, lay)?
  let handle_command = launch.command
  let handle = spawn handle_command?
  svc.attach(launch)
  svc.await_ready(launch)
  let start = svc.await_lines(lay, "event=start", 1)?[0]
  for field in ["pid=", "boot=\"", "datagram_limit=8192", "batch=16", "receive_buffer=262144", "kernel_loss=unobservable", "sender_rate=", "global_burst="] {
    assert field in start, start
  }
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
}

test test_a_record_separates_the_kernel_sender_from_what_the_sender_claims { |ctx|
  let lay = svc.layout(ctx, "separation")?
  let launch = svc.prepare(ctx, lay)?
  let handle_command = launch.command
  let handle = spawn handle_command?
  svc.attach(launch)
  svc.await_ready(launch)
  let me = process.current_pid()?
  let sent = svc.send(lay.socket, b"<13>Oct 28 10:11:12 otherhost forged[999]: hello")?
  assert sent == 48
  let line = svc.await_lines(lay, "seq=", 1)?[0]
  assert f"pid={me} " in line, line
  assert f"uid={unix.id()?.uid} " in line, line
  assert "tag=\"forged\" tag_pid=999 host=\"otherhost\"" in line, line
  assert line.ends_with("msg=\"hello\""), line
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
}

test test_stop_while_idle_exits_zero_prints_final_counters_and_removes_only_its_own_socket { |ctx|
  let lay = svc.layout(ctx, "idle-stop")?
  let launch = svc.prepare(ctx, lay)?
  let handle_command = launch.command
  let handle = spawn handle_command?
  svc.attach(launch)
  svc.await_ready(launch)
  svc.signal(lay, "TERM")?
  let status = wait handle?
  assert status.exited_with(0)
  assert ! lay.socket.exists()?
  let stop = svc.lines_with(lay, "event=stop")?
  assert stop.len() == 1 and "offered=0 accepted=0" in stop[0] and "kernel_loss=unobservable" in stop[0]
}

test test_a_live_competing_receiver_makes_the_second_start_fail_without_touching_the_first { |ctx|
  let lay = svc.layout(ctx, "competing")?
  let first = svc.prepare(ctx, lay)?
  let handle_command = first.command
  let handle = spawn handle_command?
  svc.attach(first)
  svc.await_ready(first)
  let before = fs.stat(lay.socket)?
  let other = svc.layout(ctx, "competing-second")?
  let rival = {...other, socket: lay.socket}
  let second = svc.prepare(ctx, rival)?
  let rival_handle_command = second.command
  let rival_handle = spawn rival_handle_command?
  svc.attach(second)
  assert (wait rival_handle?).shell_code()? == 1
  assert "another receiver is listening" in svc.log_text(rival)?, svc.log_text(rival)?
  let after = fs.stat(lay.socket)?
  assert after.ino == before.ino and after.dev == before.dev
  assert svc.send(lay.socket, b"<13>still up")? > 0
  let _ = svc.await_lines(lay, "seq=", 1)?
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
}

test test_a_second_service_on_the_same_state_directory_is_refused_by_the_lock { |ctx|
  let lay = svc.layout(ctx, "lock")?
  let first = svc.prepare(ctx, lay)?
  let handle_command = first.command
  let handle = spawn handle_command?
  svc.attach(first)
  svc.await_ready(first)
  let other = svc.layout(ctx, "lock-other")?
  let contender = {...other, state: lay.state}
  let second = svc.prepare(ctx, contender)?
  let rival_handle_command = second.command
  let rival_handle = spawn rival_handle_command?
  svc.attach(second)
  assert (wait rival_handle?).shell_code()? == 1
  assert "another receiver holds" in svc.log_text(contender)?
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
}

test test_a_restart_replaces_the_stale_socket_of_a_killed_predecessor { |ctx|
  let lay = svc.layout(ctx, "stale")?
  let first = svc.prepare(ctx, lay)?
  let handle_command = first.command
  let handle = spawn handle_command?
  svc.attach(first)
  svc.await_ready(first)
  let pid = svc.reported_pid(lay)?
  process.kill(pid, signal: "KILL")
  assert (wait handle?).signaled()
  assert lay.socket.exists()?
  let again = svc.prepare(ctx, lay)?
  let restarted_command = again.command
  let restarted = spawn restarted_command?
  svc.attach(again)
  svc.await_ready(again)
  assert svc.send(lay.socket, b"<13>after restart")? > 0
  let _ = svc.await_lines(lay, "seq=", 1)?
  svc.signal(lay, "TERM")?
  assert (wait restarted?).exited_with(0)
}

test test_a_foreign_entry_at_the_socket_path_stops_the_service_and_is_left_untouched { |ctx|
  let lay = svc.layout(ctx, "foreign")?
  lay.socket.write(b"precious")
  let launch = svc.prepare(ctx, lay)?
  let handle_command = launch.command
  let handle = spawn handle_command?
  svc.attach(launch)
  assert (wait handle?).shell_code()? == 1
  assert lay.socket.read_bytes()? == b"precious"
  assert "not a socket" in svc.log_text(lay)?, svc.log_text(lay)?
  lay.socket.remove()
  let target = fp"{lay.root}/elsewhere"
  target.write(b"data")
  lay.socket.symlink(to: target)
  let again = svc.prepare(ctx, lay)?
  let second_command = again.command
  let second = spawn second_command?
  svc.attach(again)
  assert (wait second?).shell_code()? == 1
  assert target.read_bytes()? == b"data"
}

test test_an_unsafe_socket_directory_is_refused { |ctx|
  let lay = svc.layout(ctx, "unsafe")?
  lay.dev.chmod(0o775)
  let launch = svc.prepare(ctx, lay)?
  let handle_command = launch.command
  let handle = spawn handle_command?
  svc.attach(launch)
  assert (wait handle?).shell_code()? == 1
  assert "writable by no one else" in svc.log_text(lay)?, svc.log_text(lay)?
  assert ! lay.socket.exists()?
}

proc boot(ctx: TestContext, lay: svc.Layout, wrapper: Str = "") [fs, process, net, time, error] -> Result[svc.Prepared, Error] {
  svc.prepare(ctx, lay, wrapper)
}

test test_unexpected_descriptors_are_closed_counted_and_never_leak { |ctx|
  let lay = svc.layout(ctx, "rights")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let pid = svc.reported_pid(lay)?
  let before = svc.open_descriptors(pid)?
  let sent = svc.send_with(ctx, lay, "rights", ["50", "3"])?
  assert sent.sent == 50
  assert svc.send(lay.socket, b"<13>sentinel")? > 0
  let lines = svc.await_lines(lay, "seq=", 1)?
  assert "msg=\"sentinel\"" in lines[0]
  assert svc.open_descriptors(pid)? == before
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
  let stop = svc.lines_with(lay, "event=stop")?[0]
  assert svc.counter(stop, "dropped_rights")? == 50, stop
  assert svc.counter(stop, "accepted")? == 1 and svc.counter(stop, "offered")? == 51, stop
}

test test_descriptor_exhaustion_leaves_the_service_running_without_leaking { |ctx|
  let lay = svc.layout(ctx, "exhaustion")?
  let launch = boot(ctx, lay, "ulimit -n 48")?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let pid = svc.reported_pid(lay)?
  let before = svc.open_descriptors(pid)?
  let sent = svc.send_with(ctx, lay, "rights", ["20", "100"])?
  assert sent.sent == 20
  assert svc.send(lay.socket, b"<13>after exhaustion")? > 0
  let lines = svc.await_lines(lay, "seq=", 1)?
  assert "msg=\"after exhaustion\"" in lines[0]
  assert svc.open_descriptors(pid)? == before
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
  let stop = svc.lines_with(lay, "event=stop")?[0]
  assert svc.counter(stop, "dropped_rights")? + svc.counter(stop, "dropped_control_truncated")? == 20, stop
}

test test_empty_oversize_and_exact_limit_datagrams_are_told_apart { |ctx|
  let lay = svc.layout(ctx, "sizes")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  assert svc.send(lay.socket, b"")? == 0
  assert svc.send(lay.socket, bytes.concat([b"<13>", bytes.zero(8188)?]))? == 8192
  assert svc.send(lay.socket, bytes.concat([b"<13>", bytes.zero(8189)?]))? == 8193
  assert svc.send(lay.socket, bytes.zero(65536)?)? == 65536
  assert svc.send(lay.socket, b"<13>sentinel")? > 0
  let lines = svc.await_lines(lay, "seq=", 3)?
  assert "priority=missing" in lines[0] and "msg=\"\"" in lines[0], lines[0]
  assert "payload_base64=" in lines[1] and " cut=" in lines[1], lines[1]
  assert "msg=\"sentinel\"" in lines[2]
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
  let stop = svc.lines_with(lay, "event=stop")?[0]
  assert svc.counter(stop, "dropped_oversize")? == 2, stop
  assert svc.counter(stop, "accepted")? == 3, stop
}

test test_malformed_binary_multiline_and_cut_payloads_each_render_one_line { |ctx|
  let lay = svc.layout(ctx, "payloads")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let payloads: List[Bytes] = [
    b"<>", b"<13", b"<999>bad priority", b"\xff\xfe\x00\x01 binary", b"<13>multi\nline\r\nmessage\n\n",
    bytes.from_text("<13>cut: " + ["0123456789" for _ in range(300)].join("")),
    b"<13>quote \" and backslash \\ and tab \t", b"<13>seq=999 uid=0 forged\nseq=1000 uid=0 forged",
  ]
  for payload in payloads { assert svc.send(lay.socket, payload)? == payload.len() }
  let lines = svc.await_lines(lay, "seq=", payloads.len())?
  assert lines.len() == payloads.len()
  let text = svc.log_text(lay)?
  assert text.lines().len() == payloads.len() + 1, text
  for index in range(payloads.len()) {
    assert lines[index].starts_with(f"seq={index + 1} "), lines[index]
  }
  assert "cut=952" in lines[5], lines[5]
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
}

proc spawn_sender(ctx: TestContext, lay: svc.Layout, tag: Str, scenario: Str, args: List[Str]) [fs, process, error] -> Result[Command, Error] {
  let root = fp"{lay.root}/{tag}"
  root.mkdir()
  Ok(svc.sender_command(ctx, root, [scenario, lay.socket.display()].extend(args)))
}

test test_concurrent_senders_are_all_recorded_in_order_with_their_own_kernel_pids { |ctx|
  let lay = svc.layout(ctx, "concurrent")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let first = spawn_sender(ctx, lay, "a", "numbered", ["100"])?
  let second = spawn_sender(ctx, lay, "b", "numbered", ["100"])?
  let third = spawn_sender(ctx, lay, "c", "numbered", ["100"])?
  let sender_a = spawn first?
  let sender_b = spawn second?
  let sender_c = spawn third?
  assert (wait sender_a?).exited_with(0) and (wait sender_b?).exited_with(0) and (wait sender_c?).exited_with(0)
  let lines = svc.await_lines(lay, "seq=", 300)?
  assert lines.len() == 300
  var last_by_pid: Map[Str, Int] = {}
  for index in range(300) {
    assert lines[index].starts_with(f"seq={index + 1} "), lines[index]
    let fields = rx" pid=([0-9]+) .* tag=\x22numbered\x22 tag_pid=([0-9]+) .* msg=\x22n([0-9]+)\x22$".captures(lines[index])
    assert ! fields.is_empty(), lines[index]
    assert fields[1] == fields[2], "kernel pid and self-reported pid differ for an honest sender"
    let number = fields[3].parse_int()?
    assert number == (last_by_pid.get(fields[1]) ?? -1) + 1, lines[index]
    last_by_pid[fields[1]] = number
  }
  assert last_by_pid.len() == 3
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
  let stop = svc.lines_with(lay, "event=stop")?[0]
  assert svc.counter(stop, "offered")? == 300 and svc.counter(stop, "accepted")? == 300, stop
}

test test_a_flood_is_bounded_counted_and_leaves_the_service_responsive { |ctx|
  let lay = svc.layout(ctx, "flood")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let pid = svc.reported_pid(lay)?
  var delivered = 0
  for round in range(3) {
    let result = svc.send_with(ctx, lay, "flood", ["1500", "200"])?
    delivered += result.sent
  }
  var sent = 0
  var queue_full = 0
  let burst = svc.send_with(ctx, lay, "flood", ["3000", "200", "nonblocking"])?
  sent = burst.sent
  queue_full = burst.again
  assert delivered == 4500
  assert sent + queue_full == 3000 and queue_full > 0
  assert svc.send(lay.socket, b"<13>after the flood")? > 0
  let _ = svc.await_containing(lay, "msg=\"after the flood\"", 1)?
  let status = fp"/proc/{pid}/status".read_text()?
  let peak = rx"VmHWM:\s+([0-9]+) kB".captures(status)[1].parse_int()?
  assert peak < 400000, f"peak resident set {peak} kB"
  assert svc.open_descriptors(pid)? < 20
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
  let stop = svc.lines_with(lay, "event=stop")?[0]
  let offered = svc.counter(stop, "offered")?
  let accepted = svc.counter(stop, "accepted")?
  assert offered >= 4500 and offered <= 4500 + sent + 1, stop
  assert accepted >= 600, stop
  assert accepted + svc.counter(stop, "dropped_rate")? == offered, stop
  assert svc.counter(stop, "dropped_rate")? > 0, stop
  assert svc.counter(stop, "senders")? <= 256
}

test test_a_termination_signal_during_a_flood_stops_the_service_promptly { |ctx|
  let lay = svc.layout(ctx, "term-load")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let flood = spawn_sender(ctx, lay, "f", "flood", ["60000", "300"])?
  let flooder = spawn flood?
  let _ = svc.await_lines(lay, "seq=", 20)?
  let started = time.monotonic()?
  svc.signal(lay, "TERM")?
  let status = wait handle?
  let elapsed = time.monotonic()? - started
  assert status.exited_with(0), svc.log_text(lay)?
  assert elapsed < 8000000000, f"stopping took {elapsed} ns"
  assert ! lay.socket.exists()?
  let stop = svc.lines_with(lay, "event=stop")?
  assert stop.len() == 1
  let _ = wait flooder?
}

test test_a_full_pipe_never_stalls_the_receive_loop_and_is_counted { |ctx|
  let lay = svc.layout(ctx, "full-pipe")?
  let fifo = fp"{lay.root}/sink.fifo"
  fs.mkfifo(fifo, 0o600)
  let drain = unix.open_fd(fifo, nonblock: true)?
  defer unix.close_fd(drain)
  let errors = fp"{lay.root}/stderr.log"
  let launch = svc.prepare(ctx, lay, f"exec 1<>{fifo.display()}", stderr_file: errors)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  assert "readable" in unix.poll_fd(drain, ["readable"], timeout_ms: 30000)?
  let started = unix.read_fd(drain, 4096)?.utf8()?
  let pid = rx"pid=([0-9]+)".captures(started)[1].parse_int()?
  for round in range(3) {
    let result = svc.send_with(ctx, lay, "flood", ["150", "1500"])?
    assert result.sent == 150
  }
  assert svc.send(lay.socket, b"<13>still receiving")? > 0
  time.sleep(500ms)?
  process.kill(pid, signal: "TERM")
  let status = wait handle?
  assert status.exited_with(0), errors.read_text()?
  let finale = [line for line in errors.read_text()?.lines() if line.starts_with("event=stop")]
  assert finale.len() == 1, errors.read_text()?
  assert svc.counter(finale[0], "dropped_sink_blocked")? > 0, finale[0]
}

test test_an_unwritable_sink_is_counted_reported_once_a_second_and_does_not_grow_memory { |ctx|
  let lay = svc.layout(ctx, "dev-full")?
  let errors = fp"{lay.root}/stderr.log"
  let launch = svc.prepare(ctx, lay, "exec 1>/dev/full", stderr_file: errors)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let announced = svc.await_containing_in(errors, "event=start", 1)?[0]
  let pid = rx"pid=([0-9]+)".captures(announced)[1].parse_int()?
  let resident = rx"VmRSS:\s+([0-9]+) kB"
  let before = resident.captures(fp"/proc/{pid}/status".read_text()?)[1].parse_int()?
  let started = time.monotonic()?
  for round in range(8) {
    let result = svc.send_with(ctx, lay, "flood", ["150", "800"])?
    assert result.sent == 150
  }
  assert svc.send(lay.socket, b"<13>sentinel")? > 0
  time.sleep(500ms)?
  let seconds = (time.monotonic()? - started) / 1000000000
  let found = [line for line in errors.read_text()?.lines() if line.starts_with("syslogd: output failed")]
  assert found.len() >= 1 and found.len() <= seconds + 2, f"{found.len()} reports in {seconds} s"
  let after = resident.captures(fp"/proc/{pid}/status".read_text()?)[1].parse_int()?
  assert after < before + 30000, f"resident set grew from {before} kB to {after} kB"
  handle.cancel("TERM", kill_after: 10s)?
  let finale = [line for line in errors.read_text()?.lines() if line.starts_with("event=stop")]
  assert finale.len() == 1, errors.read_text()?
  assert svc.counter(finale[0], "dropped_sink_error")? > 0, finale[0]
}

test test_copy_truncate_rotation_of_the_append_only_log_is_tolerated { |ctx|
  let lay = svc.layout(ctx, "truncate")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let pid = svc.reported_pid(lay)?
  assert svc.send(lay.socket, b"<13>before rotation")? > 0
  let _ = svc.await_containing(lay, "before rotation", 1)?
  lay.log.truncate(0)
  assert svc.send(lay.socket, b"<13>after rotation")? > 0
  let found = svc.await_containing(lay, "after rotation", 1)?
  let text = svc.log_text(lay)?
  assert text.lines().len() == 1 and "before rotation" not in text and "\0" not in text, text
  process.kill(pid, signal: "TERM")
  assert (wait handle?).exited_with(0)
}

test test_a_renamed_log_is_followed_and_reported_once { |ctx|
  let lay = svc.layout(ctx, "renamed")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  let pid = svc.reported_pid(lay)?
  assert svc.send(lay.socket, b"<13>first")? > 0
  let _ = svc.await_containing(lay, "msg=\"first\"", 1)?
  let moved = fp"{lay.root}/syslogd.log.1"
  fs.rename_noreplace(lay.log, moved)
  time.sleep(1200ms)?
  assert svc.send(lay.socket, b"<13>second")? > 0
  let _ = svc.await_containing_in(moved, "msg=\"second\"", 1)?
  let after = moved.read_text()?
  assert after.lines().len() >= 3 and "event=sink-moved" in after, after
  assert [line for line in after.lines() if line.starts_with("event=sink-moved")].len() == 1
  process.kill(pid, signal: "TERM")
  assert (wait handle?).exited_with(0)
}

test test_a_removed_log_stops_the_service_so_a_supervisor_can_reopen_it { |ctx|
  let lay = svc.layout(ctx, "removed")?
  let errors = fp"{lay.root}/stderr.log"
  let launch = svc.prepare(ctx, lay, stderr_file: errors)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  assert svc.send(lay.socket, b"<13>first")? > 0
  let _ = svc.await_containing(lay, "msg=\"first\"", 1)?
  lay.log.remove()
  time.sleep(1200ms)?
  assert svc.send(lay.socket, b"<13>second")? > 0
  let status = wait handle?
  assert status.shell_code()? == 1
  assert "the output file was removed" in errors.read_text()?
  assert ! lay.socket.exists()?
}

test test_the_interrupt_signal_stops_the_service_like_a_termination_signal { |ctx|
  let lay = svc.layout(ctx, "interrupt")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  svc.signal(lay, "INT")?
  assert (wait handle?).exited_with(0)
  assert ! lay.socket.exists()?
}

test test_a_hangup_from_a_supervisor_reload_leaves_the_service_running { |ctx|
  let lay = svc.layout(ctx, "reload")?
  let launch = boot(ctx, lay)?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  svc.signal(lay, "HUP")?
  assert svc.send(lay.socket, b"<13>after reload")? > 0
  let _ = svc.await_containing(lay, "msg=\"after reload\"", 1)?
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
}
