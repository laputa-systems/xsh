test test_ln_symbolic_force { |ctx|
  let root = test.temp_dir(ctx, name: "ln")?
  let src = fp"${root}/src.txt"
  let dst = fp"${root}/dst.txt"
  src.write("new")?
  dst.write("old")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ln.xsh" -- -sf $src $dst ?
  assert "src.txt" in dst.readlink()?.display()
}
