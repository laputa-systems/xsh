test test_signal_table_is_ordered_and_named {
  let table = process.signals()
  assert table[0].name == "EXIT"
  assert table[0].number == 0
  var previous = -1
  for signal in table {
    assert signal.number > previous, f"{signal.name} is out of order"
    previous = signal.number
  }

  assert [s.number for s in table if s.name == "HUP"] == [1]
  assert [s.number for s in table if s.name == "KILL"] == [9]
  assert [s.number for s in table if s.name == "TERM"] == [15]

  assert process.signal("kill")?.number == 9
  assert process.signal("SIGterm")?.number == 15
  assert process.signal("9")?.name == "KILL"
  assert process.signal("0")?.name == "EXIT"
  assert process.signal("exit")?.number == 0
  assert process.signal("POLL")?.name == "IO"
  assert process.signal("SIGIO")?.number == process.signal("IO")?.number
  assert process.signal("CLD")?.number == process.signal("CHLD")?.number
  assert process.signal("IOT")?.name == "ABRT"
  assert process.signal("WINCH")?.number == 28
  test.error_kind(process.signal("NOSUCH"), "invalid-signal")
  test.error_kind(process.signal("129"), "invalid-signal")
  test.error_kind(process.signal(""), "invalid-signal")
}

test test_realtime_signals_are_named_by_offset {
  let table = process.signals()
  let last = table[-1]
  if last.name != "RTMAX" {
    test.skip("the host has no real-time signals")
  }

  let rtmin = process.signal("RTMIN")?
  assert last.number > rtmin.number
  assert process.signal("SIGRTMIN")?.number == rtmin.number
  assert process.signal("rtmax")?.number == last.number
  assert process.signal("SIGRTMIN+3")?.number == rtmin.number + 3
  assert process.signal("RTMIN+3")?.name == "RTMIN+3"
  assert process.signal("RTMAX-2")?.number == last.number - 2
  assert process.signal(f"{rtmin.number + 1}")?.name == "RTMIN+1"
  test.error_kind(process.signal("RTMAX+1"), "invalid-signal")
  test.error_kind(process.signal("RTMIN-1"), "invalid-signal")
  test.error_kind(process.signal("RTMIN+999"), "invalid-signal")
  test.error_kind(process.signal("RTMIN+"), "invalid-signal")
  # Numbers the table leaves unnamed keep their number as the name.
  assert process.signal("32")?.name == "32"
}

test test_process_identity_reads {
  let pid = process.current_pid()?
  assert process.parent_pid()? > 0
  assert process.parent_pid()? != pid
  assert process.group_id()? > 0
  assert process.group_id(pid)? == process.group_id()?
  assert process.session_id()? > 0
  assert process.session_id(pid)? == process.session_id()?

  let missing = process.group_id(2000000000)
  test.error_kind(missing, "process-group-id")
  if let Err(failure) = missing {
    assert failure.errno == 3
  }

  test.error_kind(process.session_id(-5), "pid-range")
}

test test_children_lead_their_own_process_group_and_can_be_signaled_as_a_group {
  let handle = spawn run sleep 30 ?
  assert process.group_id(handle.pid)? == handle.pid
  assert process.session_id(handle.pid)? == process.session_id()?
  process.kill_group(handle.pid, "0")

  # An exec'd child can no longer be moved into another group (EACCES).
  let moved = process.set_group_id(handle.pid, process.group_id()?)
  test.error_kind(moved, "process-set-group-id")
  if let Err(failure) = moved {
    assert failure.errno == 13
  }

  process.kill_group(handle.pid)
  let status = wait handle?
  assert status.signaled()
  assert status.signal_number()? == 15
  assert status.shell_code()? == 143
}

test test_kill_group_reports_a_missing_group_and_a_bad_signal {
  let missing = process.kill_group(2000000000, "TERM")
  test.error_kind(missing, "process-missing")
  if let Err(failure) = missing {
    assert failure.errno == 3
  }

  test.error_kind(process.kill_group(0), "pid-range")
  test.error_kind(process.kill_group(-3), "pid-range")
  test.error_kind(process.kill_group(1, "NOSUCH"), "invalid-signal")
}

test test_new_sessions_report_their_leader {
  let plan = process.command_argv("sleep", ["sleep", "30"], new_session: true)
  let child = process.spawn(plan)?
  assert process.session_id(child.pid)? == child.pid
  assert process.group_id(child.pid)? == child.pid
  process.kill(child.pid, signal: "KILL")
}

test test_a_group_leader_cannot_start_a_session { |ctx|
  let output = test.run_xsh(
    ctx,
    """
match process.new_session() {
  Err(failure) => print f"errno={failure.errno ?? -1}"
  Ok(_) => print "started"
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "errno=1\n"
}

test test_shell_code_reports_exits_and_signal_deaths {
  let ok = spawn run sh -c "exit 7" ?
  assert (wait ok?).shell_code()? == 7
  let fine = spawn run true ?
  assert (wait fine?).shell_code()? == 0
  let killed = spawn run sh -c "kill -KILL $$" ?
  let status = wait killed?
  assert status.signaled()
  assert status.shell_code()? == 137
}

test test_wait_timeout_returns_null_until_a_child_finishes {
  let handle = spawn run sleep 30 ?
  let early = process.wait_timeout([handle], 50ms)?
  assert early == null
  let again = process.wait_timeout([handle], 0ms)?
  assert again == null

  process.kill(handle.pid, signal: "TERM")
  let done = process.wait_timeout([handle], 10s)?
  if let finished = done {
    assert finished.index == 0
    assert finished.pid == handle.pid
    assert finished.status.signaled()
    assert finished.status.signal_number()? == 15
  } else {
    test.fail("the terminated child was not reported")
  }

  let quick = spawn run true ?
  let completed = process.wait_timeout([quick], 10s)?
  if let finished = completed {
    assert finished.status.exited_with(0)
  } else {
    test.fail("the finished child was not reported")
  }
}

test test_priority_and_nice_change_only_the_calling_process { |ctx|
  let output = test.run_xsh(
    ctx,
    """
let before = process.priority()?
assert process.priority(0, "process")? == before
assert process.priority(process.current_pid()?)? == before
assert process.priority(0, "group")? == before
assert process.priority(0, "user")? <= before
let want = if before + 3 > 19 { 19 } else { before + 3 }
assert process.nice(3)? == want
assert process.priority()? == want
let target = if want < 19 { want + 1 } else { want }
process.set_priority(0, target)?
assert process.priority()? == target
process.set_priority(process.current_pid()?, target, which: "process")?
match process.priority(0, "bogus") {
  Err(failure) => print f"which={failure.errno ?? -1}"
  Ok(_) => print "accepted"
}
match process.priority(2000000000) {
  Err(failure) => print f"missing={failure.errno ?? -1}"
  Ok(_) => print "found"
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "which=-1\nmissing=3\n"
  assert process.priority()? == process.priority(0, "process")?
}

test test_rlimits_are_read_and_lowered_by_name { |ctx|
  let all = process.rlimits()?
  for name in ["cpu", "fsize", "data", "stack", "core", "nofile", "as"] {
    assert [l.resource for l in all if l.resource == name] == [name]
  }

  let nofile = process.rlimit("nofile")?
  assert nofile.resource == "nofile"
  assert (nofile.soft ?? 1000000000) <= (nofile.hard ?? 1000000000) or nofile.hard == null
  test.error_kind(process.rlimit("nosuch"), "invalid-argument")

  let output = test.run_xsh(
    ctx,
    """
process.set_rlimit("fsize", soft: 4096, hard: 8192)?
let both = process.rlimit("fsize")?
assert both.soft == 4096 and both.hard == 8192
process.set_rlimit("fsize", hard: 2048)?
let clamped = process.rlimit("fsize")?
assert clamped.hard == 2048 and clamped.soft == 2048
process.set_rlimit("fsize", soft: 1024)?
assert process.rlimit("fsize")?.soft == 1024
assert process.rlimit("fsize")?.hard == 2048
match process.set_rlimit("fsize", soft: 999999) {
  Err(failure) => print f"soft-above-hard={failure.errno ?? -1}"
  Ok(_) => print "accepted"
}
match process.set_rlimit("fsize", soft: -1) {
  Err(failure) => print f"negative={failure.errno ?? -1}"
  Ok(_) => print "accepted"
}
process.set_rlimit("core", soft: null)?
assert process.rlimit("core")?.soft == process.rlimit("core")?.hard
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "soft-above-hard=22\nnegative=-1\n"
}

test test_signal_actions_survive_exec { |ctx|
  let output = test.run_xsh(
    ctx,
    """
assert process.signal_action("USR1")? == "default"
process.set_signal_action("USR1", "ignore")?
assert process.signal_action("USR1")? == "ignore"
process.set_signal_action("USR1", "default")?
assert process.signal_action("USR1")? == "default"
test_invalid_action()
process.set_signal_action("USR2", "ignore")?
unix.exec(process.command {
  run sh -c "kill -USR2 $$; echo survived"
})?

proc test_invalid_action() [io] {
  match process.set_signal_action("USR1", "catch") {
    Err(failure) => print f"action={failure.errno ?? -1}"
    Ok(_) => print "accepted"
  }
  match process.set_signal_action("KILL", "ignore") {
    Err(failure) => print f"kill={failure.errno ?? -1}"
    Ok(_) => print "accepted"
  }
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "action=-1\nkill=22\nsurvived\n"
}

test test_flush_stdout_reports_a_closed_pipe_unless_sigpipe_is_default { |ctx|
  let root = test.temp_dir(ctx, name: "flush")?
  let body = """io.write_stdout_bytes(bytes.zero(1000000)?)
match io.flush_stdout() {
  Err(failure) => {
    if failure.errno == 32 { exit 41 }
    exit 40
  }
  Ok(_) => { exit 0 }
}
"""
  fp"{root}/plain.xsh".write(body)
  fp"{root}/default.xsh".write("process.set_signal_action(\"PIPE\", \"default\")?\n" + body)
  let pipeline = """{ "$0" "$1"; echo "status=$?" >&2; } | head -c1 >/dev/null"""
  let output = test.run_xsh(
    ctx,
    r"""
let dir = e"WRITER_DIR"?
let pipeline = e"PIPELINE"?
let xsh = applet.current_exe()?
for name in ["plain", "default"] {
  let script = fp"{dir}/{name}.xsh"
  let captured = run.capture --text sh -c $pipeline $xsh $script ?
  print f"{name} {captured.stderr.trim()}"
}
""",
    env: {WRITER_DIR: root, PIPELINE: pipeline},
  )?
  assert output.success, output.stderr
  assert output.stdout == "plain status=41\ndefault status=141\n"
}

test test_flush_stdout_is_a_no_op_for_captured_output {
  io.write_stdout("captured")
  io.flush_stdout()
}
