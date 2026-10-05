# Script prelude: starts `child` in a process group of its own and waits until
# it reports, through the file named by the first argument, that it is running.
# The child then sleeps until it is signaled.
const ready_sleeper = r"""let marker = Path(args[0])
let sleeper = process.command_argv("sh", ["sh", "-c", "printf ready > $1; exec sleep 30", "sh", marker.display()])
let child = unix.spawn_process_group(sleeper)?
var ready_tries = 0
while ! marker.exists()? and ready_tries < 300 {
  time.sleep(10ms)?
  ready_tries += 1
}
"""

type FakeChildEvent = {pid: Int, status: Status}

test test_unix_fake_covers_module_surface { |ctx|
  let root = test.temp_dir(ctx, name: "unix")?
  let log = fp"{root}/unix.jsonl"
  let command = process.command_argv("demo", ["demo", "arg"])

  test.unix_fake(ctx, {signal: "USR1", log: log})
  assert unix.reap_child_events()?.collect().is_empty()
  unix.pid1_setup(["TERM"], subreaper: true, allow_non_pid1: true)
  let event = unix.wait_pid1_event()?
  assert event.kind == "signal"
  assert event.signal == "USR1"
  let shutdown = unix.shutdown_process_groups([1000], 1ms, kill_timeout: 1ms)?
  assert shutdown.term_sent >= 0
  assert unix.tty()? == "/dev/tty"
  assert unix.id()?.groups[0].name == "root"
  let attrs = unix.tty_attrs()?
  assert attrs.raw
  unix.set_tty_attrs(attrs)
  unix.set_hostname("xsh")
  let child = unix.spawn_process_group(command)?
  let notify_child = unix.spawn_process_group(command, notify: true)?
  assert notify_child.notify_fd > 0
  assert unix.notify_ready(notify_child.notify_fd)?
  unix.notify_close(notify_child.notify_fd)
  assert ! unix.notify_ready(child.notify_fd)?
  let logged = unix.spawn_process_group_log(command, fp"{root}/child.log")?
  let logged_pair = unix.spawn_logged_process_group(command, command)?
  let tty_child = unix.spawn_with_tty(command, tty: "tty1")?
  assert child.pid == 1000
  assert ! child.new_session
  assert logged.pid == 1002
  assert logged_pair.pid == 1003
  assert logged_pair.log_pid == 1004
  assert tty_child.pid == 1005
  assert tty_child.new_session
  unix.kill_process_group(child.pid, "TERM")

  # `kill_all` is not faked: it still searches the host's processes.
  test.error_kind(unix.kill_all("definitely-missing-process", signal: "TERM"), "process-missing")
  unix.exec(command)

  let log_text = log.read_text()?
  assert "\"op\":\"reap_child_events\"" in log_text
  assert "\"op\":\"pid1_setup\"" in log_text
  assert "\"op\":\"spawn_process_group\"" in log_text
  assert "\"op\":\"kill_process_group\"" in log_text
  assert "\"op\":\"tty\"" in log_text
  assert "\"op\":\"set_tty_attrs\"" in log_text
  assert "\"op\":\"set_hostname\"" in log_text
  assert "\"log_path\"" in log_text
  assert "\"op\":\"spawn_logged_process_group\"" in log_text
  assert "\"op\":\"exec\"" in log_text
}

test test_unix_fake_child_events_are_typed { |ctx|
  test.unix_fake(ctx, {event_kind: "child", pid: 42, child_pid: 43, status_kind: "signal", status_code: 15})
  let events: List[FakeChildEvent] = unix.reap_child_events()?.collect()
  assert events[0].pid == 43
  assert events[0].status.signaled()
  assert events[0].status.signal_number()? == 15
}

test test_unix_fake_rejects_unknown_settings { |ctx|
  test.error_kind(test.unix_fake(ctx, {uptime_seconds: 17}), "test-unix-fake")
  test.error_kind(test.unix_fake(ctx, {log: true}), "test-unix-fake")
}

test test_unix_fake_covers_scripts_the_test_runs { |ctx|
  let root = test.temp_dir(ctx, name: "unix-fake-script")?
  let log = fp"{root}/unix.jsonl"
  test.unix_fake(ctx, {tty: "/dev/fake-tty", log: log})
  let result = test.expect(
    ctx,
    """unix.set_hostname("xsh")?
print (unix.tty()?)""",
    status: 0,
  )?
  assert result.stdout == """/dev/fake-tty
"""
  assert "\"op\":\"set_hostname\"" in log.read_text()?

  # A log that cannot be written raises the fake's own logging error.
  let blocked = fp"{root}/file"
  blocked.write("not a directory")
  test.unix_fake(ctx, {log: fp"{blocked}/unix.jsonl"})
  let failed = test.run_script(ctx, "unix.set_hostname(\"xsh\")?")?
  assert ! failed.success
  assert "unix-fake-log" in failed.stderr
}

test test_unix_set_hostname_reaches_the_host_without_a_fake {
  # There is no environment gate: without a fake the call goes to the host.
  # A name longer than any host accepts fails there, so the host keeps its name
  # whether or not the test has the privilege to change it.
  var name = ""
  while name.byte_len() < 300 {
    name = name + "x"
  }

  test.error_kind(unix.set_hostname(name), "unix-set-hostname")
  # Like filesystem errors, the failure carries the facet of its OS error:
  # unprivileged hosts refuse first, privileged ones reject the length.
  match unix.set_hostname(name) {
    Err(is PermissionDenied) | Err(is HostIo) => {}
    Err(error) => test.fail(f"unexpected facet for {error.message}")
    Ok(_) => test.fail("an over-long hostname was accepted")
  }
}

test test_unix_uptime_seconds_reads_the_host_text {
  guard system.uname()?.sysname == "Linux" else {
    # The entry reads `/proc/uptime` on Linux only; on other platforms the
    # binding is still native, so there is nothing to add here.
    test.skip("unix.uptime_seconds reads /proc/uptime on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than the value a host would report: the reading is a whole,
  # non-negative second count, and it never decreases.
  let first = unix.uptime_seconds()?
  assert first >= 0
  let second = unix.uptime_seconds()?
  assert second >= first
}

test test_wait_pid1_event_timeout_kind { |ctx|
  # The optional timeout argument is accepted and the fake reports the
  # `timeout` event kind. (The native deadline loop returning `timeout` on expiry
  # is exercised outside the shared test process to avoid installing real PID 1
  # signal handlers here.)
  test.unix_fake(ctx, {event_kind: "timeout"})
  assert unix.wait_pid1_event(timeout: 5ms)?.kind == "timeout"
  assert unix.wait_pid1_event()?.kind == "timeout"
}

test test_unix_uptime_seconds_is_real_where_the_platform_has_one {
  if system.uname()?.sysname in ["Linux", "Darwin"] {
    assert unix.uptime_seconds()? >= 0
  } else {
    test.error_kind(unix.uptime_seconds(), "unix-unsupported")
  }
}

test test_unix_exec_replaces_the_script_process { |ctx|
  let output = test.expect(
    ctx,
    """let command = process.command_argv("sh", ["sh", "-c", "printf replaced"])
unix.exec(command)?
print "not-reached"
""",
    status: 0,
  )?
  assert output.stdout == "replaced"
}

# The process-group entries start and reap real children. `unix.reap_child_events`
# collects every exited child of its process, so each test below runs its
# script in a process of its own.
proc process_groups_are_supported() [env, error] -> Bool {
  system.uname()?.sysname in ["Linux", "Darwin"]
}

test test_unix_reap_child_events_reports_an_exit_status { |ctx|
  guard process_groups_are_supported() else {
    test.skip("unix process groups need Linux or macOS")
    return
  }
  let output = test.expect(
    ctx,
    r"""type ChildEvent = {pid: Int, status: Status}
let command = process.command_argv("false", ["false"])
let child = unix.spawn_process_group(command)?
var events: List[ChildEvent] = []
var tries = 0
while events.len() == 0 and tries < 100 {
  time.sleep(10ms)?
  events = unix.reap_child_events()?.collect()
  tries += 1
}
print ${events[0].pid == child.pid} ${events[0].status.exited_with(1)}
""",
    status: 0,
  )?
  assert output.stdout == "true true\n"
}

test test_unix_reap_child_events_reports_a_signal_status { |ctx|
  guard process_groups_are_supported() else {
    test.skip("unix process groups need Linux or macOS")
    return
  }
  let root = test.temp_dir(ctx, name: "unix-child-signal")?
  let output = test.expect(
    ctx,
    ready_sleeper + r"""type ChildEvent = {pid: Int, status: Status}
let term = process.signal("TERM")?
unix.kill_process_group(child.pid, "TERM")?
var events: List[ChildEvent] = []
var tries = 0
while events.len() == 0 and tries < 100 {
  time.sleep(10ms)?
  events = unix.reap_child_events()?.collect()
  tries += 1
}
print ${events[0].pid == child.pid} ${events[0].status.signaled()} ${events[0].status.signal_number()? == term.number}
""",
    status: 0,
    args: [fp"{root}/ready"],
  )?
  assert output.stdout == "true true true\n"
}

test test_unix_process_group_can_be_signaled_without_killing_its_parent { |ctx|
  guard process_groups_are_supported() else {
    test.skip("unix process groups need Linux or macOS")
    return
  }
  let root = test.temp_dir(ctx, name: "unix-process-group")?
  let output = test.expect(
    ctx,
    ready_sleeper + r"""unix.kill_process_group(child.pid, "TERM")?
time.sleep(50ms)?
let reaped = unix.reap_child_events()?.collect()
match process.kill(child.pid, signal: "0") {
  Err(e) => {
    test.error_kind(e, "process-missing")?
    print ${child.detach} ${child.new_session} ${child.ignore_hup} ${marker.exists()?} ${reaped.len() >= 0} "process-missing"
  }
}
""",
    status: 0,
    args: [fp"{root}/ready"],
  )?
  assert output.stdout == "true false true true true process-missing\n"
}

test test_unix_logged_process_group_pipes_stdout_and_stderr_to_the_logger { |ctx|
  guard process_groups_are_supported() else {
    test.skip("unix process groups need Linux or macOS")
    return
  }
  let root = test.temp_dir(ctx, name: "unix-logged-process-group")?
  let log = fp"{root}/service.log"
  let output = test.expect(
    ctx,
    r"""let log = Path(args[0])
let command = process.command_argv("sh", ["sh", "-c", "printf service-out; printf service-err >&2"])
let logger = process.command_argv("sh", ["sh", "-c", "cat > $1", "sh", log.display()])
let child = unix.spawn_logged_process_group(command, logger)?
var events = unix.reap_child_events()?.collect()
var tries = 0
while events.len() < 2 and tries < 100 {
  time.sleep(10ms)?
  events = events.extend(unix.reap_child_events()?.collect())
  tries += 1
}
print ${child.pid > 0} ${child.log_pid > 0}
""",
    status: 0,
    args: [log],
  )?
  assert output.stdout == "true true\n"
  let log_text = log.read_text()?
  assert "service-out" in log_text, log_text
  assert "service-err" in log_text, log_text
}

test test_unix_kill_all_does_not_match_a_wrapper_shell_by_its_script_argument { |ctx|
  guard process_groups_are_supported() else {
    test.skip("unix.kill_all needs Linux or macOS")
    return
  }
  # The shell's process name is `sh`; the script name appears only in its argv.
  let root = test.temp_dir(ctx, name: "unix-killall-wrapper")?
  let script = fp"{root}/wrapper-killall-target"
  let marker = fp"{root}/ready"
  script.write(
    r"""#!/bin/sh
printf ready > "$1"
sleep 30 &
child=$!
trap 'kill "$child" 2>/dev/null || true; wait "$child" 2>/dev/null || true' EXIT TERM INT
wait "$child"
""",
  )
  let wrapper = spawn run sh $script $marker ?
  var tries = 0
  while ! marker.exists() and tries < 300 {
    time.sleep(10ms)
    tries += 1
  }

  assert marker.exists()?
  let output = test.expect(
    ctx,
    """match unix.kill_all("wrapper-killall-target") {
  Err(e) => {
    test.error_kind(e, "process-missing")?
    print "process-missing"
  }
}
""",
    status: 0,
  )?
  assert output.stdout == "process-missing\n"
  # The wrapper is still running: signal 0 reaches it.
  process.kill(wrapper.pid, signal: "0")
  wrapper.cancel(signal: "TERM", kill_after: 1s)
}
