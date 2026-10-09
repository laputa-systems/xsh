test test_unexpand_leading_and_all_blanks { |ctx|
  let input = test.temp_file(ctx, name: "spaces.txt", contents: b"        a       b\n")?
  let leading = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" $input ?
  let all = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" --all $input ?

  assert leading == "\ta       b\n"
  assert all == "\ta\tb\n"
}

test test_unexpand_custom_stops { |ctx|
  let input = test.temp_file(ctx, name: "custom.txt", contents: b"   x\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" --tabs=3 --all $input ?

  assert output == "\tx\n"
}
