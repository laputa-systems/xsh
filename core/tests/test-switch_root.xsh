type AppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_applet(ctx: TestContext, argv: List[Str], log: Path) -> AppletRun {
  test.linux_fake(ctx, {log: log, hwclock_epoch_ms: 0, sysctl_value: "42"})
  test.run_script(ctx, fp"{ctx.core_dir}/switch_root.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "switch_root")?
}

test test_switch_root_passes_paths_and_rejects_unforwarded_init_arguments { |ctx|
  let log = test.temp_path(ctx, name: "switch")
  let result = run_applet(ctx, ["/new", "/sbin/init"], log)
  assert result.success, result.stderr
  assert "\"op\":\"switch_root\"" in log.read_text()?
  let rejected = run_applet(ctx, ["/new", "/sbin/init", "--argument"], log)
  assert ! rejected.success
  assert "init arguments" in rejected.stderr
}
