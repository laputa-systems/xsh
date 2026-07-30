proc test_text_fields_replacement_and_counts() [error] {
  let row = " alpha::beta::gamma "
  let fields = row.trim().fields(delimiter: "::")
  let joined = fields.join(separator: "/")
  let replaced = joined.replace("beta", "B")
  let scalars = "h\u{e9}".split("")
  let wrapped = "alpha beta gamma".wrap(10)
  let slug = "alpha beta_gamma".translate(" _", "--")
  let deleted = "a-b_c".delete("-_")
  let squeezed = "nooo   way".squeeze(chars: " o")

  test.eq(fields[0], "alpha")?
  test.eq(fields[2], "gamma")?
  test.eq(joined, "alpha/beta/gamma")?
  test.eq(replaced, "alpha/B/gamma")?
  test.eq("desserts".reverse(), "stressed")?
  test.eq(
    """one
two
""".count_lines(),
    2,
  )?
  test.eq("one two".count_words(), 2)?
  test.eq("h\u{e9}".count_chars(), 2)?
  test.eq("h\u{e9}".count_bytes(), 3)?
  test.eq(scalars[1], "\u{e9}")?
  test.eq(wrapped[0], "alpha beta")?
  test.eq(wrapped[1], "gamma")?
  test.eq(slug, "alpha-beta-gamma")?
  test.eq(deleted, "abc")?
  test.eq(squeezed, "no way")?
}
