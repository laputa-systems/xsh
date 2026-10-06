type KmodRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_rmmod(ctx: TestContext, argv: List[Str], log: Path) -> KmodRun {
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/rmmod.xsh".read_text()?
  test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir}, b"", "rmmod")?
}

test test_rmmod_normalizes_names_and_forwards_force_under_fake { |ctx|
  let log = test.temp_file(ctx, name: "rmmod.jsonl", contents: b"")?
  let output = run_rmmod(ctx, ["-f", "-v", "demo-name.ko.xz", "second"], log)
  assert output.success, output.stderr
  assert output.stdout == "rmmod demo_name\nrmmod second\n"
  let calls = log.read_text()?
  assert "\"name\":\"demo_name\"" in calls
  assert "\"force\":\"true\"" in calls
  assert calls.lines().len() == 2
}

test test_rmmod_rejects_wait_without_removing { |ctx|
  let log = test.temp_file(ctx, name: "rmmod-wait.jsonl", contents: b"")?
  let output = run_rmmod(ctx, ["--wait", "demo"], log)
  assert output.status == 1
  assert "not supported" in output.stderr
  assert log.read_text()? == ""
}
