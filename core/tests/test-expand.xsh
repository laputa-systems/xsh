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

test test_expand_accepts_non_utf8_path_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "expand-raw-path")?
  let input = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  input.write("a\tb\n")
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" -- $input
  assert output == "a       b\n"
}

test test_expand_tab_list_errors_have_no_usage_hint { |ctx|
  let input = test.temp_file(ctx, name: "tabs", contents: b"a\n")?
  let error = test.temp_path(ctx, name: "tab-error")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/expand.xsh" -- --tabs=0 $input 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "expand: tab size cannot be 0\n"
}
