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

proc test_which_finds_shell() [env, process, error] {
  let output = run.text xsh_bin() core_script("which.xsh") -- sh ?
  test.contains(output, "sh")?
}

proc test_which_processes_all_names_before_missing_status(ctx: TestContext) [env, fs, process, error] {
  let out = test.temp_path(ctx, name: "which.out")
  let status = run.status xsh_bin() core_script("which.xsh") -- sh xsh-core-missing-command > $out
  test.ok(! status.exited_with(0))?
  test.contains(out.read_text()?, "sh")?
}
