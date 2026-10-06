type KmodRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_lsmod(ctx: TestContext, argv: List[Str], log: Path) -> KmodRun {
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/lsmod.xsh".read_text()?
  test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir}, b"", "lsmod")?
}

test test_lsmod_prints_native_module_columns { |ctx|
  let log = test.temp_file(ctx, name: "lsmod.jsonl", contents: b"")?
  let output = run_lsmod(ctx, [], log)
  assert output.success, output.stderr
  assert output.stdout.starts_with("Module                  Size  Used by\n")
  assert "xsh_demo" in output.stdout
  assert "4096" in output.stdout
  assert "xsh_dep" in output.stdout
  assert "\"op\":\"modules\"" in log.read_text()?
}

test test_lsmod_help_does_not_query_modules { |ctx|
  let log = test.temp_file(ctx, name: "lsmod-help.jsonl", contents: b"")?
  let output = run_lsmod(ctx, ["--help"], log)
  assert output.success, output.stderr
  assert "Usage: lsmod" in output.stdout
  assert log.read_text()? == ""
}
