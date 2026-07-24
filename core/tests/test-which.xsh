proc test_which_finds_shell(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/which.xsh" -- sh ?
  test.contains(output, "sh")?
}

proc test_which_processes_all_names_before_missing_status(ctx: TestContext) [fs, process, env, error] {
  let out = test.temp_path(ctx, name: "which.out")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/which.xsh" -- sh xsh-core-missing-command > $out
  test.ok(! status.exited_with(0))?
  test.contains(out.read_text()?, "sh")?
}
