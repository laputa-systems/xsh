test test_uint_assignment_rejects_negative_values_after_rhs [error] { |ctx|
  for source in [
    "var value: UInt = 1\nvalue = -1\n",
    "var value: Map[Str, UInt] = {a: 1}\nvalue[\"a\"] = -1\n",
    "type Row = {count: UInt}\nvar value: Row = {count: 1}\nvalue.count = -1\n",
    "var value: List[UInt] = [1]\nvalue[0] = -1\n",
    "var value: UInt = 1\nvalue -= 2\n",
    "var value: Map[Str, UInt] = {a: 1}\nvalue[\"a\"] -= 2\n",
    "type Row = {count: UInt}\nvar value: Row = {count: 1}\nvalue.count -= 2\n",
    "var value: List[UInt] = [1]\nvalue[0] -= 2\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
  }
}

test test_uint_failed_compound_preserves_rhs_effects_and_aliases [error] { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  test.eq(output.success, false)?
  test.ok("UInt" in output.stderr)?
  test.eq(output.stdout, "selector\nrhs\n[{\"count\":1,\"untouched\":9}] [{\"count\":1,\"untouched\":2}]\n")?
}

test test_uint_mutation_checks_whole_replacements_and_list_append [error] { |ctx|
  for source in [
    "var value: List[UInt] = [1]\nvalue = [-1]\n",
    "var value: List[UInt] = [1]\nvalue += [-1]\n",
    "type Row = {count: UInt}\nvar value: Row = {count: 1}\nvalue = {count: -1}\n",
    "var value: Map[Str, UInt] = {a: 1}\nvalue = {a: -1}\n",
    "var value: List[List[UInt]] = [[1]]\nvalue[0] = [-1]\n",
    "var value: Map[Str, List[UInt]] = {a: [1]}\nvalue[\"a\"] += [-1]\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
  }
}

test test_uint_valid_updates_preserve_current_root_and_aliases [error] {
  var value: UInt = 5
  value -= 5
  value += 2
  value -= -3
  test.eq(value, 5)?
  var rows: List[UInt] = [1, 2]
  let alias = rows
  rows[0] += if true {
    rows = [10, 20]
    3
  } else { 0 }
  rows += [0, 255]
  test.eq(rows, [13, 20, 0, 255])?
  test.eq(alias, [1, 2])?
  var entries: Map[Str, UInt] = {a: 1}
  entries["new"] = 0
  entries["a"] *= 2
  test.eq(entries.get("new")?, 0)?
  test.eq(entries.get("a")?, 2)?
}


test test_uint_rejected_same_slot_method_result_keeps_owned_container [error] { |ctx|
  let output = test.run_script(ctx, r"""
var entries: Map[Str, UInt] = {a: 1}
let alias = entries
defer { print (json.encode(entries)?) (json.encode(alias)?) }
entries = entries.set("bad", if true {
  print "rhs"
  -1
} else { 0 })
""")?
  test.eq(output.success, false)?
  test.ok("UInt" in output.stderr)?
  test.eq(output.stdout, "rhs\n{\"a\":1} {\"a\":1}\n")?
}

test test_uint_calls_reject_negative_arguments_returns_and_defaults [error] { |ctx|
  for source in [
    "pure accept(n: UInt) -> Int { return n }\nlet dynamic = -1\nprint (accept(dynamic))\n",
    "pure make(n: Int) -> UInt { return n }\nlet returned = make(-1)\n",
    "pure make() -> UInt { -1 }\nprint (make())\n",
    "pure accept(n: UInt = -1) -> Int { return n }\nprint (accept())\n",
    "pure accept(n: List[UInt] = [-1]) -> Int { return n[0] }\nprint (accept())\n",
    "pure make(n: Int) -> List[UInt] { return [n] }\nlet returned = make(-1)\n",
    "pure make(n: Int) -> Map[Str, UInt] { return {a: n} }\nlet returned = make(-1)\n",
    "type Row = {count: UInt}\npure make(n: Int) -> Row { return {count: n} }\nlet returned = make(-1)\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
  }
}

test test_uint_domain_failure_runs_cleanup_without_try_conversion [error] { |ctx|
  for body in [
    "var value: UInt = 1\nvalue = -1\n",
    "let value = accept(-1)\n",
    "let value = make(-1)\n",
  ] {
    let output = test.run_script(ctx, "pure accept(n: UInt) -> Int { return n }\nproc make(n: Int) [error] -> UInt {\n  defer { print \"callee\" }\n  return n\n}\ndefer { print \"outer\" }\nlet captured = try {\n  defer { print \"inner\" }\n" + body + "}\nprint \"after\"\n")?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
    if "make(-1)" in body {
      test.eq(output.stdout, "callee\ninner\nouter\n")?
    } else {
      test.eq(output.stdout, "inner\nouter\n")?
    }
  }
  let checked: Result[UInt] = try { (-1).require(UInt)? }
  test.error_kind(checked, "schema")?
}

test test_uint_producer_yields_and_delegation_validate_each_reached_item [error] { |ctx|
  for body in ["yield n", "yield @[n]", "yield @raw(n)", "yield @[[n]]"] {
    let item = if body == "yield @[[n]]" { "List[UInt]" } else { "UInt" }
    let source = "stream raw(n: Int) [io] -> Stream[Int] {\n  defer { print \"child\" }\n  yield n\n}\nstream checked(n: Int) [io] -> Stream[" + item + "] {\n  defer { print \"parent\" }\n  " + body + "\n  print \"after yield\"\n}\ndefer { print \"outer\" }\nfor value in checked(-1) { print \"received\" }\n"
    let output = test.run_script(ctx, source)?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
    if body == "yield @raw(n)" {
      test.eq(output.stdout, "child\nparent\nouter\n")?
    } else {
      test.eq(output.stdout, "parent\nouter\n")?
    }
  }
}

test test_uint_producer_cancellation_skips_unreached_invalid_items [error] { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  test.eq(output.success, true)?
  test.eq(output.stdout, "1\nchild\nparent\n")?
}

test test_uint_nominal_constructors_reject_negative_payloads [error] { |ctx|
  for source in [
    "enum Count { Counted(UInt), Empty }\nlet negative = -1\nlet rejected = Counted(negative)\n",
    "enum Count { Counted(List[UInt]), Empty }\nlet negative = -1\nlet rejected = Counted([negative])\n",
    "error CountError = Bad(count: UInt)\nlet negative = -1\nlet rejected = CountError.Bad(count: negative)\n",
    "error CountError = Bad(count: List[UInt])\nlet negative = -1\nlet rejected = CountError.Bad(count: [negative])\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
  }
}


test test_uint_functional_methods_validate_arguments_before_publication [error] { |ctx|
  for source in [
    "let base: Map[Str, UInt] = {a: 1}\nlet rejected = base.set(\"b\", -1)\n",
    "let base: List[UInt] = [1]\nlet rejected = base.push(-1)\n",
    "let base: List[UInt] = [1]\nlet rejected = base.extend([-1])\n",
    "let base: List[UInt] = [1]\nlet rejected = base.get(9) ?? -1\n",
    "let base: Map[Str, UInt] = {a: 1}\nlet rejected = base.get(\"missing\") ?? -1\n",
    "let base: Map[Str, List[UInt]] = {a: [1]}\nlet rejected = base.push(\"a\", -1)\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
  }
}

test test_uint_map_membership_validates_keys_without_changing_list_comparison [error] { |ctx|
  let compared = test.run_script(ctx, "let values: List[UInt] = [1]\nprint (-1 in values)\nlet fields: Map[UInt, Str] = {[1]: \"one\"}\nprint (1 in fields)\n")?
  test.ok(compared.success, compared.stderr)?
  test.eq(compared.stdout, "false\ntrue\n")?
  let rejected = test.run_script(ctx, "let fields: Map[UInt, Str] = {[1]: \"one\"}\ndefer { print \"cleanup\" }\nlet found = -1 in fields\nprint \"after\"\n")?
  test.eq(rejected.success, false)?
  test.ok("type-error" in rejected.stderr)?
  test.ok("UInt" in rejected.stderr)?
  test.eq(rejected.stdout, "cleanup\n")?
}

test test_uint_constructor_guards_preserve_all_operand_effects [error] { |ctx|
  let output = test.run_script(ctx, "enum Count { Counted(UInt, Int), Empty }\ndefer { print \"cleanup\" }\nlet value = Counted({ print \"first\"; -1 }, { print \"second\"; 2 })\nprint \"after\"\n")?
  test.eq(output.success, false)?
  test.ok("UInt" in output.stderr)?
  test.eq(output.stdout, "first\nsecond\ncleanup\n")?
}


test test_uint_methods_preserve_receiver_and_named_operand_order [error] { |ctx|
  let output = test.run_script(ctx, """
proc receiver() [io] -> Map[Str, UInt] { print "receiver"; {a: 1} }
defer { print "cleanup" }
let value = receiver().set(value: { print "value"; -1 }, key: { print "key"; "b" })
print "after"
""")?
  test.eq(output.success, false)?
  test.ok("type-error" in output.stderr)?
  test.eq(output.stdout, "receiver\nvalue\nkey\ncleanup\n")?
  let skipped = test.run_script(ctx, """
let absent: Map[Str, UInt]? = null
let value = absent?.set("a", { print "unexpected"; -1 })
print (value == null)
""")?
  test.ok(skipped.success, skipped.stderr)?
  test.eq(skipped.stdout, "true\n")?
}


test test_uint_inferred_bindings_preserve_merge_domains [error] { |ctx|
  for body in [
    "let rejected = if false { good } else { -1 }",
    "let rejected = match false { true => good, false => -1 }",
    "let absent: UInt? = null\nlet rejected = absent ?? -1",
    "let rejected = [good, -1]",
  ] {
    let output = test.run_script(ctx, "let good: UInt = 1\n" + body + "\nprint \"after\"\n")?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
  }
}


test test_uint_imported_constructor_payloads_keep_declared_domains [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "uint-constructors")?
  fp"${root}/counts.xsh".write_atomic("""##! Checked count payloads.
## A nonnegative count.
export enum Count { Counted(UInt) }
## A nonnegative failure payload.
export error CountError = Bad(count: UInt)
""")?
  for body in ["let rejected = c.Counted(-1)", "let rejected = c.CountError.Bad(count: -1)"] {
    let output = test.run_script(ctx, "use counts as c\n" + body + "\n", [], {XSH_MODULE_PATH: root.display()})?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
  }
}


test test_uint_standard_builtin_results_receive_valid_created_values [error] { |ctx|
  for body in [
    "print (([good, -1] |> min()) ?? 0)",
    "print ([good, -1].get(1) ?? 0)",
    "print (if false { good } else { -1 })",
    "for value in [good, -1] { print (value) }",
    "let absent: UInt? = null\nprint (absent ?? -1)",
  ] {
    let output = test.run_script(ctx, "let good: UInt = 1\n" + body + "\n")?
    test.eq(output.success, false)?
    test.ok("type-error" in output.stderr)?
    test.ok("UInt" in output.stderr)?
  }
}
