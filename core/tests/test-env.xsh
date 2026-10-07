test test_env_assignment_runs_command { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- XSH_MODULE_PATH=ok ${ctx.xsh_bin} \
    fp"{ctx.core_dir}/printenv.xsh" -- XSH_MODULE_PATH
  assert output.trim() == "ok"
}

test test_env_split_string_runs_command { |ctx|
  let script = fp"{ctx.core_dir}/printenv.xsh"
  let command = f"XSH_MODULE_PATH=split {ctx.xsh_bin} {script} -- XSH_MODULE_PATH"
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- "-S" $command
  assert output.trim() == "split"
}

test test_env_split_string_as_single_shebang_arg_runs_command { |ctx|
  let script = fp"{ctx.core_dir}/printenv.xsh"
  let command = f"-S XSH_MODULE_PATH=split {ctx.xsh_bin} {script} -- XSH_MODULE_PATH"
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- $command
  assert output.trim() == "split"
}

test test_env_uses_direct_xsh_shebang { |ctx|
  assert fp"{ctx.core_dir}/env.xsh".read_text()?.starts_with("#!/bin/xsh")
}

test test_env_split_string_preserves_quoted_argument { |ctx|
  let command = f"-S {ctx.xsh_bin} {ctx.core_dir}/printf.xsh -- %s \"hello world\""
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- $command

  assert output == "hello world"
}

test test_env_chdir_runs_command_in_selected_directory { |ctx|
  let directory = test.temp_dir(ctx, name: "env-cwd")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- -C ${directory} pwd

  assert output.trim() == directory.display()
}

test test_env_null_environment_list_uses_nul_separators { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- -0

  assert "PATH=" in output
  assert "\0" in output
  assert ! output.ends_with("\n")
}
