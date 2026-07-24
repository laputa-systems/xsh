proc test_env_assignment_runs_command(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/env.xsh" -- XSH_MODULE_PATH=ok ${ctx.xsh_bin} fp"${ctx.core_dir}/printenv.xsh" -- XSH_MODULE_PATH ?
  test.eq(output.trim(), "ok")?
}

proc test_env_split_string_runs_command(ctx: TestContext) [process, env, error] {
  let script = fp"${ctx.core_dir}/printenv.xsh"
  let command = f"XSH_MODULE_PATH=split ${ctx.xsh_bin.display()} ${script.display()} -- XSH_MODULE_PATH"
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/env.xsh" -- "-S" $command ?
  test.eq(output.trim(), "split")?
}

proc test_env_split_string_as_single_shebang_arg_runs_command(ctx: TestContext) [process, env, error] {
  let script = fp"${ctx.core_dir}/printenv.xsh"
  let command = f"-S XSH_MODULE_PATH=split ${ctx.xsh_bin.display()} ${script.display()} -- XSH_MODULE_PATH"
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/env.xsh" -- $command ?
  test.eq(output.trim(), "split")?
}

proc test_env_uses_direct_xsh_shebang(ctx: TestContext) [fs, env, error] {
  test.ok(fp"${ctx.core_dir}/env.xsh".read_text()?.starts_with("#!/bin/xsh"))?
}
