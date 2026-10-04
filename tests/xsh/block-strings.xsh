test test_block_strings_remove_structural_breaks_and_exact_margin {
  let value = """
    first
      second
    """
  assert value == """first
  second"""
  let empty = """
    """
  assert empty == ""
  let newline = """
    first

    """
  assert newline == """first
"""
  let raw = r"""
    \n ${literal}
    """
  assert raw == r"\n ${literal}"
}

test test_block_strings_keep_blank_line_whitespace_and_raw_backslashes {
  let blank_lines = """
    first
  
      
    last
    """
  assert blank_lines == """first

  
last"""
  let unicode_blank = """
    first
 
    last
    """
  assert unicode_blank == """first
 
last"""
  let escapes = """
    \tword\nnext
    """
  assert escapes == """	word
next"""
  let raw = r"""
    \tword\nnext
    """
  assert raw == r"\tword\nnext"
}

test test_block_strings_keep_interpolated_newlines_and_nested_source {
  let inserted = """one
no source margin"""
  let formatted = f"""
    before
    {inserted}
    after
    """
  assert formatted == """before
one
no source margin
after"""
  let marker = r"${not_an_expression}"
  let rendered = f"""
    before
    {marker}
    after
    """
  assert rendered == """before
""" + marker + """\nafter"""
  let nested = f"""
    before
    {if true {
      r"""nested
  exact"""
} else { "other" }}
    after
    """
  assert nested == """before
nested
  exact
after"""
}

test test_block_strings_leave_nonblock_and_other_literal_domains_exact {
  let inline_opening = """first
    last
    """
  assert inline_opening == """first
    last
    """
  let inline_closing = """
    first
    """ + ""
  assert inline_closing == """\n    first
    """
  let data = b"\n    first\n    "
  assert data == b"\n    first\n    "
  let path_value = p"\n    first\n    "
  assert path_value.display() == """\n    first
    """
  let formatted_path = fp"""
    {"first"}
    """
  assert formatted_path.display() == """\n    first
    """
}

test test_block_strings_reject_missing_exact_space_tab_prefix { |ctx|
  for source in [
    """let value = \"""
  good
 bad
  \"""
""",
    """let value = \"""
	good
 good
	\"""
""",
    """let value = f\"""
  {1}
 wrong
  \"""
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, rejected.stderr
    assert "parse.block-string-margin" in rejected.stderr
  }
}

test test_block_strings_keep_interpolation_evaluation_order { |ctx|
  let source = """proc part(label: Str) [io] -> Str { print $label; label + "\\nnext" }
let value = f\"""
  {part("left")}
  {part("right")}
  \"""
print $value
"""
  let executed = test.run_script(ctx, source)?
  assert executed.success, executed.stderr
  assert executed.stdout == """left
right
left
next
right
next
"""
}

test test_block_strings_preserve_crlf_tabs_and_explicit_final_newlines { |ctx|
  let source = """let value = \"""\r
	 first\r
	   second\r
	 \"""
print $value
"""
  let executed = test.run_script(ctx, source)?
  assert executed.success, executed.stderr
  assert executed.stdout == """first\r
  second
"""
}

test test_block_string_formatter_preserves_values_and_converges { |ctx|
  let source = """let text = \"""
  first
    second

  \"""
let leading = "\\nfirst\\n"
let inserted = "left\\nright"
let formatted = f\"""
  before
  {inserted}
  after
  \"""
print \${json.encode(text)?} \${json.encode(leading)?} \${json.encode(formatted)?}
"""
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "block-string-format.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let fixed = candidate.read_text()?
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let repeated = run.capture --text "xsht" fmt $candidate ?
  assert repeated.status.exited_with(0), repeated.stderr
  assert candidate.read_text()? == fixed
}

test test_block_string_lint_preserves_literal_bytes_and_converges { |ctx|
  let source = """let value = "first\\n" + "  second\\n"
print \${json.encode(value)?}
"""
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "block-string-fix.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  assert """value = \"""
""" in fixed
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  assert repeated.status.exited_with(0), repeated.stderr
  assert candidate.read_text()? == fixed
}

test test_block_strings_share_layout_with_quoted_command_words { |ctx|
  let source = """let name = "demo"
print \"""
  hello $name
  \${if true { "inside" } else { "other" }}
  \"""
"""
  let executed = test.run_script(ctx, source)?
  assert executed.success, executed.stderr
  assert executed.stdout == """hello demo
inside
"""
  let raw_source = """print r\"""
  literal $name
  \"""
"""
  let raw_executed = test.run_script(ctx, raw_source)?
  assert raw_executed.success, raw_executed.stderr
  assert raw_executed.stdout == """literal $name
"""
}
