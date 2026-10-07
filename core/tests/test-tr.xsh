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

test test_tr_squeezes_runs_across_stdin_reads { |ctx|
  let contents = bytes.from_text([" " for _ in range(65537)].join(""))
  let input = test.temp_file(ctx, name: "long-space-run", contents: contents)?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -s " " < $input
  assert output == " "
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

test test_tr_requires_a_longer_first_set_to_end_with_plain_characters { |ctx|
  let error = test.temp_path(ctx, name: "tr.err")
  let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- "[:upper:]a" "[:upper:]" < /dev/null 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "tr: when translating with string1 longer than string2,\nthe latter string must not end with a character class\n"
}

test test_tr_keeps_repeat_runs_compressed_and_indexes_unsigned_counts { |ctx|
  let input = test.temp_file(ctx, name: "bytes", contents: b"abc")?
  let first = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- "[a*9223372036854775808]" b < $input
  assert first == "bbc"
  let second = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- abc "[x*9223372036854775808]" < $input
  assert second == "xxx"
  let beyond = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- "[a*18446744073709551615]b" "[x*18446744073709551614][y*]z" < $input
  assert beyond == "yzc"
}

test test_tr_complemented_classes_require_a_complete_uniform_domain { |ctx|
  let input = test.temp_file(ctx, name: "input", contents: b"aA\n")?
  let error = test.temp_path(ctx, name: "error")
  let oversized = ["x" for _ in range(231)].join("")
  let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -c "[:upper:]" $oversized < $input 2> $error
  assert status.exited_with(1)
  assert "string2 must map all characters in the domain to one" in error.read_text()?
  let short = ["x" for _ in range(100)].join("")
  let truncated = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -ct "[:upper:]" $short < $input 2> $error
  assert truncated.exited_with(1)
}

test test_tr_distinguishes_repeated_equals_from_equivalence_classes { |ctx|
  let input = test.temp_file(ctx, name: "input", contents: b"ab=c\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- "a[=*2][=c=]" xyyz < $input
  assert output == "xbyz\n"
  let error = test.temp_path(ctx, name: "error")
  for item in [{spec: "[::]", message: "missing character class name '[::]'"}, {spec: "[==]", message: "missing equivalence class character '[==]'"}, {spec: "[=aa=]", message: "aa: equivalence class operand must be a single character"}] {
    let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -d ${item.spec} < $input 2> $error
    assert status.exited_with(1)
    assert error.read_text()? == "tr: " + item.message + "\n"
  }
}

test test_tr_errors_identify_range_and_delete_mode_requirements { |ctx|
  let error = test.temp_path(ctx, name: "error")
  let range_error = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- z-a x < /dev/null 2> $error
  assert range_error.exited_with(1)
  assert error.read_text()? == "tr: range-endpoints of 'z-a' are in reverse collating sequence order\n"
  let escaped_range = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -d r"\046-\048" < /dev/null 2> $error
  assert escaped_range.exited_with(1)
  assert error.read_text()? == r"tr: range-endpoints of '&-\004' are in reverse collating sequence order" + "\n"
  let delete_error = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -d ab cd < /dev/null 2> $error
  assert delete_error.exited_with(1)
  assert "Only one string may be given when deleting without squeezing repeats." in error.read_text()?
  let squeeze_error = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" -- -ds ab < /dev/null 2> $error
  assert squeeze_error.exited_with(1)
  assert "Two strings must be given when both deleting and squeezing repeats." in error.read_text()?
}

test test_tr_preserves_non_utf8_set_operands { |ctx|
  let input = test.temp_file(ctx, name: "raw-set-input", contents: b"\x01amp\xfe\xff")?
  let translated = test.temp_path(ctx, name: "translated")
  let translate_command = "set1=$(printf 'a\\\\376\\\\377z'); exec \"$1\" \"$2\" \"$set1\" 01234 < \"$3\" > \"$4\""
  let translate_status = run.status env LC_ALL=C sh -c $translate_command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" $input $translated
  assert translate_status.exited_with(0)
  assert translated.read_bytes()? == b"\x010mp12"

  let escaped_input = test.temp_file(ctx, name: "escaped-input", contents: b"(1\xff)")?
  let deleted = test.temp_path(ctx, name: "deleted")
  let error = test.temp_path(ctx, name: "tr.err")
  let delete_command = "set1=$(printf '\\\\501\\\\377'); exec \"$1\" \"$2\" -d \"$set1\" < \"$3\" > \"$4\""
  let delete_status = run.status env LC_ALL=C sh -c $delete_command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" $escaped_input $deleted 2> $error
  assert delete_status.exited_with(0)
  assert deleted.read_bytes()? == b")"
  assert "warning: the ambiguous octal escape" in error.read_text()?

  let malformed = test.temp_path(ctx, name: "malformed-repeat")
  let malformed_error = test.temp_path(ctx, name: "malformed-repeat.err")
  let malformed_command = "set1=$(printf '[a*\\377]'); exec \"$1\" \"$2\" \"$set1\" x < /dev/null > \"$3\" 2> \"$4\""
  let malformed_status = run.status env LC_ALL=C sh -c $malformed_command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/tr.xsh" $malformed $malformed_error
  assert malformed_status.exited_with(1)
  assert "invalid repeat count" in malformed_error.read_text()?
}
