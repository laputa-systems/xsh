error TestBaseError = Base(message: Str)

test test_list_push_and_extend_preserve_older_values [error] {
  let base = [1, 2]
  let alias = base
  let pushed = base.push(3)
  let extended = pushed.extend([4, 5])

  base == [1, 2]
  alias == [1, 2]
  pushed == [1, 2, 3]
  extended == [1, 2, 3, 4, 5]
}

test test_list_concatenation_and_compound_assignment_preserve_aliases [error] {
  var items = []
  items += []
  items += [1]
  let alias = items
  items += [2, 3]
  (alias) == ([1])
  (items) == ([1, 2, 3])
  items += items
  (items) == ([1, 2, 3, 1, 2, 3])
  (alias) == ([1])
  let appended = [4, 5]
  let joined = alias + appended
  (joined) == (alias.extend([4, 5]))
  ([] + [6]) == ([6])
  let singleton = [6]
  let empty_ints = []
  (singleton + empty_ints) == (singleton)
  let empty: List[Str] = []
  (empty + empty) == ([])
  var container = {items: [1]}
  let record_alias = container
  container.items += [2]
  (container.items) == ([1, 2])
  (record_alias.items) == ([1])
  container.items += container.items
  (container.items) == ([1, 2, 1, 2])
  (record_alias.items) == ([1])
  var table: Map[List[Int]] = {entry: [1]}
  let table_alias = table
  table["entry"] += [2]
  (table.get("entry")?) == ([1, 2])
  (table_alias.get("entry")?) == ([1])
  table["entry"] += table.get("entry")?
  (table.get("entry")?) == ([1, 2, 1, 2])
  (table_alias.get("entry")?) == ([1])
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
    {
      let assertion_condition = ! result.success
      let assertion_message = result.stderr
      assert assertion_condition, assertion_message
    }
    "check." in result.stderr
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  (result.stdout) == ("selector\nrhs\nselector\nitem\n1 2 3\n")
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  (result.stdout) == ("left\nright\n2\n")
}

test test_list_compound_assignment_retains_target_on_dynamic_rhs_failure [error] { |ctx|
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
  "schema check failed at $: expected List, found Int" in result.stderr
  (result.stdout) == ("1\n")
}

test test_collection_number_text_status_and_result_methods [process, error] {
  let base = ["alpha"]
  let pushed = base.push("beta")
  let extended = pushed.extend(["gamma"])
  extended.len() == 3
  ("gamma" in extended)
  extended.get(0)? == "alpha"
  (extended.get(9) ?? "fallback") == "fallback"
  ["a", "b", "c"].join(":") == "a:b:c"
  3.float().format(precision: 1) == "3.0"
  3.2.floor()? == 3
  3.2.ceil()? == 4
  3.5.round()? == 4
  "3.14159".parse_float()? == 3.14159
  test.error_kind("not-a-number".parse_float(), "parse-float")?
  16.0.sqrt() == 4.0
  2.0.pow(3.0) == 8.0
  (-3.5).abs() == 3.5
  0.0.sin() == 0.0
  0.0.cos() == 1.0
  let exp_roundtrip = 2.0.ln().exp()
  (exp_roundtrip - 2.0).abs() < 0.00000000000001

  let text = """  alpha beta
beta  """

  text.trim() == """alpha beta
beta"""

  text.trim().starts_with("alpha")
  text.trim().ends_with("beta")
  ("alpha" in text)
  (text.trim().lines().collect().len()) == (2)
  text.words().len() == 3
  "a,b,c".split(",")[1] == "b"
  "a  b\tc".fields().join(",") == "a,b,c"
  "a:b:c".fields(":").join("|") == "a|b|c"
  "banana".replace("na", "NA") == "baNANA"
  "abcdef".wrap(3).join("|") == "abc|def"
  "abc".translate("ac", "AC") == "AbC"
  "Hello.TXT".lower() == "hello.txt"
  "Hello.txt".upper() == "HELLO.TXT"
  "caf\u{e9}".upper() == "CAF\u{c9}"
  "a-b-c".delete("-") == "abc"
  "boook".squeeze("o") == "bok"
  "abc".reverse() == "cba"

  """a
b
""".count_lines() == 2

  "one two".count_words() == 2
  "caf\u{e9}".count_chars() == 4
  "caf\u{e9}".byte_len() == 5
  "caf\u{e9}".byte_len() == 5
  ("caf\u{e9}".byte_at(0) ?? -1) == 99
  ("caf\u{e9}".byte_at(3) ?? -1) == 195
  ("caf\u{e9}".byte_at(4) ?? -1) == 169
  "caf\u{e9}".byte_at(9) == null
  ("caf\u{e9}".byte_at(9) ?? 0) == 0
  "caf\u{e9}".byte_slice(0, 3) == "caf"
  "caf\u{e9}".byte_slice(3) == "\u{e9}"

  """alpha
beta""".find("\n") == 5

  """alpha
beta""".find("a", 1) == 4

  """alpha
beta""".find("z") == null

  "42".parse_int()? == 42
  "42".parse_int_decimal()? == 42
  "42".parse_uint()? == 42
  "0".parse_uint()? == 0
  test.error_kind("+42".parse_uint(), "parse-uint")?
  test.error_kind("-1".parse_uint(), "parse-uint")?
  "42".parse_uint_positive()? == 42
  " 42 ".parse_uint_positive()? == 42
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
  ("hello" + " " + "world") == "hello world"
  let name = "Alice"
  ("Hello, " + name + "!") == "Hello, Alice!"
  ("a" + "b" + "c") == "abc"
  let status = run.status false
  status.exited()
  ! status.signaled()
  status.exited_with(1)
  status.exit_code()? == 1
  test.error_kind(status.signal_number(), "status-kind")?
  let result: Result[Int] = Err(TestBaseError.Base(message: "base message"))
  test.error_kind(result.context("wrapped", "extra"), "TestBaseError.Base")?
}

test test_int_bitset_methods [fs, error] { |ctx|
  let mode = 0o754
  mode.bit_and(0o070) == 0o050
  mode.bit_or(0o002) == 0o756
  mode.clear_bits(0o054) == 0o700

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
  "integer-bitset" in negative_receiver.stderr

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
  "integer-bitset" in negative_mask.stderr
}
