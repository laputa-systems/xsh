test test_mkdir { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir")?
  let nested = fp"{root}/a/b"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -p -m 700 $nested ?
  assert nested.exists()?
  assert nested.metadata()?.mode % 512 == 448
}
