test test_readlink { |ctx|
  let root = test.temp_dir(ctx, name: "readlink")?
  let target = fp"${root}/target.txt"
  let link = fp"${root}/link.txt"
  target.write("ok")?
  fs.symlink(target, link)?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/readlink.xsh" -- $link ?
  "target.txt" in output
  let resolved = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/readlink.xsh" -- -f $link ?
  resolved.trim() == target.resolve()?.display()
  let resolved_long = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/readlink.xsh" -- --canonicalize $link ?
  resolved_long.trim() == target.resolve()?.display()
}
