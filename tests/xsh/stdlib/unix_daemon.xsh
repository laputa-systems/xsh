test test_daemon_start_returns_actual_pre_ready_exit { |ctx|
  let result = test.expect(ctx, r"""let command = process.command_argv("sh", ["sh", "-c", "exit 23"])
let status = unix.daemon_start(command, ["HUP", "TERM"])?
assert status?.exited_with(23) ?? false
let other = process.command_argv("sh", ["sh", "-c", "exit 0"])
let zero = unix.daemon_start(other, [])?
assert zero?.exited_with(0) ?? false
""", status: 0)?
  assert result.stderr == ""
}

test test_daemon_start_acknowledges_after_worker_initialization { |ctx|
  let root = test.temp_dir(ctx)?
  let worker = fp"{root}/worker.xsh"
  worker.write(r"""let fd = (e"NOTIFY_FD" ?? "").parse_int()?
let saved = unix.duplicate_fd(fd)?
unix.close_fd(fd)?
assert fs.cwd()? == Path(args[0])
assert process.session_id()? == args[1].parse_int()?
print "initial scan"
eprint "worker stderr"
io.flush_stdout()?
let _ = process.new_session()?
cd / {
  assert process.session_id()? == process.current_pid()?
  assert unix.write_fd(saved, b"x")? == 1
  unix.close_fd(saved)?
}
""")
  let result = test.expect(ctx, r"""let root = Path(args[0]).resolve()?
let worker = Path(args[1])
let command = process.command_argv(applet.current_exe()?, ["xsh", worker.display(), "--", root.display(), f"{process.session_id()?}"], cwd: root)
print "launch"
assert unix.daemon_start(command, ["HUP", "TERM"])? == null
print "ready"
""", status: 0, args: [root, worker])?
  assert result.stdout == "launch\ninitial scan\nready\n"
  assert result.stderr == "worker stderr\n"
}

test test_signal_capture_retains_distinct_names_and_drains { |ctx|
  let result = test.expect(ctx, r"""unix.signal_setup(["SIGHUP", "term"])?
process.kill(process.current_pid()?, signal: "HUP")?
process.kill(process.current_pid()?, signal: "TERM")?
var seen: List[Str] = []
for _ in range(1000) {
  seen = seen + unix.take_signals()?
  break when seen.len() == 2
  time.sleep(1ms)?
}
assert seen == ["HUP", "TERM"]
assert unix.take_signals()? == []
""", status: 0)?
  assert result.stderr == ""
}

test test_signal_capture_rejects_uncatchable_and_hook_conflicts { |ctx|
  let result = test.expect(ctx, r"""test.error_kind(unix.signal_setup(["KILL"]), "invalid-signal")
test.error_kind(unix.signal_setup(["STOP"]), "invalid-signal")
test.error_kind(unix.signal_setup(["CHLD"]), "invalid-signal")
on SIGHUP [] {}
test.error_kind(unix.signal_setup(["HUP"]), "unix-signal")
""", status: 0)?
  assert result.stderr == ""
}

test test_signal_capture_rejects_hooks_armed_after_setup { |ctx|
  let result = test.expect(ctx, r"""unix.signal_setup(["HUP"])?
on SIGHUP [] {}
""", status: 3, stderr: ["entry-script signal hooks conflict with signal capture"])?
  assert result.stdout == ""
}

test test_daemon_start_rejects_hooks_without_spawning_or_replacing_handlers { |ctx|
  let root = test.temp_dir(ctx)?
  let spawned = fp"{root}/spawned"
  let handled = fp"{root}/handled"
  let result = test.expect(ctx, r"""let spawned = Path(args[0])
let handled = Path(args[1])
on SIGHUP [fs, process, error] {
  handled.write("handled")
  exit 17
}
let command = process.command_argv("sh", ["sh", "-c", "printf spawned > \"$1\"", "sh", spawned.display()])
test.error_kind(unix.daemon_start(command, ["HUP"]), "unix-signal")
process.kill(process.current_pid()?, signal: "HUP")?
exit 99
""", status: 17, args: [spawned, handled])?
  assert ! spawned.exists()?
  assert handled.read_text()? == "handled"
  assert result.stdout == ""
}

test test_take_signals_reports_missing_capture_as_a_result { |ctx|
  let result = test.expect(ctx, r"""test.error_kind(unix.take_signals(), "unix-signal")
print "recovered"
""", status: 0)?
  assert result.stdout == "recovered\n"
}

test test_daemon_start_rejects_discarded_child_status_before_spawning { |ctx|
  let marker = test.temp_path(ctx, name: "must-not-spawn")
  let result = test.expect(ctx, r"""let marker = Path(args[0])
process.set_signal_action("CHLD", "ignore")?
let command = process.command_argv("sh", ["sh", "-c", "printf spawned > \"$1\"", "sh", marker.display()])
test.error_kind(unix.daemon_start(command, ["HUP", "TERM"]), "unix-daemon")
assert process.signal_action("CHLD")? == "ignore"
process.set_signal_action("CHLD", "default")?
print "rejected"
""", status: 0, args: [marker])?
  assert ! marker.exists()?
  assert result.stdout == "rejected\n"
}
