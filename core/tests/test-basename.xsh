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

proc test_basename_basic() [env, process, error] {
  let output = run.text xsh_bin() core_script("basename.xsh") -- /tmp/demo.txt ?
  test.eq(output.trim(), "demo.txt")?
}

proc test_basename_suffix_and_multiple() [env, process, error] {
  let output = run.text xsh_bin() core_script("basename.xsh") -- -a -s .txt /tmp/demo.txt /tmp/other.txt ?

  test.eq(
    output.trim(),
    """demo
other""",
  )?
}

proc test_basename_runs_as_executable_shebang_script() [env, fs, process, error] {
  if ! p"/bin/xsh".exists()? {
    test.skip("/bin/xsh is not installed")?
  }
  let script = core_script("basename.xsh")
  script.chmod(0o755)?
  let output = run.text $script -- /tmp/demo.txt ?
  test.eq(output, "demo.txt\n")?
}
