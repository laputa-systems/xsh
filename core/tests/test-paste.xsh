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

proc test_paste_parallel_serial_and_delimiters(ctx: TestContext) [fs, process, env, error] {
  let left = test.temp_file(ctx, name: "left.txt", contents: b"a\nb\n")?
  let right = test.temp_file(ctx, name: "right.txt", contents: b"1\n2\n3\n")?
  let parallel = run.text xsh_bin() core_script("paste.xsh") -- $left $right ?
  let serial = run.text xsh_bin() core_script("paste.xsh") -- -s -d: $left $right ?

  test.eq(
    parallel,
    f"""a	1
b	2
	3
""",
  )?

  test.eq(
    serial,
    """a:b
1:2:3
""",
  )?
}

proc test_paste_reads_stdin_and_rejects_flags(ctx: TestContext) [fs, process, env, error] {
  let script = core_script("paste.xsh")

  let command = f"""printf 'a
b
' | ${xsh_bin().display()} ${script.display()} -- -s"""

  let output = run.text sh -c $command ?

  test.eq(
    output,
    f"""a	b
""",
  )?

  let err = test.temp_path(ctx, name: "paste.err")
  let status = run.status xsh_bin() core_script("paste.xsh") -- -z 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "unsupported option")?
}
