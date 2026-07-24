proc test_basename_basic(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/basename.xsh" -- /tmp/demo.txt ?
  test.eq(output.trim(), "demo.txt")?
}

proc test_basename_suffix_and_multiple(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/basename.xsh" -- -a -s .txt /tmp/demo.txt /tmp/other.txt ?

  test.eq(
    output.trim(),
    """demo
other""",
  )?
}

proc test_basename_runs_as_executable_shebang_script(ctx: TestContext) [fs, process, env, error] {
  if ! p"/bin/xsh".exists()? {
    test.skip("/bin/xsh is not installed")?
  }

  let script = fp"${ctx.core_dir}/basename.xsh"
  script.chmod(0o755)?
  let output = run.text $script -- /tmp/demo.txt ?

  test.eq(
    output,
    """demo.txt
""",
  )?
}
