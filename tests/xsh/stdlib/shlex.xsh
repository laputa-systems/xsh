proc test_shlex_quote_and_join() [error] {
  test.eq(shlex.quote(""), "''")?
  test.eq(shlex.quote("two words"), "'two words'")?
  test.eq(shlex.quote("can't"), "'can'\\''t'")?
  test.eq(shlex.join(["install", "-m", "0644", "two words"]), "install -m 0644 'two words'")?
}

# Every case the removed native helper tests covered, plus the byte-class edge
# cases for the ASCII safe set and non-ASCII input.
proc test_shlex_quote_preserves_safe_set_and_escapes() [error] {
  test.eq(shlex.quote("install"), "install")?
  test.eq(shlex.quote("-m"), "-m")?
  test.eq(shlex.quote("a/b_c-1.2"), "a/b_c-1.2")?
  test.eq(shlex.quote("_@%+=:,./-"), "_@%+=:,./-")?
  test.eq(
    shlex.quote("""a
b"""),
    """'a
b'""",
  )?
  test.eq(shlex.quote("'"), "''\\'''")?
  test.eq(shlex.quote("a'b'c"), "'a'\\''b'\\''c'")?
  test.eq(shlex.quote("h\u{e9}llo"), "'h\u{e9}llo'")?
  test.eq(shlex.quote("\t"), "'\t'")?
  test.eq(shlex.quote("*"), "'*'")?
  test.eq(shlex.quote("!"), "'!'")?
  test.eq(shlex.quote("~"), "'~'")?
}

proc test_shlex_join_quotes_each_argument_independently() [error] {
  test.eq(shlex.join([]), "")?
  test.eq(shlex.join([""]), "''")?
  test.eq(shlex.join(["a", "b"]), "a b")?
  test.eq(
    shlex.join(["install", "two words", "can't"]),
    "install 'two words' 'can'\\''t'",
  )?
  test.eq(
    shlex.join(
  [
  """a
b""",
  "c",
],
),
    """'a
b' c""",
  )?
}
