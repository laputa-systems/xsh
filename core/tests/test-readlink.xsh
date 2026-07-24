proc test_readlink(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "readlink")?
  let target = fp"${root}/target.txt"
  let link = fp"${root}/link.txt"
  target.write("ok")?
  fs.symlink(target, link)?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/readlink.xsh" -- $link ?
  test.contains(output, "target.txt")?
  let resolved = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/readlink.xsh" -- -f $link ?
  test.eq(resolved.trim(), target.resolve()?.display())?
  let resolved_long = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/readlink.xsh" -- --canonicalize $link ?
  test.eq(resolved_long.trim(), target.resolve()?.display())?
}
