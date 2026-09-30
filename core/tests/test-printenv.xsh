test test_printenv_named [process, env, error] { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/printenv.xsh" -- PATH ?
  (output.trim() != "")
}

test test_printenv_processes_all_names_before_missing_status [fs, process, env, error] { |ctx|
  let out = test.temp_path(ctx, name: "printenv.out")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/printenv.xsh" -- PATH XSH_CORE_MISSING_ENV_NAME > $out
  ! status.exited_with(0)
  (out.read_text()?.trim() != "")
}
