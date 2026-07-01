pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_printenv_named() [process, error] {
  let output = run.text xsh_bin() printenv.xsh -- PATH ?
  test.ok(output.trim() != "")?
}

proc test_printenv_processes_all_names_before_missing_status(ctx: TestContext) [fs, process, error] {
  let out = test.temp_path(ctx, name: "printenv.out")
  let status = run.status xsh_bin() printenv.xsh -- PATH XSH_CORE_MISSING_ENV_NAME > $out
  test.ok(! status.exited_with(0))?
  test.ok(out.read_text()?.trim() != "")?
}
