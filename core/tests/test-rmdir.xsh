test test_rmdir_parents { |ctx|
  let root = test.temp_dir(ctx, name: "rmdir")?
  fp"{root}/keep".write("keep")?
  let nested = fp"{root}/a/b/c"
  nested.mkdir()
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rmdir.xsh" -p --ignore-fail-on-non-empty $nested ?
  assert ! fp"{root}/a".exists()?
}
