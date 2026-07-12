proc xsh_bin() [env] -> Path {
  let bin = (env.get("CARGO_BIN_EXE_xsh") ?? "")
  if bin != "" {
    return fp"${bin}"
  }
  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = (env.get("XSH_CORE_DIR") ?? "")
  if dir != "" {
    return fp"${dir}/${name}"
  }
  return ../name
}

proc test_printenv_named() [env, process, error] {
  let output = run.text xsh_bin() core_script("printenv.xsh") -- PATH ?
  test.ok(output.trim() != "")?
}

proc test_printenv_processes_all_names_before_missing_status(ctx: TestContext) [env, fs, process, error] {
  let out = test.temp_path(ctx, name: "printenv.out")
  let status = run.status xsh_bin() core_script("printenv.xsh") -- PATH XSH_CORE_MISSING_ENV_NAME > $out
  test.ok(! status.exited_with(0))?
  test.ok(out.read_text()?.trim() != "")?
}
