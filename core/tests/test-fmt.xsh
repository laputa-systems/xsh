test test_fmt_joins_paragraphs_and_quick_wraps { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"one\ntwo\nthree\n\nfour five\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -q -w 8 $input
  assert output == "one two\nthree\n\nfour\nfive\n"
}

test test_fmt_prefix_leaves_other_lines { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"# one\n# two\nplain\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -q -p "# " $input
  assert output == "# one two\nplain\n"
}

test test_fmt_accepts_non_utf8_path_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "fmt-raw-path")?
  let input = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  input.write("one two\n")
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- $input
  assert output == "one two\n"
}

test test_fmt_balances_paragraph_lines { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"aa bb cc dd ee")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -w 7 $input
  assert output == "aa\nbb cc\ndd ee\n"
}

test test_fmt_preserves_invalid_utf8_bytes { |ctx|
  let input = test.temp_file(ctx, name: "raw", contents: b"=\xa0=")?
  let output = test.temp_path(ctx, name: "formatted")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -s -w1 $input > $output
  assert status.exited_with(0)
  assert output.read_bytes()? == b"=\xa0=\n"
}

test test_fmt_avoids_false_sentence_breaks_and_sentence_widows { |ctx|
  let initials = test.temp_file(ctx, name: "initials", contents: b"Donald E. Knuth and Michael F. Plass wrote this paragraph formatting algorithm.")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -g25 -w30 $initials
  assert output == "Donald E. Knuth and Michael\nF. Plass wrote this paragraph\nformatting algorithm.\n"
  let sentences = test.temp_file(ctx, name: "sentences", contents: b"One short sentence.  A slightly longer sentence follows the first sentence.")?
  let balanced = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -g25 -w30 $sentences
  assert balanced == "One short sentence.\nA slightly longer sentence\nfollows the first sentence.\n"
}

test test_fmt_split_only_handles_a_paragraph_longer_than_one_buffer { |ctx|
  # 1015 words is past GNU's 998-word buffer, so the paragraph is laid out in two flushes.
  var text = " y"
  for _ in range(1014) { text += " y" }
  let input = test.temp_file(ctx, name: "long", contents: bytes.from_text(text + "\n"))?
  var line = " y"
  for _ in range(34) { line += " y" }
  var expected = ""
  for _ in range(29) { expected += line + "\n" }
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -s $input
  assert output == expected
}

test test_fmt_width_and_goal_diagnostics { |ctx|
  let script = fp"{ctx.core_dir}/fmt.xsh"
  for flag in ["-w", "-g"] {
    let invalid = run.capture --text ${ctx.xsh_bin} $script -- $flag "invalid"
    assert invalid.status.exited_with(1)
    assert invalid.stdout == ""
    assert invalid.stderr == "fmt: invalid width: 'invalid'\n", invalid.stderr
    let overflow = run.capture --text ${ctx.xsh_bin} $script -- $flag "267672676527678256"
    assert overflow.status.exited_with(1)
    let reason = if flag == "-w" { "Numerical result out of range" } else { "Value too large for defined data type" }
    assert overflow.stderr == f"fmt: invalid width: '267672676527678256': {reason}\n", overflow.stderr
  }
  let too_wide = run.capture --text ${ctx.xsh_bin} $script -- -w 2501
  assert too_wide.status.exited_with(1)
  assert too_wide.stderr == "fmt: invalid width: '2501': Numerical result out of range\n", too_wide.stderr
  for args in [["-g", "76"], ["-w", "8", "-g", "9"]] {
    let result = run.capture --text ${ctx.xsh_bin} $script -- @args
    assert result.status.exited_with(1)
    assert result.stderr == f"fmt: invalid width: '{args[args.len() - 1]}': Value too large for defined data type\n", result.stderr
  }
}

test test_fmt_goal_only_sets_width_ten_above_goal { |ctx|
  var paragraph = ""
  for _ in range(35) { paragraph += "aaaa " }
  let input = test.temp_file(ctx, name: "goal-width", contents: bytes.from_text(paragraph + "\n"))?
  for goal in [65, 75] {
    let implicit = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -q -g $goal $input
    let explicit = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -q -w ${goal + 10} -g $goal $input
    assert implicit == explicit
  }
}
