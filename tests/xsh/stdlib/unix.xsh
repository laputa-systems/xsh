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
