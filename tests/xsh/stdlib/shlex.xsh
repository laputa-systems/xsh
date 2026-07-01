proc test_shlex_quote_and_join() [error] {
  test.eq(shlex.quote(""), "''")?
  test.eq(shlex.quote("two words"), "'two words'")?
  test.eq(shlex.quote("can't"), "'can'\\''t'")?
  test.eq(shlex.join(["install", "-m", "0644", "two words"]), "install -m 0644 'two words'")?
}
