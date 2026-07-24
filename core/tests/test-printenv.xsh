proc test_printenv_named(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/printenv.xsh" -- PATH ?
  test.ok(output.trim() != "")?
}

proc test_printenv_processes_all_names_before_missing_status(ctx: TestContext) [fs, process, env, error] {
  let out = test.temp_path(ctx, name: "printenv.out")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/printenv.xsh" -- PATH XSH_CORE_MISSING_ENV_NAME > $out
  test.ok(! status.exited_with(0))?
  test.ok(out.read_text()?.trim() != "")?
}
