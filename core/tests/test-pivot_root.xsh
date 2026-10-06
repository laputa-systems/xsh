type AppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_applet(ctx: TestContext, argv: List[Str], log: Path) -> AppletRun {
  test.linux_fake(ctx, {log: log, hwclock_epoch_ms: 0, sysctl_value: "42"})
  test.run_script(ctx, fp"{ctx.core_dir}/pivot_root.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "pivot_root")?
}

test test_pivot_root_passes_named_paths_to_native_fake { |ctx|
  let log = test.temp_path(ctx, name: "pivot")
  let result = run_applet(ctx, ["/new", "/new/old"], log)
  assert result.success, result.stderr
  let text = log.read_text()?
  assert "\"op\":\"pivot_root\"" in text
  assert "\"new_root\":\"/new\"" in text
  assert "\"put_old\":\"/new/old\"" in text
}
