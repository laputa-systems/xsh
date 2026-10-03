test test_which_finds_shell { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/which.xsh" -- sh ?
  "sh" in output
}

test test_which_processes_all_names_before_missing_status { |ctx|
  let out = test.temp_path(ctx, name: "which.out")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/which.xsh" -- sh xsh-core-missing-command > $out
  ! status.exited_with(0)
  "sh" in out.read_text()?
}
