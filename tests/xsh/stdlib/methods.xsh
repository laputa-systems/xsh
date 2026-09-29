error TestBaseError = Base(message: Str)

test test_list_push_and_extend_preserve_older_values [error] {
  let base = [1, 2]
  let alias = base
  let pushed = base.push(3)
  let extended = pushed.extend([4, 5])

  test.eq(base, [1, 2])?
  test.eq(alias, [1, 2])?
  test.eq(pushed, [1, 2, 3])?
  test.eq(extended, [1, 2, 3, 4, 5])?
}

test test_list_concatenation_and_compound_assignment_preserve_aliases [error] {
  var items: List[Int] = []
  items += []
  items += [1]
  let alias = items
  items += [2, 3]
  test.eq(alias, [1])?
  test.eq(items, [1, 2, 3])?
  items += items
  test.eq(items, [1, 2, 3, 1, 2, 3])?
  test.eq(alias, [1])?
  let joined = alias + [4, 5]
  test.eq(joined, alias.extend([4, 5]))?
  test.eq([] + [6], [6])?
  test.eq([6] + [], [6])?
  let empty: List[Str] = [] + []
  test.eq(empty, [])?
  var container = {items: [1]}
  let record_alias = container
  container.items += [2]
  test.eq(container.items, [1, 2])?
  test.eq(record_alias.items, [1])?
  container.items += container.items
  test.eq(container.items, [1, 2, 1, 2])?
  test.eq(record_alias.items, [1])?
  var table: Map[List[Int]] = map.empty().set("entry", [1])
  let table_alias = table
  table["entry"] += [2]
  test.eq(table.get("entry")?, [1, 2])?
  test.eq(table_alias.get("entry")?, [1])?
  table["entry"] += table.get("entry")?
  test.eq(table.get("entry")?, [1, 2, 1, 2])?
  test.eq(table_alias.get("entry")?, [1])?
}

test test_list_compound_assignment_checks_targets_and_elements [error] { |ctx|
  for source in [
    "var items = [1]\nitems += 2\n",
    "var items = [1]\nitems += [\"wrong\"]\n",
    "let items = [1]\nitems += [2]\n",
    "var items = [1]\nitems -= [2]\n",
    "let items = [1] + [\"wrong\"]\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.contains(result.stderr, "check.")?
  }
}

test test_list_compound_assignment_evaluates_selectors_before_rhs_once [error] { |ctx|
  let result = test.run_script(
    ctx,
    r"""proc key() [io] -> Str {
  print "selector"
  return "entry"
}
proc more() [io] -> List[Int] {
  print "rhs"
  return [2]
}
proc item() [io] -> Int {
  print "item"
  return 3
}
var table: Map[List[Int]] = map.empty().set("entry", [1])
table[key()] += more()
table[key()] += [item()]
let values = table.get("entry")?
print ${values[0]} ${values[1]} ${values[2]}
""",
  )?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "selector\nrhs\nselector\nitem\n1 2 3\n")?
}

test test_list_concatenation_evaluates_operands_once_in_source_order [error] { |ctx|
  let result = test.run_script(
    ctx,
    r"""proc left() [io] -> List[Int] {
  print "left"
  return [1]
}
proc right() [io] -> List[Int] {
  print "right"
  return [2]
}
let values = left() + right()
print values.len()
""",
  )?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "left\nright\n2\n")?
}

test test_list_compound_assignment_retains_target_on_dynamic_rhs_failure [error] { |ctx|
  let result = test.run_script(
    ctx,
    r"""pure wrong() -> Any {
  return 2
}
proc report(values: List[Int]) [io] {
  print values.len()
}
proc main() [io] {
  var values = [1]
  defer report(values)
  values += wrong()
}
""",
  )?
  test.ok(! result.success, result.stderr)?
  test.contains(result.stderr, "type-error")?
  test.eq(result.stdout, "1\n")?
}

test test_collection_number_text_status_and_result_methods [process, error] {
  let base = ["alpha"]
  let pushed = base.push("beta")
  let extended = pushed.extend(["gamma"])
  test.eq(extended.len(), 3)?
  test.ok("gamma" in extended)?
  test.eq(extended.get(0)?, "alpha")?
  test.eq((extended.get(9) ?? "fallback"), "fallback")?
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
  test.eq(("caf\u{e9}".byte_at(0) ?? -1), 99)?
  test.eq(("caf\u{e9}".byte_at(3) ?? -1), 195)?
  test.eq(("caf\u{e9}".byte_at(4) ?? -1), 169)?
  test.eq("caf\u{e9}".byte_at(9), null)?
  test.eq(("caf\u{e9}".byte_at(9) ?? 0), 0)?
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
    null,
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

test test_int_bitset_methods [fs, error] { |ctx|
  let mode = 0o754
  test.eq(mode.bit_and(0o070), 0o050)?
  test.eq(mode.bit_or(0o002), 0o756)?
  test.eq(mode.clear_bits(0o054), 0o700)?

  let negative_receiver = test.run_script(
    ctx,
    """let value = -1
value.bit_and(1)
""",
  )?
  test.ok(! negative_receiver.success, negative_receiver.stderr)?
  test.contains(negative_receiver.stderr, "integer-bitset")?

  let negative_mask = test.run_script(
    ctx,
    """let mask = -1
1.clear_bits(mask)
""",
  )?
  test.ok(! negative_mask.success, negative_mask.stderr)?
  test.contains(negative_mask.stderr, "integer-bitset")?
}
