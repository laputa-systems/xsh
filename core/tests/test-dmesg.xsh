use core.lib.system_control as control

type AppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_applet(ctx: TestContext, argv: List[Str], log: Path) -> AppletRun {
  test.linux_fake(ctx, {log: log, hwclock_epoch_ms: 0, sysctl_value: "42"})
  test.run_script(ctx, fp"{ctx.core_dir}/dmesg.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "dmesg")?
}

test test_dmesg_uses_native_messages_and_rejects_metadata_options { |ctx|
  let log = test.temp_path(ctx, name: "log")
  let output = run_applet(ctx, [], log)
  assert output.success, output.stderr
  assert output.stdout == "xsh dry-run kernel message\n"
  assert "\"op\":\"dmesg\"" in log.read_text()?
  let rejected = run_applet(ctx, ["--raw"], log)
  assert ! rejected.success
  assert "not supported" in rejected.stderr
}


test test_dmesg_notime_removes_only_a_leading_kernel_timestamp {
  assert control.kernel_message("[    12.003] driver ready", true) == "driver ready"
  assert control.kernel_message("[    12.003] driver ready", false) == "[    12.003] driver ready"
  assert control.kernel_message("driver [12.003] ready", true) == "driver [12.003] ready"
  assert control.kernel_message("[device] ready", true) == "[device] ready"
}
