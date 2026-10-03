test test_shlex_quote_and_join {
  assert shlex.quote("") == "''"
  assert shlex.quote("two words") == "'two words'"
  assert shlex.quote("can't") == "'can'\\''t'"
  assert shlex.join(["install", "-m", "0644", "two words"]) == "install -m 0644 'two words'"
}

# Every case the removed native helper tests covered, plus the byte-class edge
# cases for the ASCII safe set and non-ASCII input.
test test_shlex_quote_preserves_safe_set_and_escapes {
  assert shlex.quote("install") == "install"
  assert shlex.quote("-m") == "-m"
  assert shlex.quote("a/b_c-1.2") == "a/b_c-1.2"
  assert shlex.quote("_@%+=:,./-") == "_@%+=:,./-"
  assert shlex.quote("""a
b""") == """'a
b'"""
  assert shlex.quote("'") == "''\\'''"
  assert shlex.quote("a'b'c") == "'a'\\''b'\\''c'"
  assert shlex.quote("h\u{e9}llo") == "'h\u{e9}llo'"
  assert shlex.quote("\t") == "'\t'"
  assert shlex.quote("*") == "'*'"
  assert shlex.quote("!") == "'!'"
  assert shlex.quote("~") == "'~'"
}

test test_shlex_join_quotes_each_argument_independently {
  assert shlex.join([]) == ""
  assert shlex.join([""]) == "''"
  assert shlex.join(["a", "b"]) == "a b"
  assert shlex.join(["install", "two words", "can't"]) == "install 'two words' 'can'\\''t'"
  assert shlex.join(
    [
  """a
b""",
  "c",
],
  ) == """'a
b' c"""
}
