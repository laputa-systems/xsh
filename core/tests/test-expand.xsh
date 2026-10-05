test test_expand_initial_and_backspace { |ctx|
  let input = test.temp_file(ctx, name: "tabs", contents: b"\tx\t\nabc\x08\tx")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" -- -t 4 -i $input
  assert output == "    x\t\nabc\u{8}\tx"
}

test test_expand_absolute_and_relative_stops { |ctx|
  let input = test.temp_file(ctx, name: "tabs", contents: b"\ta\tb\tc")?
  let relative = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" -- -t "1,+5" $input
  assert relative == " a    b    c"
  let absolute = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" -- -t "1,/5" $input
  assert absolute == " a   b    c"
}

test test_expand_does_not_allocate_for_unused_large_stop { |ctx|
  let input = test.temp_file(ctx, name: "plain", contents: b"plain\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" -- -t 2147483647 $input
  assert output == "plain\n"
}

test test_expand_unicode_display_columns { |ctx|
  let input = test.temp_file(ctx, name: "wide", contents: b"\xe4\xb8\xad\tX")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" -- $input
  assert output == "中      X"
}
