type AppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_applet(ctx: TestContext, argv: List[Str], log: Path) -> AppletRun {
  test.linux_fake(ctx, {log: log, hwclock_epoch_ms: 0, sysctl_value: "42"})
  test.run_script(ctx, fp"{ctx.core_dir}/sysctl.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "sysctl")?
}

test test_sysctl_reads_writes_and_loads_configuration_through_native_keys { |ctx|
  let root = test.temp_dir(ctx, name: "sysctl")?
  let log = fp"{root}/log"
  let read = run_applet(ctx, ["-n", "kernel.pid_max"], log)
  assert read.success, read.stderr
  assert read.stdout == "42\n"
  let write = run_applet(ctx, ["-q", "-w", "net.ipv4.ip_forward=1"], log)
  assert write.success, write.stderr
  assert write.stdout == ""
  let config = fp"{root}/config"
  config.write("# comment\nnet.ipv4.ip_forward = 0\n; comment\n-kernel.pid_max = 32768\n")
  let load = run_applet(ctx, ["-p", config.display()], log)
  assert load.success, load.stderr
  assert "net.ipv4.ip_forward = 0" in load.stdout
  assert "\"op\":\"sysctl_set\"" in log.read_text()?
  assert "\"key\":\"kernel.pid_max\"" in log.read_text()?
}

test test_sysctl_bad_assignment_fails_before_any_write { |ctx|
  let log = test.temp_path(ctx, name: "sysctl-invalid")
  let result = run_applet(ctx, ["-w", "valid.key=1", "missing-value"], log)
  assert ! result.success
  assert ! log.exists()?
}


test test_sysctl_preserves_interleaved_operand_order { |ctx|
  let log = test.temp_path(ctx, name: "sysctl-order")
  let result = run_applet(ctx, ["kernel.pid_max=7", "kernel.pid_max"], log)
  assert result.success, result.stderr
  let lines = log.read_text()?.lines().collect()
  assert "sysctl_set" in lines[0]
  assert "sysctl_get" in lines[1]
}
