test test_expand_default_and_explicit_tab_stops { |ctx|
  let input = test.temp_file(ctx, name: "tabs.txt", contents: b"a\tb\n")?
  let default = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" $input ?
  let custom = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" --tabs=3 $input ?

  assert default == "a       b\n"
  assert custom == "a  b\n"
}

test test_expand_initial_only_and_multifile { |ctx|
  let first = test.temp_file(ctx, name: "first.txt", contents: b"\ta\tb\n")?
  let second = test.temp_file(ctx, name: "second.txt", contents: b"\tx\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" --initial $first $second ?

  assert output == "        a\tb\n        x\n"
}

test test_expand_last_repeating_tab_stop { |ctx|
  let input = test.temp_file(ctx, name: "repeat-tabs.txt", contents: b"\ta\tb\tc")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" --tabs=1,/5 $input ?
  assert output == " a   b    c"
}
