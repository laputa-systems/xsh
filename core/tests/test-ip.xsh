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

proc test_ip_addr_smoke() [process, env, error] {
  let output = run.text XSH_LINUX_DRY_RUN=1 xsh_bin() core_script("ip.xsh") -- addr ?
  test.ok(output.count_chars() >= 0)?
}
