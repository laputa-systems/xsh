pure uint_expected_rows(rows: List[UInt]) -> List[UInt] {
  rows
}

test test_uint_assignment_rejects_negative_values_after_rhs { |ctx|
  for source in [
    """var value: UInt = 1
value = -1
""",
    """var value: Map[Str, UInt] = {a: 1}
value["a"] = -1
""",
    """type Row = {count: UInt}
var value: Row = {count: 1}
value.count = -1
""",
    """var value: List[UInt] = [1]
value[0] = -1
""",
    """var value: UInt = 1
value -= 2
""",
    """var value: Map[Str, UInt] = {a: 1}
value["a"] -= 2
""",
    """type Row = {count: UInt}
var value: Row = {count: 1}
value.count -= 2
""",
    """var value: List[UInt] = [1]
value[0] -= 2
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
  }
}

test test_uint_failed_compound_preserves_rhs_effects_and_aliases { |ctx|
  let output = test.run_script(
    ctx,
    r"""
type Row = {count: UInt, untouched: Int}
var rows: List[Row] = [{count: 1, untouched: 2}]
let alias = rows
defer { print (json.encode(rows)?) (json.encode(alias)?) }
rows[if true {
  print "selector"
  0
} else { 1 }].count -= if true {
  rows[0].untouched = 9
  print "rhs"
  2
} else { 0 }
""",
  )?
  assert output.success == false
  assert "UInt" in output.stderr
  assert output.stdout == """selector
rhs
[{"count":1,"untouched":9}] [{"count":1,"untouched":2}]
"""
}

test test_uint_mutation_checks_whole_replacements_and_list_append { |ctx|
  for source in [
    """var value: List[UInt] = [1]
value = [-1]
""",
    """var value: List[UInt] = [1]
value += [-1]
""",
    """type Row = {count: UInt}
var value: Row = {count: 1}
value = {count: -1}
""",
    """var value: Map[Str, UInt] = {a: 1}
value = {a: -1}
""",
    """var value: List[List[UInt]] = [[1]]
value[0] = [-1]
""",
    """var value: Map[Str, List[UInt]] = {a: [1]}
value["a"] += [-1]
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
  }
}

test test_uint_valid_updates_preserve_current_root_and_aliases {
  var value: UInt = 5
  value -= 5
  value += 2
  value -= -3
  assert value == 5
  var rows: List[UInt] = [1, 2]
  let alias = rows
  rows[0] += if true {
    rows = [10, 20]
    3
  } else {
    0
  }
  rows += [0, 255]
  let expected_rows = uint_expected_rows([13, 20, 0, 255])
  let expected_alias = uint_expected_rows([1, 2])
  assert rows == expected_rows
  assert alias == expected_alias
  var entries: Map[Str, UInt] = {a: 1}
  entries["new"] = 0
  entries["a"] *= 2
  assert entries.get("new")? == 0
  assert entries.get("a")? == 2
}

test test_uint_rejected_same_slot_method_result_keeps_owned_container { |ctx|
  let output = test.run_script(
    ctx,
    r"""
var entries: Map[Str, UInt] = {a: 1}
let alias = entries
defer { print (json.encode(entries)?) (json.encode(alias)?) }
entries = entries.set("bad", if true {
  print "rhs"
  -1
} else { 0 })
""",
  )?
  assert output.success == false
  assert "UInt" in output.stderr
  assert output.stdout == """rhs
{"a":1} {"a":1}
"""
}

test test_uint_calls_reject_negative_arguments_returns_and_defaults { |ctx|
  for source in [
    """pure accept(n: UInt) -> Int { return n }
let dynamic = -1
print (accept(dynamic))
""",
    """pure make(n: Int) -> UInt { return n }
let returned = make(-1)
""",
    """pure make() -> UInt { -1 }
print (make())
""",
    """pure accept(n: UInt = -1) -> Int { return n }
print (accept())
""",
    """pure accept(n: List[UInt] = [-1]) -> Int { return n[0] }
print (accept())
""",
    """pure make(n: Int) -> List[UInt] { return [n] }
let returned = make(-1)
""",
    """pure make(n: Int) -> Map[Str, UInt] { return {a: n} }
let returned = make(-1)
""",
    """type Row = {count: UInt}
pure make(n: Int) -> Row { return {count: n} }
let returned = make(-1)
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
  }
}

test test_uint_domain_failure_runs_cleanup_without_try_conversion { |ctx|
  for body in [
    """var value: UInt = 1
value = -1
""",
    """let value = accept(-1)
""",
    """let value = make(-1)
""",
  ] {
    let output = test.run_script(
      ctx,
      """pure accept(n: UInt) -> Int { return n }
proc make(n: Int) [error] -> UInt {
  defer { print "callee" }
  return n
}
defer { print "outer" }
let captured = try {
  defer { print "inner" }
""" + body + """}
print "after"
""",
    )?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
    if "make(-1)" in body {
      assert output.stdout == """callee
inner
outer
"""
    } else {
      assert output.stdout == """inner
outer
"""
    }
  }

  let checked = test.run_script(
    ctx,
    r"""let checked: Result[UInt] = try { (-1).require(UInt)? }
test.error_kind(checked, "schema")?
""",
  )?
  let {success: checked_success, stderr: checked_message, ..} = checked
  assert checked_success, checked_message
}

test test_uint_producer_yields_and_delegation_validate_each_reached_item { |ctx|
  for body in ["yield n", "yield @[n]", "yield @raw(n)", "yield @[[n]]"] {
    let item = if body == "yield @[[n]]" { "List[UInt]" } else { "UInt" }
    let source = """stream raw(n: Int) [io] -> Stream[Int] {
  defer { print "child" }
  yield n
}
stream checked(n: Int) [io] -> Stream[""" + item + """] {
  defer { print "parent" }
  """ + body + """\n  print "after yield"
}
defer { print "outer" }
for value in checked(-1) { print "received" }
"""
    let output = test.run_script(ctx, source)?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
    if body == "yield @raw(n)" {
      assert output.stdout == """child
parent
outer
"""
    } else {
      assert output.stdout == """parent
outer
"""
    }
  }
}

test test_uint_producer_cancellation_skips_unreached_invalid_items { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream raw() [io] -> Stream[Int] {
  defer { print "child" }
  yield 1
  print "invalid reached"
  yield -1
}
stream checked() [io] -> Stream[UInt] {
  defer { print "parent" }
  yield @raw()
  print "after delegation"
}
for value in checked() {
  print (value)
  break
}
""",
  )?
  assert output.success == true
  assert output.stdout == """1
child
parent
"""
}

test test_uint_nominal_constructors_reject_negative_payloads { |ctx|
  for source in [
    """enum Count { Counted(UInt), Empty }
let negative = -1
let rejected = Counted(negative)
""",
    """enum Count { Counted(List[UInt]), Empty }
let negative = -1
let rejected = Counted([negative])
""",
    """error CountError = Bad(count: UInt)
let negative = -1
let rejected = CountError.Bad(count: negative)
""",
    """error CountError = Bad(count: List[UInt])
let negative = -1
let rejected = CountError.Bad(count: [negative])
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
  }
}

test test_uint_functional_methods_validate_arguments_before_publication { |ctx|
  for source in [
    """let base: Map[Str, UInt] = {a: 1}
let rejected = base.set("b", -1)
""",
    """let base: List[UInt] = [1]
let rejected = base.push(-1)
""",
    """let base: List[UInt] = [1]
let rejected = base.extend([-1])
""",
    """let base: List[UInt] = [1]
let rejected = base.get(9) ?? -1
""",
    """let base: Map[Str, UInt] = {a: 1}
let rejected = base.get("missing") ?? -1
""",
    """let base: Map[Str, List[UInt]] = {a: [1]}
let rejected = base.push("a", -1)
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
  }
}

test test_uint_map_membership_validates_keys_without_changing_list_comparison { |ctx|
  let compared = test.run_script(
    ctx,
    """let values: List[UInt] = [1]
print (-1 in values)
let fields: Map[UInt, Str] = {[1]: "one"}
print (1 in fields)
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = compared
    assert assertion_condition, assertion_message
  }
  assert compared.stdout == """false
true
"""
  let rejected = test.run_script(
    ctx,
    """let fields: Map[UInt, Str] = {[1]: "one"}
defer { print "cleanup" }
let found = -1 in fields
print "after"
""",
  )?
  assert rejected.success == false
  {
    let assertion_condition = "schema check failed: expected UInt, found Int" in rejected.stderr
    let assertion_message = rejected.stderr
    assert assertion_condition, assertion_message
  }
  assert rejected.stdout == """cleanup
"""
}

test test_uint_constructor_guards_preserve_all_operand_effects { |ctx|
  let output = test.run_script(
    ctx,
    """enum Count { Counted(UInt, Int), Empty }
defer { print "cleanup" }
let value = Counted({ print "first"; -1 }, { print "second"; 2 })
print "after"
""",
  )?
  assert output.success == false
  assert "UInt" in output.stderr
  assert output.stdout == """first
second
cleanup
"""
}

test test_uint_methods_preserve_receiver_and_named_operand_order { |ctx|
  let output = test.run_script(
    ctx,
    """
proc receiver() [io] -> Map[Str, UInt] { print "receiver"; {a: 1} }
defer { print "cleanup" }
let value = receiver().set(value: { print "value"; -1 }, key: { print "key"; "b" })
print "after"
""",
  )?
  assert output.success == false
  assert "type-error" in output.stderr
  assert output.stdout == """receiver
value
key
cleanup
"""
  let skipped = test.run_script(
    ctx,
    """
let absent: Map[Str, UInt]? = null
let value = absent?.set("a", { print "unexpected"; -1 })
print (value == null)
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = skipped
    assert assertion_condition, assertion_message
  }
  assert skipped.stdout == """true
"""
}

test test_uint_inferred_bindings_preserve_merge_domains { |ctx|
  for body in [
    "let rejected = if false { good } else { -1 }",
    "let rejected = match false { true => good, false => -1 }",
    """let absent: UInt? = null
let rejected = absent ?? -1""",
    "let rejected = [good, -1]",
  ] {
    let output = test.run_script(
      ctx,
      """let good: UInt = 1
""" + body + """\nprint "after"
""",
    )?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
  }
}

test test_uint_imported_constructor_payloads_keep_declared_domains { |ctx|
  let root = test.temp_dir(ctx, name: "uint-constructors")?
  fp"{root}/counts.xsh".write_atomic("""##! Checked count payloads.
## A nonnegative count.
export enum Count { Counted(UInt) }
## A nonnegative failure payload.
export error CountError = Bad(count: UInt)
""")
  for body in ["let rejected = c.Counted(-1)", "let rejected = c.CountError.Bad(count: -1)"] {
    let output = test.run_script(
      ctx,
      """use counts as c
""" + body + "\n",
      [],
      {XSH_MODULE_PATH: root},
    )?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
  }
}

test test_uint_standard_builtin_results_receive_valid_created_values { |ctx|
  for body in [
    "print (([good, -1] |> min()) ?? 0)",
    "print ([good, -1].get(1) ?? 0)",
    "print (if false { good } else { -1 })",
    "for value in [good, -1] { print (value) }",
    """let absent: UInt? = null
print (absent ?? -1)""",
  ] {
    let output = test.run_script(
      ctx,
      """let good: UInt = 1
""" + body + "\n",
    )?
    assert output.success == false
    assert "type-error" in output.stderr
    assert "UInt" in output.stderr
  }
}
