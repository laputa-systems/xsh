test test_shlex_quote_and_join [error] {
  shlex.quote("") == "''"
  shlex.quote("two words") == "'two words'"
  shlex.quote("can't") == "'can'\\''t'"
  shlex.join(["install", "-m", "0644", "two words"]) == "install -m 0644 'two words'"
}

# Every case the removed native helper tests covered, plus the byte-class edge
# cases for the ASCII safe set and non-ASCII input.
test test_shlex_quote_preserves_safe_set_and_escapes [error] {
  shlex.quote("install") == "install"
  shlex.quote("-m") == "-m"
  shlex.quote("a/b_c-1.2") == "a/b_c-1.2"
  shlex.quote("_@%+=:,./-") == "_@%+=:,./-"
  shlex.quote("""a
b""") == """'a
b'"""
  shlex.quote("'") == "''\\'''"
  shlex.quote("a'b'c") == "'a'\\''b'\\''c'"
  shlex.quote("h\u{e9}llo") == "'h\u{e9}llo'"
  shlex.quote("\t") == "'\t'"
  shlex.quote("*") == "'*'"
  shlex.quote("!") == "'!'"
  shlex.quote("~") == "'~'"
}

test test_shlex_join_quotes_each_argument_independently [error] {
  shlex.join([]) == ""
  shlex.join([""]) == "''"
  shlex.join(["a", "b"]) == "a b"
  shlex.join(["install", "two words", "can't"]) == "install 'two words' 'can'\\''t'"
  shlex.join(
  [
  """a
b""",
  "c",
],
) == """'a
b' c"""
}
