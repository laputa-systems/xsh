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

proc test_date_format() [process, env, error] {
  let output = run.text xsh_bin() core_script("date.xsh") -- -u +%Y ?
  test.eq(output.trim().count_chars(), 4)?
  let offset = run.text xsh_bin() core_script("date.xsh") -- -u +%:z ?
  test.eq(offset.trim(), "+00:00")?
}
