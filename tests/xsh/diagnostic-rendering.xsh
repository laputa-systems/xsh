test test_diagnostic_bounds_a_long_source_line_and_underline { |ctx|
  let padding = ["a" for _ in range(10000)].join("")
  let output = test.expect(ctx, "let value: Int = \"" + padding + "\"\n", status: 2)?
  assert "check.type-mismatch" in output.stderr, output.stderr
  let lines = output.stderr.lines()
  assert lines[2].count_chars() <= 160, lines[2]
  assert "…" in lines[2], lines[2]
  let markers = lines[3].split(" expected ")[0]
  assert markers.count_chars() <= 160, markers
  assert "^" in markers, markers
  assert ":1:18\n" in output.stderr, output.stderr
}

test test_diagnostic_clips_before_a_distant_unicode_label { |ctx|
  let padding = ["α" for _ in range(5000)].join("")
  let prefix = "let padding = \"" + padding + "\"; "
  let output = test.expect(ctx, prefix + "let value: Int = false\n", status: 2)?
  assert "check.type-mismatch" in output.stderr, output.stderr
  assert f":1:{prefix.count_chars() + 18}\n" in output.stderr, output.stderr
  let lines = output.stderr.lines()
  assert lines[2].starts_with("  …"), lines[2]
  assert "α" in lines[2] and lines[2].ends_with("false"), lines[2]
  assert lines[2].count_chars() <= 160, lines[2]
  let markers = lines[3].split(" expected ")[0]
  assert markers.count_chars() <= 160, markers
  assert "^^^^^" in markers, markers
}

test test_diagnostic_bounds_a_multiline_label { |ctx|
  let padding = ["α" for _ in range(5000)].join("")
  let source = "let value: Int = \"\"\"" + padding + "\ncontinued\"\"\"\n"
  let output = test.expect(ctx, source, status: 2)?
  assert "check.type-mismatch" in output.stderr, output.stderr
  assert ":1:18\n" in output.stderr, output.stderr
  let lines = output.stderr.lines()
  assert lines[2].count_chars() <= 160, lines[2]
  assert "…" in lines[2] and "α" in lines[2], lines[2]
  let markers = lines[3].split(" expected ")[0]
  assert markers.count_chars() <= 160, markers
  assert markers.ends_with("…"), markers
}

test test_diagnostic_keeps_an_end_of_line_marker_visible { |ctx|
  let padding = ["a" for _ in range(5000)].join("")
  let source = "let padding = \"" + padding + "\"; let value ="
  let output = test.expect(ctx, source, status: 2)?
  assert "parse.expected-expression" in output.stderr, output.stderr
  assert f":1:{source.count_chars() + 1}\n" in output.stderr, output.stderr
  let lines = output.stderr.lines()
  assert lines[2].count_chars() <= 160, lines[2]
  assert lines[2].ends_with("let value ="), lines[2]
  let markers = lines[3].split(" expected ")[0]
  assert markers.count_chars() <= 160, markers
  assert markers.ends_with("^"), markers
}
