proc xsh_bin() [env] -> Path {
  let bin = env.get("CARGO_BIN_EXE_xsh") ?? ""

  if bin != "" {
    return fp"${bin}"
  }

  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = env.get("XSH_CORE_DIR") ?? ""

  if dir != "" {
    return fp"${dir}/${name}"
  }

  return ../name
}

proc test_env_assignment_runs_command() [process, env, error] {
  let output = run.text xsh_bin() core_script("env.xsh") -- XSH_MODULE_PATH=ok xsh_bin() core_script("printenv.xsh") -- XSH_MODULE_PATH ?
  test.eq(output.trim(), "ok")?
}

proc test_env_split_string_runs_command() [process, env, error] {
  let script = core_script("printenv.xsh")
  let command = f"XSH_MODULE_PATH=split ${xsh_bin().display()} ${script.display()} -- XSH_MODULE_PATH"
  let output = run.text xsh_bin() core_script("env.xsh") -- "-S" $command ?
  test.eq(output.trim(), "split")?
}

proc test_env_split_string_as_single_shebang_arg_runs_command() [process, env, error] {
  let script = core_script("printenv.xsh")
  let command = f"-S XSH_MODULE_PATH=split ${xsh_bin().display()} ${script.display()} -- XSH_MODULE_PATH"
  let output = run.text xsh_bin() core_script("env.xsh") -- $command ?
  test.eq(output.trim(), "split")?
}

proc test_env_uses_direct_xsh_shebang() [fs, env, error] {
  test.ok(core_script("env.xsh").read_text()?.starts_with("#!/bin/xsh"))?
}
