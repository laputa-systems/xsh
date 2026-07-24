proc test_link(ctx: TestContext) [fs, process, env, error] {
  let src = test.temp_file(ctx, name: "source.txt", contents: b"same\n")?
  let dst = test.temp_path(ctx, name: "linked.txt")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/link.xsh" -- $src $dst
  test.ok(status.exited_with(0))?

  test.eq(
    dst.read_text()?,
    """same
""",
  )?
}
