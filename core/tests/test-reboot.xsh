type AppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_applet(ctx: TestContext, argv: List[Str], log: Path) -> AppletRun {
  test.linux_fake(ctx, {log: log, hwclock_epoch_ms: 0, sysctl_value: "42"})
  test.run_script(ctx, fp"{ctx.core_dir}/reboot.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "reboot")?
}

test test_reboot_forced_operation_is_native_and_default_does_not_force_shutdown { |ctx|
  let log = test.temp_path(ctx, name: "reboot")
  let rejected = run_applet(ctx, [], log)
  assert ! rejected.success
  assert ! log.exists()?
  let forced = run_applet(ctx, ["--force", "--no-sync"], log)
  assert forced.success, forced.stderr
  assert "\"op\":\"reboot\"" in log.read_text()?
}
