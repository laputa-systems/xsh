error TestBaseError = Base

test test_list_push_and_extend_preserve_older_values {
  let base = [1, 2]
  let alias = base
  let pushed = base.push(3)
  let extended = pushed.extend([4, 5])

  assert base == [1, 2]
  assert alias == [1, 2]
  assert pushed == [1, 2, 3]
  assert extended == [1, 2, 3, 4, 5]
}

test test_list_concatenation_and_compound_assignment_preserve_aliases {
  var items = []
  items += []
  items += [1]
  let alias = items
  items += [2, 3]
  assert alias == [1]
  assert items == [1, 2, 3]
  items += items
  assert items == [1, 2, 3, 1, 2, 3]
  assert alias == [1]
  let appended = [4, 5]
  let joined = alias + appended
  assert joined == alias.extend([4, 5])
  assert [] + [6] == [6]
  let singleton = [6]
  let empty_ints = []
  assert singleton + empty_ints == singleton
  let empty: List[Str] = []
  assert empty + empty == []
  var container = {items: [1]}
  let record_alias = container
  container.items += [2]
  assert container.items == [1, 2]
  assert record_alias.items == [1]
  container.items += container.items
  assert container.items == [1, 2, 1, 2]
  assert record_alias.items == [1]
  var table: Map[List[Int]] = {entry: [1]}
  let table_alias = table
  table["entry"] += [2]
  assert table.get("entry")? == [1, 2]
  assert table_alias.get("entry")? == [1]
  table["entry"] += table.get("entry")?
  assert table.get("entry")? == [1, 2, 1, 2]
  assert table_alias.get("entry")? == [1]
}

test test_list_compound_assignment_checks_targets_and_elements { |ctx|
  for source in [
    """var items = [1]
items += 2
""",
    """var items = [1]
items += ["wrong"]
""",
    """let items = [1]
items += [2]
""",
    """var items = [1]
items -= [2]
""",
    """let items = [1] + ["wrong"]
""",
  ] {
    let result = test.run_script(ctx, source)?
    {
      let assertion_condition = ! result.success
      let assertion_message = result.stderr
      assert assertion_condition, assertion_message
    }
    assert "check." in result.stderr
  }
}

test test_list_compound_assignment_evaluates_selectors_before_rhs_once { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """selector
rhs
selector
item
1 2 3
"""
}

test test_list_concatenation_evaluates_operands_once_in_source_order { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """left
right
2
"""
}

test test_list_compound_assignment_retains_target_on_dynamic_rhs_failure { |ctx|
  let result = test.run_script(
    ctx,
    r"""pure wrong() -> Any {
  return 2
}
proc report(values: List[Int]) [io] -> Unit {
  print values.len()
}
proc main() [io, error] {
  var values = [1]
  defer report(values)
  values += wrong().require(List[Int])?
}
""",
  )?
  {
    let assertion_condition = ! result.success
    let assertion_message = result.stderr
    assert assertion_condition, assertion_message
  }
  assert "schema check failed at $: expected List, found Int" in result.stderr
  assert result.stdout == """1
"""
}

test test_collection_number_text_status_and_result_methods {
  let base = ["alpha"]
  let pushed = base.push("beta")
  let extended = pushed.extend(["gamma"])
  assert extended.len() == 3
  assert "gamma" in extended
  assert extended.get(0)? == "alpha"
  assert (extended.get(9) ?? "fallback") == "fallback"
  assert ["a", "b", "c"].join(":") == "a:b:c"
  assert 3.float().format(precision: 1) == "3.0"
  assert 3.2.floor()? == 3
  assert 3.2.ceil()? == 4
  assert 3.5.round()? == 4
  assert "3.14159" as Float == 3.14159
  test.error_kind("not-a-number".parse_float(), "parse-float")
  assert 16.0.sqrt() == 4.0
  assert 2.0.pow(3.0) == 8.0
  assert (-3.5).abs() == 3.5
  assert 0.0.sin() == 0.0
  assert 0.0.cos() == 1.0
  let exp_roundtrip = 2.0.ln().exp()
  assert (exp_roundtrip - 2.0).abs() < 0.00000000000001

  let text = """  alpha beta
beta  """

  assert text.trim() == """alpha beta
beta"""

  assert text.trim().starts_with("alpha")
  assert text.trim().ends_with("beta")
  assert "alpha" in text
  assert text.trim().lines().collect().len() == 2
  assert text.words().len() == 3
  assert "a,b,c".split(",")[1] == "b"
  assert "a  b\tc".fields().join(",") == "a,b,c"
  assert "a:b:c".fields(":").join("|") == "a|b|c"
  assert "banana".replace("na", "NA") == "baNANA"
  assert "abcdef".wrap(3).join("|") == "abc|def"
  assert "abc".translate("ac", "AC") == "AbC"
  assert "Hello.TXT".lower() == "hello.txt"
  assert "Hello.txt".upper() == "HELLO.TXT"
  assert "café".upper() == "CAFÉ"
  assert "a-b-c".delete("-") == "abc"
  assert "boook".squeeze("o") == "bok"
  assert "abc".reverse() == "cba"

  assert """a
b
""".count_lines() == 2

  assert "one two".count_words() == 2
  assert "café".count_chars() == 4
  assert "café".byte_len() == 5
  assert "café".byte_len() == 5
  assert ("café".byte_at(0) ?? -1) == 99
  assert ("café".byte_at(3) ?? -1) == 195
  assert ("café".byte_at(4) ?? -1) == 169
  assert "café".byte_at(9) == null
  assert ("café".byte_at(9) ?? 0) == 0
  assert "café".byte_slice(0, 3) == "caf"
  assert "café".byte_slice(3) == "é"

  assert """alpha
beta""".find("\n") == 5

  assert """alpha
beta""".find("a", 1) == 4

  assert """alpha
beta""".find("z") == null

  assert "42" as Int == 42
  assert "42".parse_int_decimal()? == 42
  assert "42" as UInt == 42
  assert "0" as UInt == 0
  test.error_kind("+42".parse_uint(), "parse-uint")
  test.error_kind("-1".parse_uint(), "parse-uint")
  assert "42".parse_uint_positive()? == 42
  assert " 42 ".parse_uint_positive()? == 42
  test.error_kind("0".parse_uint_positive(), "parse-uint-positive")
  test.error_kind("+42".parse_uint_positive(), "parse-uint-positive")
  test.error_kind("-1".parse_uint_positive(), "parse-uint-positive")
  test.error_kind("0x2a".parse_uint_positive(), "parse-uint-positive")
  test.error_kind("nope".parse_uint_positive(), "parse-uint-positive")
  test.error_kind("0x10".parse_int_decimal(), "parse-int")
  test.error_kind("+5".parse_int_decimal(), "parse-int")
  test.error_kind(" 5 ".parse_int_decimal(), "parse-int")
  test.error_kind("05".parse_int_decimal(), "parse-int")
  test.error_kind("nope".parse_int(), "parse-int")
  assert "hello" + " " + "world" == "hello world"
  let name = "Alice"
  assert "Hello, " + name + "!" == "Hello, Alice!"
  assert "a" + "b" + "c" == "abc"
  let status = run.status false
  assert status.exited()
  assert ! status.signaled()
  assert status.exited_with(1)
  assert status.exit_code()? == 1
  test.error_kind(status.signal_number(), "status-kind")
  let result: Result[Int] = Err(TestBaseError.Base("base message"))
  test.error_kind(result.context("wrapped", "extra"), "TestBaseError.Base")
}

test test_int_bitset_methods { |ctx|
  let mode = 0o754
  assert mode.bit_and(0o070) == 0o050
  assert mode.bit_or(0o002) == 0o756
  assert mode.clear_bits(0o054) == 0o700

  let negative_receiver = test.run_script(
    ctx,
    """let value = -1
value.bit_and(1)
""",
  )?
  {
    let assertion_condition = ! negative_receiver.success
    let assertion_message = negative_receiver.stderr
    assert assertion_condition, assertion_message
  }
  assert "integer-bitset" in negative_receiver.stderr

  let negative_mask = test.run_script(
    ctx,
    """let mask = -1
1.clear_bits(mask)
""",
  )?
  {
    let assertion_condition = ! negative_mask.success
    let assertion_message = negative_mask.stderr
    assert assertion_condition, assertion_message
  }
  assert "integer-bitset" in negative_mask.stderr
}
