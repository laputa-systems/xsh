test test_nl_numbering_sections_and_formats { |ctx|
  let input = test.temp_file(ctx, name: "numbered.txt", contents: b"one\n\\:\\:\ntwo\n")?
  let default = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" $input ?
  let zeros = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -n rz -v -2 -w 4 $input ?
  assert default == "     1\tone\n\n     1\ttwo\n"
  assert zeros.starts_with("-002\tone\n")
}

test test_nl_custom_separator_and_all_lines { |ctx|
  let input = test.temp_file(ctx, name: "all-lines.txt", contents: b"a\n\nb\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -ba -s "|" -w 2 $input ?
  assert output == " 1|a\n 2|\n 3|b\n"
}
