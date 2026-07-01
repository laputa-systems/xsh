pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_which_finds_shell() [process, error] {
  let output = run.text xsh_bin() which.xsh -- sh ?
  test.contains(output, "sh")?
}

proc test_which_processes_all_names_before_missing_status(ctx: TestContext) [fs, process, error] {
  let out = test.temp_path(ctx, name: "which.out")
  let status = run.status xsh_bin() which.xsh -- sh xsh-core-missing-command > $out
  test.ok(! status.exited_with(0))?
  test.contains(out.read_text()?, "sh")?
}
