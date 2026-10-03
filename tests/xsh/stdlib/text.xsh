test test_text_fields_replacement_and_counts {
  let row = " alpha::beta::gamma "
  let fields = row.trim().fields(delimiter: "::")
  let joined = fields.join(separator: "/")
  let replaced = joined.replace("beta", "B")
  let scalars = "h\u{e9}".split("")
  let wrapped = "alpha beta gamma".wrap(10)
  let slug = "alpha beta_gamma".translate(" _", "--")
  let deleted = "a-b_c".delete("-_")
  let squeezed = "nooo   way".squeeze(chars: " o")

  assert fields[0] == "alpha"
  assert fields[2] == "gamma"
  assert joined == "alpha/beta/gamma"
  assert replaced == "alpha/B/gamma"
  assert "desserts".reverse() == "stressed"
  assert """one
two
""".count_lines() == 2
  assert "one two".count_words() == 2
  assert "h\u{e9}".count_chars() == 2
  assert "h\u{e9}".byte_len() == 3
  assert scalars[1] == "\u{e9}"
  assert "a,b,c".split(",", maxsplit: 1) == ["a", "b,c"]
  assert "a,b,c".split(",", 1) == ["a", "b,c"]
  assert "a,b,c".split(",", maxsplit: 0) == ["a,b,c"]
  assert "a,b,c".split(",", maxsplit: -1) == ["a", "b", "c"]
  assert "abc".split("", maxsplit: 1) == ["a", "bc"]
  assert wrapped[0] == "alpha beta"
  assert wrapped[1] == "gamma"
  assert slug == "alpha-beta-gamma"
  assert deleted == "abc"
  assert squeezed == "no way"
}

# Wrapping is per input line, greedy, and measured in Unicode scalar values.
test test_text_wrap_fills_lines_and_cuts_overlong_words {
  # Empty text wraps to no lines at all, while a blank input line is a line
  # like any other and a trailing newline contributes one more empty line.
  assert "".wrap(5) == []
  assert """a

b""".wrap(5) == ["a", "", "b"]
  assert """a
""".wrap(5) == ["a", ""]
  assert "\n".wrap(5) == ["", ""]

  # A word narrower than the width is one line, a word of exactly the width is
  # one line, and a wider word is cut into pieces of exactly the width.
  assert "abc".wrap(5) == ["abc"]
  assert "abcde".wrap(5) == ["abcde"]
  assert "abcdef".wrap(5) == ["abcde", "f"]
  assert "abcdefghij".wrap(3) == ["abc", "def", "ghi", "j"]

  # Runs of whitespace separate words and every line takes greedily the words
  # that fit, so leading, trailing, and repeated whitespace disappears with
  # them rather than becoming a line or a column of its own.
  assert "one two three four".wrap(7) == ["one two", "three", "four"]
  assert "one two three four".wrap(5) == ["one", "two", "three", "four"]
  assert "one two three four".wrap(100) == ["one two three four"]
  assert "a  b".wrap(5) == ["a b"]
  assert "  a   b  ".wrap(5) == ["a b"]
  assert "  one   two  ".wrap(3) == ["one", "two"]
  assert """tab	here  new
line""".wrap(4) == ["tab", "here", "new", "line"]

  # Each input line wraps on its own, and a carriage return before a newline
  # is part of the line break rather than a word.
  assert """a
b""".wrap(5) == ["a", "b"]
  assert """a\r
b""".wrap(5) == ["a", "b"]

  # Columns count Unicode scalar values rather than bytes, so a multi-byte
  # scalar is never cut in half.
  assert "h\u{e9}llo w\u{f6}rld".wrap(4) == ["h\u{e9}ll", "o", "w\u{f6}rl", "d"]
  assert "\u{65e5}\u{672c}\u{8a9e} test".wrap(2) == ["\u{65e5}\u{672c}", "\u{8a9e}", "te", "st"]
  assert "\u{1f600}\u{1f600}\u{1f600}".wrap(2) == ["\u{1f600}\u{1f600}", "\u{1f600}"]
  assert "\u{1f600}\u{1f600}".wrap(2) == ["\u{1f600}\u{1f600}"]
}

test test_text_wrap_short_unicode_lines_keep_normalization_and_trailing_line {
  assert ("  caf\u{e9}  \u{65e5}\u{672c} \u{1f600}  " + """
short
""").wrap(72) == ["caf\u{e9} \u{65e5}\u{672c} \u{1f600}", "short", ""]
}

# Field selection is either the whitespace policy or one literal delimiter.
test test_text_fields_selects_runs_or_literal_delimiters {
  # The default delimiter selects runs of Unicode whitespace, so leading,
  # trailing, and repeated whitespace contribute no fields.
  assert "  alpha \t beta \n gamma ".fields() == ["alpha", "beta", "gamma"]
  assert "alpha  beta".fields() == ["alpha", "beta"]
  assert "".fields() == []
  assert "   ".fields() == []

  # An explicit delimiter is taken literally and empty fields between adjacent
  # delimiters are dropped.
  assert "a:b::c".fields(delimiter: ":") == ["a", "b", "c"]
  assert " a b ".fields(delimiter: " ") == ["a", "b"]
  assert "a,,b".fields(delimiter: ",") == ["a", "b"]
  assert " alpha::beta::gamma ".fields(delimiter: "::") == [" alpha", "beta", "gamma "]

  # A delimiter that never occurs leaves the whole text as one field, and an
  # empty text has no fields to leave.
  assert "alpha beta".fields(delimiter: ",") == ["alpha beta"]
  assert "".fields(delimiter: ",") == []

  # An empty delimiter is the whitespace policy again, and a multi-byte
  # delimiter is matched as a whole.
  assert "a b".fields(delimiter: "") == ["a", "b"]
  assert "a\u{e9}b".fields(delimiter: "\u{e9}") == ["a", "b"]
  assert """a\r
b""".fields(delimiter: """\r
""") == ["a", "b"]
}

# A width of zero or less is a rejection rather than an empty wrap, so it is
# observed the way a user would: through a child run of the script.
test test_text_wrap_rejects_a_non_positive_width { |ctx|
  let zero = test.run_script(
    ctx,
    """let lines = "abc".wrap(0)
print $lines.len()
""",
  )?
  {
    let assertion_condition = ! zero.success
    let assertion_message = zero.stderr
    assert assertion_condition, assertion_message
  }
  assert zero.status == 3
  assert "text-wrap" in zero.stderr
  assert "width must be positive" in zero.stderr

  let negative = test.run_script(
    ctx,
    """let lines = "abc".wrap(-2)
print $lines.len()
""",
  )?
  {
    let assertion_condition = ! negative.success
    let assertion_message = negative.stderr
    assert assertion_condition, assertion_message
  }
  assert negative.status == 3
  assert "text-wrap" in negative.stderr
  assert "width must be positive" in negative.stderr
}
