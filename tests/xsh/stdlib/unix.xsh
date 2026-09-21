proc test_unix_dry_run_covers_module_surface(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "unix")?
  let log = fp"${root}/unix.jsonl"
  let command = process.command_argv("demo", ["demo", "arg"])

  env XSH_UNIX_DRY_RUN=1 XSH_UNIX_DRY_RUN_SIGNAL=USR1 XSH_UNIX_UPTIME_SECONDS=17 XSH_UNIX_DRY_RUN_LOG=$log {
    test.eq(unix.reap_child_events()?.collect().len(), 0)?
    unix.pid1_setup(["TERM"], subreaper: true, allow_non_pid1: true)?
    let event = unix.wait_pid1_event()?
    test.eq(event.kind, "signal")?
    let shutdown = unix.shutdown_process_groups([1000], 1ms, kill_timeout: 1ms)?
    test.ok(shutdown.term_sent >= 0)?
    test.eq(unix.uptime_seconds()?, 17)?
    test.eq(unix.tty()?, "/dev/tty")?
    test.eq(unix.id()?.groups[0].name, "root")?
    let attrs = unix.tty_attrs()?
    test.ok(attrs.raw)?
    unix.set_tty_attrs(attrs)?
    unix.set_hostname("xsh")?
    let child = unix.spawn_process_group(command)?
    let notify_child = unix.spawn_process_group(command, notify: true)?
    test.ok(notify_child.notify_fd > 0)?
    test.ok(unix.notify_ready(notify_child.notify_fd)?)?
    unix.notify_close(notify_child.notify_fd)?
    test.ok(! unix.notify_ready(child.notify_fd)?)?
    let logged = unix.spawn_process_group_log(command, fp"${root}/child.log")?
    let logged_pair = unix.spawn_logged_process_group(command, command)?
    let tty_child = unix.spawn_with_tty(command, tty: "tty1")?
    test.ok(child.pid > 0)?
    test.ok(logged.pid > 0)?
    test.ok(logged_pair.log_pid > 0)?
    test.ok(tty_child.new_session)?
    unix.kill_process_group(child.pid, "TERM")?
    test.error_kind(unix.kill_all("definitely-missing-process", signal: "TERM"), "process-missing")?
    unix.exec(command)?
  } ?

  let log_text = log.read_text()?
  test.contains(log_text, "\"op\":\"pid1_setup\"")?
  test.contains(log_text, "\"op\":\"spawn_process_group\"")?
  test.contains(log_text, "\"log_path\"")?
  test.contains(log_text, "\"op\":\"spawn_logged_process_group\"")?
  test.contains(log_text, "\"op\":\"exec\"")?
}

proc test_unix_uptime_seconds_dry_run_log(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "unix-uptime")?
  let log = fp"${root}/unix.jsonl"

  # The dry-run reading comes from the override variable, and the call appends
  # one line to the log file with the seconds count as a JSON string.
  env XSH_UNIX_DRY_RUN=1 XSH_UNIX_UPTIME_SECONDS=17 XSH_UNIX_DRY_RUN_LOG=$log {
    test.eq(unix.uptime_seconds()?, 17)?
  } ?
  test.eq(
    log.read_text()?,
    """{"op":"uptime_seconds","seconds":"17"}
""",
  )?

  # An override that is not an integer reads as zero, and so does an unset one.
  env XSH_UNIX_DRY_RUN=1 XSH_UNIX_UPTIME_SECONDS=nope {
    test.eq(unix.uptime_seconds()?, 0)?
  } ?
  env XSH_UNIX_DRY_RUN=1 {
    test.eq(unix.uptime_seconds()?, 0)?
  } ?
}

proc test_unix_uptime_seconds_log_failure_kind(ctx: TestContext) [fs, process, env, error] {
  if system.uname()?.sysname != "Linux" {
    # The script-backed entry reports a log failure as the call's `Err`, while
    # the native dry-run arm raises it, so the failure is only a value on the
    # platform that uses this implementation.
    test.skip("a log failure is the call's Err on Linux only")
    return
  }

  # A log that cannot be written is reported with the log's own kind rather than
  # the entry's kind, and the call has no reading to report.
  let root = test.temp_dir(ctx, name: "unix-uptime-log")?
  let blocked = fp"${root}/file"
  fs.write(blocked, "not a directory")?
  let blocked_log = fp"${blocked}/unix.jsonl"
  env XSH_UNIX_DRY_RUN=1 XSH_UNIX_UPTIME_SECONDS=17 XSH_UNIX_DRY_RUN_LOG=$blocked_log {
    test.error_kind(unix.uptime_seconds(), "unix-dry-run-log")?
  } ?
}

proc test_unix_uptime_seconds_reads_the_host_text() [process, env, error] {
  if system.uname()?.sysname != "Linux" {
    # The entry reads `/proc/uptime` on Linux only; on other platforms the
    # binding is still native, so there is nothing to add here.
    test.skip("unix.uptime_seconds reads /proc/uptime on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than the value a host would report: the reading is a whole,
  # non-negative second count, and it never decreases. The dry-run gate is
  # emptied so the surrounding environment cannot decide what is read.
  env XSH_UNIX_DRY_RUN="" {
    let first = unix.uptime_seconds()?
    test.ok(first >= 0)?
    let second = unix.uptime_seconds()?
    test.ok(second >= first)?
  } ?
}

proc test_wait_pid1_event_timeout_kind() [process, env, error] {
  # The optional timeout argument is accepted and the dry-run path reports the
  # `timeout` event kind. (The native deadline loop returning `timeout` on expiry
  # is exercised outside the shared test process to avoid installing real PID 1
  # signal handlers here.)
  env XSH_UNIX_DRY_RUN=1 XSH_UNIX_DRY_RUN_EVENT_KIND=timeout {
    test.eq(unix.wait_pid1_event(timeout: 5ms)?.kind, "timeout")?
    test.eq(unix.wait_pid1_event()?.kind, "timeout")?
  } ?
}
