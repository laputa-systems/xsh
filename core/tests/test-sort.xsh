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

proc test_sort_unique_reverse(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "sort.txt", contents: b"b\na\nb\n")?
  let output = run.text xsh_bin() core_script("sort.xsh") -- -u -r $input ?
  let output_lines = output.lines().collect()
  test.eq(output_lines[0], "b")?
  test.eq(output_lines[1], "a")?
  let keyed = test.temp_file(ctx, name: "keyed.txt", contents: b"b,20\na,3\nc,1\n")?
  let by_second = run.text xsh_bin() core_script("sort.xsh") -- -t, -k2 -n $keyed ?
  let by_second_lines = by_second.lines().collect()
  test.eq(by_second_lines[0], "c,1")?
  test.eq(by_second_lines[2], "b,20")?
}
