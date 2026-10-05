test test_tr_translate_delete_squeeze_and_stdin { |ctx|
  let input = test.temp_file(ctx, name: "tr.txt", contents: b"abbc\n")?
  let translated = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- a A < $input
  let upper = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- a-z A-Z < $input
  let deleted = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -d b < $input
  let digits = test.temp_file(ctx, name: "digits.txt", contents: b"a1-b2\n")?
  let only_digits = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -cd "[:digit:]" < $digits
  let squeezed = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -s b B < $input
  assert translated.trim() == "Abbc"
  assert upper.trim() == "ABBC"
  assert deleted.trim() == "ac"
  assert only_digits.trim() == "12"
  assert squeezed.trim() == "aBc"
  let script = fp"{ctx.core_dir}/tr.xsh"

  let command = f"""printf 'abc
' | {ctx.xsh_bin} {script} -- a A"""

  let stdin_output = run.text sh -c $command
  assert stdin_output.trim() == "Abc"
}

test test_tr_rejects_bad_usage { |ctx|
  let err = test.temp_path(ctx, name: "tr.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- a 2> $err
  assert ! status.exited_with(0)
  assert "missing operand" in err.read_text()?
}

test test_tr_does_not_add_newline_and_single_set_squeezes { |ctx|
  let input = test.temp_file(ctx, name: "raw", contents: b"aaabbcc")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -s a-c < $input
  assert output == "abc"
}

test test_tr_escapes_and_delete_then_squeeze { |ctx|
  let input = test.temp_file(ctx, name: "raw", contents: b"a\t\tb\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -ds a "\\t" < $input
  assert output == "\tb\n"
}

test test_tr_last_duplicate_mapping_and_octal_repeat { |ctx|
  let input = test.temp_file(ctx, name: "raw", contents: b"aab")?
  let duplicated = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- aab XYZ < $input
  assert duplicated == "YYZ"
  let repeated = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- a-b "[x*02]" < $input
  assert repeated == "xxx"
}

test test_tr_repeat_leaves_room_for_suffix_and_rejects_misaligned_classes { |ctx|
  let input = test.temp_file(ctx, name: "raw", contents: b"abcd")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- a-d "[x*]YZ" < $input
  assert output == "xxYZ"
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -t "1[:upper:]" "[:upper:]" < $input
  assert ! status.exited_with(0)
}
