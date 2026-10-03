test test_map_module_and_methods [error] {
  let m0: Map[Int] = {}
  let m1 = m0.set("one", 1).set("two", 2)
  ("one" in m1)
  m1.get("two")? == 2
  (m1.get("missing") ?? 99) == 99
  m1.keys()[0] == "one"
  m1.values()[1] == 2
  ("one" not in m1.remove("one"))
  test.error_kind(m1.get("missing"), "map-missing")?
}

test test_map_updates_preserve_older_values_and_nested_lists [error] {
  let base: Map[List[Int]] = {items: [1]}
  let alias = base
  let replaced = base.set("items", [9])
  let pushed = base.push("items", 2)
  let removed = pushed.remove("items")

  (base.get("items")?) == ([1])
  (alias.get("items")?) == ([1])
  (replaced.get("items")?) == ([9])
  pushed.get("items")? == [1, 2]
  ("items" not in removed)
  pushed.get("items")? == [1, 2]

  var mutable = pushed
  mutable["items"] = [3]
  mutable.get("items")? == [3]
  pushed.get("items")? == [1, 2]
}

test test_map_index_updates_group_push_and_comprehension [error] {
  var counts: Map[Int] = {}
  counts["pkg"] = (counts.get("pkg") ?? 0) + 1
  counts["pkg"] = (counts.get("pkg") ?? 0) + 4
  counts.get("pkg")? == 5

  let empty_groups: Map[List[Str]] = {}
  let groups = empty_groups.push("pkg", "one").push("pkg", "two").push("tool", "alpha")
  groups.get("pkg")? == ["one", "two"]
  groups.get("tool")? == ["alpha"]
  ("pkg" not in empty_groups)

  let versions = {row.name: row.version for row in [{name: "pkg", version: "1"}, {name: "tool", version: "2"}]}
  versions.get("tool")? == "2"
}

test test_map_iteration_item_shape_order_and_snapshot [error] {
  var counts: Map[Int] = {beta: 2, alpha: 1}
  var seen = []
  for entry in counts {
    seen += [f"${entry.key}=${entry.value}"]
    counts["alpha"] = 99
    counts = counts.remove("beta").set("gamma", 3)
  }
  (seen) == (["alpha=1", "beta=2"])
  (counts.get("alpha")?) == (99)
  "beta" not in counts
  (counts.get("gamma")?) == (3)

  let empty: Map[Int] = {}
  ([entry.key for entry in empty]) == ([])
}

type MapIterationPayload = {label: Str, amount: Int}

test test_map_iteration_nested_targets_and_qualifiers [error] {
  let values: Map[MapIterationPayload] = {
    second: {label: "two", amount: 2},
    first: {label: "one", amount: 1},
  }
  var selected = []
  for {key, value: {label: name, amount, ..}, ..} in values {
    selected += [f"${key}:${name}:${amount}"]
  }
  (selected) == (["first:one:1", "second:two:2"])
  let expanded = [f"${key}:${item}" for {key, value: payload, ..} in values if payload.amount > 1 for item in [payload.label, payload.label.upper()]]
  (expanded) == (["second:two", "second:TWO"])
  let labels = {key: value.label for {key, value} in values}
  (labels.get("first")?) == ("one")
}

error MapIterationError = Missing(code: Int)

pure map_iteration_failed_source() -> Result[Map[Int], MapIterationError] {
  Err(MapIterationError.Missing(code: 7))
}

proc map_iteration_collect_failure() [error] -> Result[List[Str], MapIterationError] {
  [key for {key, value: _, ..} in map_iteration_failed_source()]
}

proc map_iteration_loop_failure() [error] -> Result[Int, MapIterationError] {
  for entry in map_iteration_failed_source() {
    return entry.value
  }
  0
}

pure map_iteration_success_value() -> Result[Int, MapIterationError] {
  3
}

pure map_iteration_success_source() -> Result[Map[Int], MapIterationError] {
  {first: 4}
}

test test_map_iteration_result_sources_preserve_nominal_errors [error] {
  if let Err(MapIterationError.Missing {code}) = map_iteration_collect_failure() {
    code == 7
  } else {
    test.fail("comprehension lost the source error")?
  }
  if let Err(MapIterationError.Missing {code}) = map_iteration_loop_failure() {
    code == 7
  } else {
    test.fail("loop lost the source error")?
  }
  let empty_values: Map[Result[Int, MapIterationError]] = {}
  let values = empty_values.set("alpha", map_iteration_success_value()).set("beta", Err(MapIterationError.Missing(code: 9)))
  for entry in values {
    match entry.value {
      Ok(value) => (value) == (3)
      Err(MapIterationError.Missing {code}) => (code) == (9)
    }
  }
  let wrapped = map_iteration_success_source()
  ([entry.value for entry in wrapped]) == ([4])
}

test test_map_iteration_break_continue_restore_outer_bindings [error] {
  let values: Map[Int] = {alpha: 1, beta: 2, gamma: 3}
  let outer_key = "outer"
  var sum = 0
  for {key, value} in values {
    continue when key == "alpha"
    sum += value
    break
  }
  (sum) == (2)
  (outer_key) == ("outer")
}

test test_map_iteration_evaluates_source_once_and_unwinds_failure [error] { |ctx|
  let output = test.run_xsh(
    ctx,
    """
error SourceError = Missing(code: Int)
proc load() [io] -> Map[Int] {
  print "source"
  return {beta: 2, alpha: 1}
}
proc close() [io] { print "closed" }
pure failed() -> Result[Map[Int], SourceError] {
  return Err(SourceError.Missing(code: 7))
}
proc gather() [io, error] -> Result[List[Str], SourceError] {
  defer close()
  return [entry.key for entry in failed()]
}
for entry in load() { print f"\${entry.key}=\${entry.value}" }
match gather() {
  Err(SourceError.Missing {code}) => print f"caught=\${code}"
  _ => print "unexpected"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("source\nalpha=1\nbeta=2\nclosed\ncaught=7\n")
}

test test_map_literals_computed_constant_keys_spreads_and_aliases [error] {
  let name = "beta"
  let inferred = {[name]: 2, alpha: 1, "literal.dot": 3}
  (inferred.keys()) == (["alpha", "beta", "literal.dot"])
  let constants: Map[Int] = {alpha: 1, beta: 2}
  var combined = {...constants, [name]: 4, alpha: 5, ["alpha"]: 6}
  let alias = combined
  combined["alpha"] = 99
  (alias.get("alpha")?) == (6)
  (constants.get("alpha")?) == (1)
  (combined.get("beta")?) == (4)
  let spread_only: Map[Int] = {...constants}
  (spread_only) == (constants)
  let nested: Map[List[Str]] = {empty: [], [name]: ["value"]}
  (nested.get("empty")?) == ([])
  let source_row = {alpha: 1, beta: "two"}
  (source_row.beta) == ("two")
  let ordinary = {...source_row}
  (ordinary.alpha) == (1)
  var values = inferred
  var seen = []
  for {key, value: _} in values {
    seen += [key]
    values = values.set("later", 9)
  }
  (seen) == (["alpha", "beta", "literal.dot"])
}

test test_map_literals_evaluate_keys_values_spreads_and_overwrites_once [error] { |ctx|
  let output = test.run_script(ctx, r"""proc key(value: Str) [io] -> Str {
  print f"key $value"
  return value
}
proc value(amount: Int) [io] -> Int {
  print f"value $amount"
  return amount
}
proc spread() [io] -> Map[Int] {
  print "spread"
  return {["same"]: 3}
}
let values = {[key("same")]: value(1), same: value(2), ...spread(), [key("last")]: value(4)}
print (values.get("same") ?? 0) (values.get("last") ?? 0)
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("key same\nvalue 1\nvalue 2\nspread\nkey last\nvalue 4\n3 4\n")
}

test test_map_literals_failure_stops_before_value_and_later_entries [error] { |ctx|
  let output = test.run_script(ctx, r"""error BuildError = Stopped(message: Str)
proc key() [io] -> Result[Str, BuildError] {
  print "key"
  return Err(BuildError.Stopped(message: "stop map"))
}
proc value() [io] -> Int { print "value"; return 1 }
let values = {["first"]: value(), [(key()?)]: value(), later: value()}
print values.len()
""")?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  "stop map" in output.stderr
  (output.stdout) == ("value\nkey\n")
}

test test_map_literals_reject_wrong_key_context_bad_spreads_and_incompatible_values [error] { |ctx|
  for source in [
    "let value: Map[Int] = {[1]: 2}\n",
    "let key: Any = \"name\"\nlet value = {[key]: 2}\n",
    "let value = {[\"name\"]: 2, other: \"wrong\"}\n",
    "let value: Map[Int] = {one: \"wrong\"}\n",
    "let source_row = {one: 1}\nlet value = {[\"name\"]: 2, ...source_row}\n",
    "let dynamic: Any = {one: 1}\nlet value: Map[Int] = {...dynamic}\n",
    "let base = map.empty().set(\"one\", 1)\nlet value = {...base}\n",
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

pure map_literal_return() -> Map[Int] { {answer: 42} }
pure map_literal_tail() -> Map[Int] { {answer: 43} }
pure map_literal_parameter(input: Map[Int]) -> Int { (input.get("answer") ?? 0) }
pure map_literal_nested(input: List[Map[Int]]) -> List[Map[Int]] { input }
type MapLiteralEnvelope = {values: Map[Int]}

test test_map_literals_expected_context_reaches_returns_arguments_and_nested_values [error] {
  (map_literal_return().get("answer")?) == (42)
  (map_literal_tail().get("answer")?) == (43)
  (map_literal_parameter({answer: 44})) == (44)
  let nested = map_literal_nested([{answer: 45}, {answer: 46}])
  (nested[1].get("answer")?) == (46)
  let envelope = MapLiteralEnvelope(values: {answer: 47})
  (envelope.values.get("answer")?) == (47)
  let constructed = MapLiteralEnvelope(values: {answer: 50})
  (constructed.values.get("answer")?) == (50)
  let empty_nested: Map[Map[Int]] = {}
  let set_nested = empty_nested.set("nested", {answer: 51})
  (set_nested.get("nested")?.get("answer")?) == (51)
  var replaced: Map[Int] = {answer: 48}
  replaced = {answer: 49}
  (replaced.get("answer")?) == (49)
}

pure map_literal_default(input: Map[List[Int]] = {empty: [], ["numbers"]: [1, 2]}) -> Map[List[Int]] { input }
pure map_literal_spread_default(input = {["first"]: 1, ...{second: 2}}) -> Map[Int] { input }

test test_map_literal_defaults_keep_context_and_independent_values [error] {
  let first = map_literal_default()
  var second = map_literal_default()
  second["numbers"] = [9]
  (first.get("numbers")?) == ([1, 2])
  (second.get("numbers")?) == ([9])
  (map_literal_spread_default().get("second")?) == (2)
}

type MapLiteralDefaults = {counts: Map[Int] = {fixed: 1, ["other"]: 2}}

test test_map_literal_record_defaults_retain_map_identity_and_aliases [error] {
  let earlier = MapLiteralDefaults()
  var changed = MapLiteralDefaults()
  changed.counts = changed.counts.set("fixed", 9)
  (earlier.counts.get("fixed")?) == (1)
  (changed.counts.get("fixed")?) == (9)
  (changed.counts.keys()) == (["fixed", "other"])
}

test test_map_iteration_binding_is_entry_record_and_result_sources_need_error [error] { |ctx|
  let counts: Map[Int] = {beta: 2, alpha: 1}
  ([entry for entry in counts]) == ([{key: "alpha", value: 1}, {key: "beta", value: 2}])
  ([entry.value for entry in counts]) == (counts.values())
  for source in [
    "proc forbidden(values: Result[Map[Int]]) [] { for entry in values { let _ = entry } }\n",
    "proc forbidden(values: Result[Map[Int]]) [] { let _ = [entry.key for entry in values] }\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    "check.effect-violation" in output.stderr
  }
}
