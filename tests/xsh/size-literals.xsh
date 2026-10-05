const chunk = 64KiB

pure doubled(size: UInt) -> UInt {
  size * 2
}

test test_size_literals_count_bytes {
  assert 1KiB == 1024 and 1MiB == 1048576 and 1GiB == 1073741824
  assert 1KB == 1000 and 1MB == 1000000 and 1GB == 1000000000
  assert 0KiB == 0
  assert chunk == 65536
  assert 8589934591GiB == 9223372035781033984
}

test test_size_literal_is_a_plain_unsigned_integer {
  # There is no size type: the literal displays and computes as its bytes.
  assert f"{64MiB}" == "67108864"
  assert 64MiB / 1MiB == 64
  assert 4KiB + 96 == 4192
  assert 1MiB - 2MiB == -1048576
  assert doubled(16KiB) == 32KiB
  let signed: Int = 5KB
  assert signed == 5000
  let label = match chunk {
    64KiB => "chunk",
    else => "other",
  }
  assert label == "chunk"
}

test test_size_literal_binding_is_unsigned { |ctx|
  # A binding inferred from a size literal is `UInt`, for `let` and `const`.
  for keyword in ["let", "const"] {
    let source = f"{keyword} limit = 1MiB\nvar left = limit\nleft = left - 2MiB\nprint f\"{{left}}\"\n"
    let output = test.run_script(ctx, source)?
    assert ! output.success, keyword
    assert "violates UInt constraint" in output.stderr, f"{keyword}: {output.stderr}"
  }

  let annotated = test.expect(ctx, "var left: Int = 1MiB\nleft = left - 2MiB\nprint f\"{left}\"\n", status: 0)?
  assert annotated.stdout == "-1048576\n"
}

test test_size_literal_in_a_collection_is_unsigned_for_let_and_const { |ctx|
  # A `const` types its data from its values, which used to make a size
  # literal inside a list, map, or record `Int` where `let` made it `UInt`.
  let cases = [
    {name: "list", data: "[1MiB, 4MiB]", element: "data[0]"},
    {name: "nested list", data: "[[1MiB], [4MiB]]", element: "data[0][0]"},
    {name: "record", data: "{soft: 1MiB, hard: 4MiB}", element: "data.soft"},
    {name: "record in list", data: "[{soft: 1MiB}]", element: "data[0].soft"},
    {name: "map", data: "{[\"soft\"]: 1MiB}", element: "data.get(\"soft\")?"},
  ]
  for keyword in ["let", "const"] {
    for item in cases {
      let source = f"{keyword} data = {item.data}\nvar left = {item.element}\nleft = left - 8MiB\nprint f\"{{left}}\"\n"
      let output = test.run_script(ctx, source)?
      assert ! output.success, f"{keyword} {item.name}: {output.stdout}"
      assert "violates UInt constraint" in output.stderr, f"{keyword} {item.name}: {output.stderr}"
    }
  }

  # A written element type still decides.
  let annotated = test.expect(
    ctx,
    "const data: List[Int] = [1MiB]\nvar left = data[0]\nleft = left - 8MiB\nprint f\"{left}\"\n",
    status: 0,
  )?
  assert annotated.stdout == "-7340032\n"
}

test test_size_literal_out_of_range_is_a_check_error { |ctx|
  let output = test.run_script(ctx, "let size = 8589934592GiB\nprint f\"{size}\"\n")?
  assert ! output.success
  assert "err[check.size-literal]" in output.stderr, output.stderr
  assert "size literal exceeds 9223372036854775807 bytes" in output.stderr, output.stderr
}

test test_size_unit_must_end_the_literal { |ctx|
  let cases = [
    {name: "fraction", source: "let size = 1.5MiB\n"},
    {name: "longer word", source: "let rate = 1KBps\n"},
    {name: "spaced unit", source: "let size = 4 KiB\n"},
    {name: "lowercase unit", source: "let size = 4kib\n"},
  ]
  for item in cases {
    let output = test.run_script(ctx, item.source)?
    assert ! output.success, item.name
    assert "err[check.size-literal]" not in output.stderr, f"{item.name}: {output.stderr}"
  }
}

test test_size_spelling_in_a_command_word_stays_text { |ctx|
  let output = test.expect(ctx, "run printf \"%s %s\\n\" 64KB (64KB) ?\n", status: 0)?
  assert output.stdout == "64KB 64000\n", output.stdout
}

test test_fmt_and_highlight_keep_the_literal { |ctx|
  let source = "let size   =  64MiB\nprint f\"{size}\"\n"
  let file = test.temp_file(ctx, name: "sizes.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == "let size = 64MiB\nprint f\"{size}\"\n"
  let shown = run.capture --text "xsht" highlight $file
  assert shown.status.exited_with(0), shown.stderr
  assert r"""{"kind":"number","text":"64MiB"}""" in shown.stdout, shown.stdout
}

test test_lint_rewrites_literal_products_of_1024 { |ctx|
  let source = "let size: UInt = 3 * 1024 * 1024\nassert size < (4 * 1024 * 1024)\nlet chunk = 64 * 1024\nprint f\"{size} {chunk}\"\n"
  let file = test.temp_file(ctx, name: "products.xsh", contents: bytes.from_text(source))?
  let reported = run.capture --text "xsht" lint --only lint.prefer-size-literal $file
  assert reported.stderr.split("warn[lint.prefer-size-literal]").len() == 4, reported.stderr
  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-size-literal $file
  # The untyped binding would become `UInt`, so it is reported and left alone.
  assert file.read_text()? == "let size: UInt = 3MiB\nassert size < 4MiB\nlet chunk = 64 * 1024\nprint f\"{size} {chunk}\"\n", fixed.stderr
  let output = test.expect(ctx, file.read_text()?, status: 0)?
  assert output.stdout == "3145728 65536\n"
}
