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

proc test_link(ctx: TestContext) [env, fs, process, error] {
  let src = test.temp_file(ctx, name: "source.txt", contents: b"same\n")?
  let dst = test.temp_path(ctx, name: "linked.txt")
  let status = run.status xsh_bin() core_script("link.xsh") -- $src $dst
  test.ok(status.exited_with(0))?

  test.eq(
    dst.read_text()?,
    """same
""",
  )?
}
