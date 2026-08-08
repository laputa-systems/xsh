error TestBaseError = Base(message: Str)

proc test_collection_number_text_status_and_result_methods() [process, error] {
  let base = ["alpha"]
  let pushed = base.push("beta")
  let extended = pushed.extend(["gamma"])
  test.eq(extended.len(), 3)?
  test.ok("gamma" in extended)?
  test.eq(extended.get(0)?, "alpha")?
  test.eq(extended.get(9, "fallback"), "fallback")?
  test.eq(["a", "b", "c"].join(":"), "a:b:c")?
  test.eq(3.float().format(precision: 1), "3.0")?
  test.eq(3.2.floor()?, 3)?
  test.eq(3.2.ceil()?, 4)?
  test.eq(3.5.round()?, 4)?
  test.eq("3.14159".parse_float()?, 3.14159)?
  test.error_kind("not-a-number".parse_float(), "parse-float")?
  test.eq(16.0.sqrt(), 4.0)?
  test.eq(2.0.pow(3.0), 8.0)?
  test.eq((-3.5).abs(), 3.5)?
  test.eq(0.0.sin(), 0.0)?
  test.eq(0.0.cos(), 1.0)?
  let exp_roundtrip = 2.0.ln().exp()
  test.ok((exp_roundtrip - 2.0).abs() < 0.00000000000001)?

  let text = """  alpha beta
beta  """

  test.eq(
    text.trim(),
    """alpha beta
beta""",
  )?

  test.ok(text.trim().starts_with("alpha"))?
  test.ok(text.trim().ends_with("beta"))?
  test.ok("alpha" in text)?
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
  test.eq("42".parse_int_decimal()?, 42)?
  test.eq("42".parse_uint()?, 42)?
  test.eq("0".parse_uint()?, 0)?
  test.error_kind("+42".parse_uint(), "parse-uint")?
  test.error_kind("-1".parse_uint(), "parse-uint")?
  test.eq("42".parse_uint_positive()?, 42)?
  test.eq(" 42 ".parse_uint_positive()?, 42)?
  test.error_kind("0".parse_uint_positive(), "parse-uint-positive")?
  test.error_kind("+42".parse_uint_positive(), "parse-uint-positive")?
  test.error_kind("-1".parse_uint_positive(), "parse-uint-positive")?
  test.error_kind("0x2a".parse_uint_positive(), "parse-uint-positive")?
  test.error_kind("nope".parse_uint_positive(), "parse-uint-positive")?
  test.error_kind("0x10".parse_int_decimal(), "parse-int")?
  test.error_kind("+5".parse_int_decimal(), "parse-int")?
  test.error_kind(" 5 ".parse_int_decimal(), "parse-int")?
  test.error_kind("05".parse_int_decimal(), "parse-int")?
  test.error_kind("nope".parse_int(), "parse-int")?
  test.eq("hello" + " " + "world", "hello world")?
  let name = "Alice"
  test.eq("Hello, " + name + "!", "Hello, Alice!")?
  test.eq("a" + "b" + "c", "abc")?
  let status = run.status false
  test.ok(status.exited())?
  test.ok(! status.signaled())?
  test.ok(status.exited_with(1))?
  test.eq(status.exit_code()?, 1)?
  test.error_kind(status.signal_number(), "status-kind")?
  let result: Result[Int] = Err(TestBaseError.Base(message: "base message"))
  test.error_kind(result.context("wrapped", "extra"), "TestBaseError.Base")?
}
