pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_env_assignment_runs_command() [process, error] {
  let output = run.text xsh_bin() env.xsh -- XSH_MODULE_PATH=ok xsh_bin() printenv.xsh -- XSH_MODULE_PATH ?
  test.eq(output.trim(), "ok")?
}

proc test_env_split_string_runs_command() [process, error] {
  let command = f"XSH_MODULE_PATH=split ${xsh_bin().display()} printenv.xsh -- XSH_MODULE_PATH"
  let output = run.text xsh_bin() env.xsh -- "-S" $command ?
  test.eq(output.trim(), "split")?
}

proc test_env_split_string_as_single_shebang_arg_runs_command() [process, error] {
  let command = f"-S XSH_MODULE_PATH=split ${xsh_bin().display()} printenv.xsh -- XSH_MODULE_PATH"
  let output = run.text xsh_bin() env.xsh -- $command ?
  test.eq(output.trim(), "split")?
}

proc test_env_uses_direct_xsh_shebang() [fs, error] {
  test.ok(p"env.xsh".read_text()?.starts_with("#!/bin/xsh"))?
}
