test test_which_finds_shell { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/which.xsh" -- sh
  assert "sh" in output
}

test test_which_processes_all_names_before_missing_status { |ctx|
  let out = test.temp_path(ctx, name: "which.out")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/which.xsh" -- sh xsh-core-missing-command > $out
  assert ! status.exited_with(0)
  assert "sh" in out.read_text()?
}

test test_which_all_reports_each_executable_search_path_match { |ctx|
  let root = test.temp_dir(ctx, name: "which-all")?
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  first.mkdir()
  second.mkdir()
  fp"{first}/tool".write("#!/bin/sh\n", mode: 0o755)
  fp"{second}/tool".write("#!/bin/sh\n", mode: 0o755)
  fp"{second}/not-executable".write("data", mode: 0o644)
  let search = f"{first}:{second}"
  let output = run.capture --text PATH=$search ${ctx.xsh_bin} fp"{ctx.core_dir}/which.xsh" -- -a tool not-executable
  assert output.status.exited_with(1)
  assert output.stdout == f"{first}/tool\n{second}/tool\n"
  let single = run.capture --text PATH=$search ${ctx.xsh_bin} fp"{ctx.core_dir}/which.xsh" -- tool
  assert single.status.exited_with(0)
  assert single.stdout == f"{first}/tool\n"
}
