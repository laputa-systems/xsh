test test_rmdir_parents [fs, process, env, error] { |ctx|
  let root = test.temp_dir(ctx, name: "rmdir")?
  let nested = fp"${root}/a/b/c"
  nested.mkdir()?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rmdir.xsh" -- -p $nested ?
  ! fp"${root}/a".exists()?
}
