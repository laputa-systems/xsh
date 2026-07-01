error TestBaseError = Base(message: Str)

proc test_collection_number_text_status_and_result_methods() [process, error] {
  let base = ["alpha"]
  let pushed = base.push("beta")
  let extended = pushed.extend(["gamma"])
  test.eq(extended.len(), 3)?
  test.ok(extended.contains("gamma"))?
  test.eq(extended.get(0)?, "alpha")?
  test.eq(extended.get(9, "fallback"), "fallback")?
  test.eq(["a", "b", "c"].join(":"), "a:b:c")?
  test.eq(3.float().format(precision: 1), "3.0")?
  test.eq(3.2.floor()?, 3)?
  test.eq(3.2.ceil()?, 4)?
  test.eq(3.5.round()?, 4)?

  let text = """  alpha beta
beta  """

  test.eq(
    text.trim(),
    """alpha beta
beta""",
  )?

  test.ok(text.trim().starts_with("alpha"))?
  test.ok(text.trim().ends_with("beta"))?
  test.ok(text.contains("alpha"))?
  test.eq(text.trim().lines().collect().len(), 2)?
  test.eq(text.words().len(), 3)?
  test.eq("a,b,c".split(",")[1], "b")?
  test.eq("a  b\tc".fields().join(","), "a,b,c")?
  test.eq("a:b:c".fields(":").join("|"), "a|b|c")?
  test.eq("banana".replace("na", "NA"), "baNANA")?
  test.eq("abcdef".wrap(3).join("|"), "abc|def")?
  test.eq("abc".translate("ac", "AC"), "AbC")?
  test.eq("Hello.TXT".lower(), "hello.txt")?
  test.eq("Hello.txt".upper(), "HELLO.TXT")?
  test.eq("caf\u{e9}".upper(), "CAF\u{c9}")?
  test.eq("a-b-c".delete("-"), "abc")?
  test.eq("boook".squeeze("o"), "bok")?
  test.eq("abc".reverse(), "cba")?

  test.eq(
    """a
b
""".count_lines(),
    2,
  )?

  test.eq("one two".count_words(), 2)?
  test.eq("caf\u{e9}".count_chars(), 4)?
  test.eq("caf\u{e9}".count_bytes(), 5)?
  test.eq("caf\u{e9}".byte_len(), 5)?
  test.eq("caf\u{e9}".byte_at(0), 99)?
  test.eq("caf\u{e9}".byte_at(3), 195)?
  test.eq("caf\u{e9}".byte_at(4), 169)?
  test.eq("caf\u{e9}".byte_at(9), -1)?
  test.eq("caf\u{e9}".byte_at(9, default: 0), 0)?
  test.eq("caf\u{e9}".byte_slice(0, 3), "caf")?
  test.eq("caf\u{e9}".byte_slice(3), "\u{e9}")?

  test.eq(
    """alpha
beta""".find("\n"),
    5,
  )?

  test.eq(
    """alpha
beta""".find("a", 1),
    4,
  )?

  test.eq(
    """alpha
beta""".find("z"),
    -1,
  )?

  test.eq("42".parse_int()?, 42)?
  test.error_kind("nope".parse_int(), "parse-int")?
  let status = run.status false
  test.ok(status.exited())?
  test.ok(! status.signaled())?
  test.ok(status.exited_with(1))?
  test.eq(status.exit_code()?, 1)?
  test.error_kind(status.signal_number(), "status-kind")?
  let result: Result[Int] = Err(TestBaseError.Base(message: "base message"))
  test.error_kind(result.context("wrapped", "extra"), "TestBaseError.Base")?
}
