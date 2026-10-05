const backslash = "\\"

# A script whose lines are `lines`; a line ending in `backslash` continues.
pure script(lines: List[Str]) -> Str {
  lines.join("\n") + "\n"
}

test test_backslash_continues_command_arguments { |ctx|
  let source = script(
    [
      f"run printf \"%s|\" {backslash}",
      f"  one \"two words\" {backslash}",
      "  three ?",
      f"print {backslash}",
      f"  alpha {backslash}",
      "  beta # a comment may end the last line",
    ],
  )
  let output = test.run_script(ctx, source)?
  assert output.success, output.stderr
  assert output.stdout == "one|two words|three|alpha beta\n", output.stdout
}

test test_continuation_separates_every_part_of_a_run_form { |ctx|
  let source = script(
    [
      f"let text = run.text {backslash}",
      f"  --timeout=5s {backslash}",
      f"  GREETING=hello {backslash}",
      f"  printenv {backslash}",
      f"  GREETING {backslash}",
      f"  | run tr a-z A-Z {backslash}",
      f"  2> /dev/null {backslash}",
      "  ?",
      "print $text",
      f"env FIRST=1 {backslash}",
      f"  SECOND=2 {backslash}",
      "  {",
      "  run printenv SECOND ?",
      "}",
    ],
  )
  let output = test.run_script(ctx, source)?
  assert output.success, output.stderr
  assert output.stdout == "HELLO\n\n2\n", output.stdout
}

test test_continuation_accepts_crlf_line_endings { |ctx|
  let output = test.run_script(ctx, f"print one {backslash}\r\n  two\r\n")?
  assert output.success, output.stderr
  assert output.stdout == "one two\n", output.stdout
}

test test_backslash_must_end_its_line_after_a_space { |ctx|
  let cases = [
    {name: "trailing space", source: f"print one {backslash} \n  two\n"},
    {name: "comment", source: f"print one {backslash} # note\n  two\n"},
    {name: "glued to a word", source: f"print one{backslash}\n  two\n"},
    {name: "mid line", source: f"print one {backslash}two\n"},
  ]
  for item in cases {
    let output = test.run_script(ctx, item.source)?
    assert ! output.success, item.name
    assert "err[lex.unexpected-character]" in output.stderr, f"{item.name}: {output.stderr}"
    assert "err[parse.line-continuation]" not in output.stderr, f"{item.name}: {output.stderr}"
  }
}

test test_continuation_outside_a_command_is_rejected { |ctx|
  let cases = [
    {name: "expression", source: f"let total = 1 + {backslash}\n  2\nprint \$total\n"},
    {name: "typed argument", source: f"print (1 + {backslash}\n  2)\n"},
    {name: "call arguments", source: f"let size = [1, {backslash}\n  2].len()\nprint \$size\n"},
    {name: "blank line follows", source: f"print one {backslash}\n\nprint two\n"},
    {name: "comment line follows", source: f"print one {backslash}\n  # note\n  two\n"},
    {name: "end of file", source: f"print one {backslash}\n"},
    {name: "end of block", source: f"if true {{\n  print one {backslash}\n}}\n"},
    {name: "between statements", source: f"let n = 1 {backslash}\nprint \$n\n"},
  ]
  for item in cases {
    let output = test.run_script(ctx, item.source)?
    assert ! output.success, item.name
    assert output.stderr.split("err[parse.line-continuation]").len() == 2, f"{item.name}: {output.stderr}"
    assert "a `\\` joins lines only between the parts of a command" in output.stderr, output.stderr
  }
}

test test_backslash_in_a_quoted_word_is_an_escape { |ctx|
  let output = test.run_script(ctx, f"print \"one {backslash}\n  two\"\n")?
  assert ! output.success
  assert "err[lex.invalid-escape]" in output.stderr, output.stderr
  assert "err[parse.line-continuation]" not in output.stderr, output.stderr
}

test test_grep_matches_inside_a_continued_command { |ctx|
  let source = script(
    [
      "let names = [\"a\"]",
      f"run printf \"%s\" {backslash}",
      f"  (names.len() + 1) {backslash}",
      "  names.len() ?",
    ],
  )
  let file = test.temp_file(ctx, name: "continued.xsh", contents: bytes.from_text(source))?
  let found = run.capture --text "xsht" grep "X.len()" $file ?
  assert found.status.exited_with(0), found.stderr
  assert ":3:" in found.stdout and ":4:" in found.stdout, found.stdout
  assert "2 matches" in found.stdout, found.stdout
}

test test_highlight_colors_a_continued_command { |ctx|
  let source = script([f"run printf {backslash}", "  one ?"])
  let file = test.temp_file(ctx, name: "continued.xsh", contents: bytes.from_text(source))?
  let shown = run.capture --text "xsht" highlight $file ?
  assert shown.status.exited_with(0), shown.stderr
  assert r"""{"kind":"punctuation","text":"\\"}""" in shown.stdout, shown.stdout
  assert r"""{"kind":"function","text":"printf"}""" in shown.stdout, shown.stdout
}
