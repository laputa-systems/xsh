test test_realpath { |ctx|
  let root = test.temp_dir(ctx, name: "realpath")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/realpath.xsh" -- $root ?
  output.trim() == root.resolve()?.display()
}
