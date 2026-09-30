test test_block_strings_remove_structural_breaks_and_exact_margin [error] {
  let value = """
    first
      second
    """
  value == "first\n  second"
  let empty = """
    """
  empty == ""
  let newline = """
    first

    """
  newline == "first\n"
  let raw = r"""
    \n ${literal}
    """
  raw == r"\n ${literal}"
}

test test_block_strings_keep_blank_line_whitespace_and_raw_backslashes [error] {
  let blank_lines = """
    first
  
      
    last
    """
  blank_lines == "first\n\n  \nlast"
  let unicode_blank = """
    first
 
    last
    """
  unicode_blank == "first\n\u{a0}\nlast"
  let escapes = """
    \tword\nnext
    """
  escapes == "\tword\nnext"
  let raw = r"""
    \tword\nnext
    """
  raw == r"\tword\nnext"
}

test test_block_strings_keep_interpolated_newlines_and_nested_source [error] {
  let inserted = "one\nno source margin"
  let formatted = f"""
    before
    $inserted
    after
    """
  formatted == "before\none\nno source margin\nafter"
  let marker = r"${not_an_expression}"
  let rendered = f"""
    before
    $marker
    after
    """
  rendered == "before\n" + marker + "\nafter"
  let nested = f"""
    before
    ${if true {
      r"""nested
  exact"""
} else { "other" }}
    after
    """
  nested == "before\nnested\n  exact\nafter"

}

test test_block_strings_leave_nonblock_and_other_literal_domains_exact [error] {
  let inline_opening = """first
    last
    """
  inline_opening == "first\n    last\n    "
  let inline_closing = """
    first
    """ + ""
  inline_closing == "\n    first\n    "
  let data = b"""
    first
    """
  data == b"\n    first\n    "
  let path_value = p"""
    first
    """
  path_value.display() == "\n    first\n    "
  let formatted_path = fp"""
    ${"first"}
    """
  formatted_path.display() == "\n    first\n    "
}

test test_block_strings_reject_missing_exact_space_tab_prefix [error] { |ctx|
  for source in [
    "let value = \"\"\"\n  good\n bad\n  \"\"\"\n",
    "let value = \"\"\"\n\tgood\n good\n\t\"\"\"\n",
    "let value = f\"\"\"\n  \${1}\n wrong\n  \"\"\"\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, rejected.stderr
    "parse.block-string-margin" in rejected.stderr
  }
}

test test_block_strings_keep_interpolation_evaluation_order [error] { |ctx|
  let source = "proc part(label: Str) [io] -> Str { print $label; label + \"\\nnext\" }\nlet value = f\"\"\"\n  \${part(\"left\")}\n  \${part(\"right\")}\n  \"\"\"\nprint $value\n"
  let executed = test.run_script(ctx, source)?
  assert executed.success, executed.stderr
  executed.stdout == "left\nright\nleft\nnext\nright\nnext\n"
}

test test_block_strings_preserve_crlf_tabs_and_explicit_final_newlines [error] { |ctx|
  let source = "let value = \"\"\"\r\n\t first\r\n\t   second\r\n\t \"\"\"\nprint $value\n"
  let executed = test.run_script(ctx, source)?
  assert executed.success, executed.stderr
  executed.stdout == "first\r\n  second\n"
}

test test_block_string_formatter_preserves_values_and_converges [fs, process, error] { |ctx|
  let source = "let text = \"\"\"\n  first\n    second\n\n  \"\"\"\nlet leading = \"\\nfirst\\n\"\nlet inserted = \"left\\nright\"\nlet formatted = f\"\"\"\n  before\n  $inserted\n  after\n  \"\"\"\nprint \${json.encode(text)?} \${json.encode(leading)?} \${json.encode(formatted)?}\n"
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "block-string-format.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let fixed = candidate.read_text()?
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  after.stdout == before.stdout
  let repeated = run.capture --text "xsht" fmt $candidate ?
  assert repeated.status.exited_with(0), repeated.stderr
  candidate.read_text()? == fixed
}

test test_block_string_lint_preserves_literal_bytes_and_converges [fs, process, error] { |ctx|
  let source = "let value = \"first\\n\" + \"  second\\n\"\nprint \${json.encode(value)?}\n"
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "block-string-fix.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  "value = \"\"\"\n" in fixed
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  after.stdout == before.stdout
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  assert repeated.status.exited_with(0), repeated.stderr
  candidate.read_text()? == fixed
}

test test_block_strings_share_layout_with_quoted_command_words [error] { |ctx|
  let source = "let name = \"demo\"\nprint \"\"\"\n  hello $name\n  \${if true { \"inside\" } else { \"other\" }}\n  \"\"\"\n"
  let executed = test.run_script(ctx, source)?
  assert executed.success, executed.stderr
  executed.stdout == "hello demo\ninside\n"
  let raw_source = "print r\"\"\"\n  literal $name\n  \"\"\"\n"
  let raw_executed = test.run_script(ctx, raw_source)?
  assert raw_executed.success, raw_executed.stderr
  raw_executed.stdout == "literal $name\n"
}
