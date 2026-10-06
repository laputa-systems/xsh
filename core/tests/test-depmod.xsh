type KmodRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_depmod(ctx: TestContext, argv: List[Str], log: Path) -> KmodRun {
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/depmod.xsh".read_text()?
  test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir}, b"", "depmod")?
}

test test_depmod_release_and_all_under_fake { |ctx|
  let log = test.temp_file(ctx, name: "depmod.jsonl", contents: b"")?
  let output = run_depmod(ctx, ["--all", "6.12.1"], log)
  assert output.success, output.stderr
  assert "\"version\":\"6.12.1\"" in log.read_text()?
}

test test_depmod_rejects_unsupported_basedir_before_writing { |ctx|
  let log = test.temp_file(ctx, name: "depmod-basedir.jsonl", contents: b"")?
  let output = run_depmod(ctx, ["-b", "/tmp"], log)
  assert output.status == 1
  assert "not supported" in output.stderr
  assert log.read_text()? == ""
}
