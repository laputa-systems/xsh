type KmodRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_insmod(ctx: TestContext, argv: List[Str], log: Path) -> KmodRun {
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/insmod.xsh".read_text()?
  test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir}, b"", "insmod")?
}

test test_insmod_forwards_quoted_parameters_under_fake { |ctx|
  let log = test.temp_file(ctx, name: "insmod.jsonl", contents: b"")?
  let output = run_insmod(ctx, ["/tmp/demo.ko", "debug=1", "message=hello world"], log)
  assert output.success, output.stderr
  assert output.stdout == ""
  let calls = log.read_text()?
  assert "\"op\":\"insmod\"" in calls
  assert "debug=1 message=\\\"hello world\\\"" in calls
}

test test_insmod_rejects_unsupported_force_without_loading { |ctx|
  let log = test.temp_file(ctx, name: "unsupported-insmod.jsonl", contents: b"")?
  let output = run_insmod(ctx, ["--force", "/tmp/demo.ko"], log)
  assert output.status == 1
  assert "not supported" in output.stderr
  assert log.read_text()? == ""
}

test test_insmod_missing_file_reports_error_without_kernel_insert { |ctx|
  guard system.uname()?.sysname == "Linux" else { test.skip("module insertion requires Linux"); return }
  let root = test.temp_dir(ctx, name: "insmod-missing")?
  let source = fp"{ctx.core_dir}/insmod.xsh".read_text()?
  let output = test.run_script(ctx, source, [f"{root}/missing.ko"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "insmod")?
  assert output.status == 1
  assert "could not insert module" in output.stderr
  assert "No such file or directory" in output.stderr
}
