type AppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_applet(ctx: TestContext, argv: List[Str], log: Path) -> AppletRun {
  test.linux_fake(ctx, {log: log, hwclock_epoch_ms: 0, sysctl_value: "42"})
  test.run_script(ctx, fp"{ctx.core_dir}/hwclock.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "hwclock")?
}

test test_hwclock_show_set_and_hctosys_use_typed_epoch { |ctx|
  let log = test.temp_path(ctx, name: "hwclock")
  let show = run_applet(ctx, ["--show", "--utc"], log)
  assert show.success, show.stderr
  assert show.stdout == "1970-01-01 00:00:00.000000+00:00\n"
  let written = run_applet(ctx, ["--set", "--date", "1970-01-02 00:00:00 UTC"], log)
  assert written.success, written.stderr
  assert "\"epoch_ms\":\"86400000\"" in log.read_text()?
  let sync = run_applet(ctx, ["--hctosys"], log)
  assert sync.success, sync.stderr
  assert "\"op\":\"set_system_clock\"" in log.read_text()?
}

test test_hwclock_unsupported_local_rtc_and_conflicting_modes_fail_before_effects { |ctx|
  let log = test.temp_path(ctx, name: "hwclock-invalid")
  let result = run_applet(ctx, ["--localtime"], log)
  assert ! result.success
  assert ! log.exists()?
  let conflict = run_applet(ctx, ["-s", "-w"], log)
  assert ! conflict.success
  assert ! log.exists()?
}
