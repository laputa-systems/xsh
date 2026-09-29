proc test_map_module_and_methods() [error] {
  let m0: Map[Int] = {}
  let m1 = m0.set("one", 1).set("two", 2)
  test.ok(m1.has("one"))?
  test.eq(m1.get("two")?, 2)?
  test.eq(m1.get("missing", 99), 99)?
  test.eq(m1.keys()[0], "one")?
  test.eq(m1.values()[1], 2)?
  test.ok(! m1.remove("one").has("one"))?
  test.error_kind(m1.get("missing"), "map-missing")?
}

proc test_map_updates_preserve_older_values_and_nested_lists() [error] {
  let base = map.empty().set("items", [1])
  let alias = base
  let replaced = base.set("items", [9])
  let pushed = base.push("items", 2)
  let removed = pushed.remove("items")

  test.eq(base.get("items")?, [1])?
  test.eq(alias.get("items")?, [1])?
  test.eq(replaced.get("items")?, [9])?
  test.eq(pushed.get("items")?, [1, 2])?
  test.ok(! removed.has("items"))?
  test.eq(pushed.get("items")?, [1, 2])?

  var mutable = pushed
  mutable["items"] = [3]
  test.eq(mutable.get("items")?, [3])?
  test.eq(pushed.get("items")?, [1, 2])?
}

proc test_map_index_updates_group_push_and_comprehension() [error] {
  var counts: Map[Int] = {}
  counts["pkg"] = counts.get("pkg", 0) + 1
  counts["pkg"] = counts.get("pkg", 0) + 4
  test.eq(counts.get("pkg")?, 5)?

  let empty_groups: Map[List[Str]] = {}
  let groups = empty_groups.push("pkg", "one").push("pkg", "two").push("tool", "alpha")
  test.eq(groups.get("pkg")?, ["one", "two"])?
  test.eq(groups.get("tool")?, ["alpha"])?
  test.ok(! empty_groups.has("pkg"))?

  let versions = {row.name: row.version for row in [{name: "pkg", version: "1"}, {name: "tool", version: "2"}]}
  test.eq(versions.get("tool")?, "2")?
}

proc test_map_iteration_item_shape_order_and_snapshot() [error] {
  var counts: Map[Int] = {beta: 2, alpha: 1}
  var seen: List[Str] = []
  for entry in counts {
    seen += [f"${entry.key}=${entry.value}"]
    counts["alpha"] = 99
    counts = counts.remove("beta").set("gamma", 3)
  }
  test.eq(seen, ["alpha=1", "beta=2"])?
  test.eq(counts.get("alpha")?, 99)?
  test.ok(! counts.has("beta"))?
  test.eq(counts.get("gamma")?, 3)?

  let empty: Map[Int] = {}
  test.eq([entry.key for entry in empty], [])?
}

type MapIterationPayload = {label: Str, amount: Int}

proc test_map_iteration_nested_targets_and_qualifiers() [error] {
  let values: Map[MapIterationPayload] = {
    second: {label: "two", amount: 2},
    first: {label: "one", amount: 1},
  }
  var selected: List[Str] = []
  for {key, value: {label: name, amount, ..}, ..} in values {
    selected += [f"${key}:${name}:${amount}"]
  }
  test.eq(selected, ["first:one:1", "second:two:2"])?
  let expanded = [f"${key}:${item}" for {key, value: payload, ..} in values if payload.amount > 1 for item in [payload.label, payload.label.upper()]]
  test.eq(expanded, ["second:two", "second:TWO"])?
  let labels = {key: value.label for {key, value} in values}
  test.eq(labels.get("first")?, "one")?
}

error MapIterationError = Missing(code: Int)

pure map_iteration_failed_source() -> Result[Map[Int], MapIterationError] {
  return Err(MapIterationError.Missing(code: 7))
}

proc map_iteration_collect_failure() [error] -> Result[List[Str], MapIterationError] {
  return [key for {key, value: _, ..} in map_iteration_failed_source()]
}

proc map_iteration_loop_failure() [error] -> Result[Int, MapIterationError] {
  for entry in map_iteration_failed_source() {
    return entry.value
  }
  return 0
}

pure map_iteration_success_value() -> Result[Int, MapIterationError] {
  return 3
}

pure map_iteration_success_source() -> Result[Map[Int], MapIterationError] {
  return {first: 4}
}

proc test_map_iteration_result_sources_preserve_nominal_errors() [error] {
  match map_iteration_collect_failure() {
    Err(MapIterationError.Missing {code}) => test.eq(code, 7)?
    _ => test.fail("comprehension lost the source error")?
  }
  match map_iteration_loop_failure() {
    Err(MapIterationError.Missing {code}) => test.eq(code, 7)?
    _ => test.fail("loop lost the source error")?
  }
  let empty_values: Map[Result[Int, MapIterationError]] = {}
  let values = empty_values.set("alpha", map_iteration_success_value()).set("beta", Err(MapIterationError.Missing(code: 9)))
  for entry in values {
    match entry.value {
      Ok(value) => test.eq(value, 3)?
      Err(MapIterationError.Missing {code}) => test.eq(code, 9)?
    }
  }
  let wrapped = map_iteration_success_source()
  test.eq([entry.value for entry in wrapped], [4])?
}

proc test_map_iteration_break_continue_restore_outer_bindings() [error] {
  let values: Map[Int] = {alpha: 1, beta: 2, gamma: 3}
  let key = "outer"
  var sum = 0
  for {key, value} in values {
    continue when key == "alpha"
    sum += value
    break
  }
  test.eq(sum, 2)?
  test.eq(key, "outer")?
}

proc test_map_iteration_evaluates_source_once_and_unwinds_failure(ctx: TestContext) [error] {
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "source\nalpha=1\nbeta=2\nclosed\ncaught=7\n")?
}

proc test_map_literals_computed_constant_keys_spreads_and_aliases() [error] {
  let name = "beta"
  let inferred = {[name]: 2, alpha: 1, "literal.dot": 3}
  test.eq(inferred.keys(), ["alpha", "beta", "literal.dot"])?
  let constants: Map[Int] = {alpha: 1, "beta": 2}
  var combined = {...constants, [name]: 4, alpha: 5, ["alpha"]: 6}
  let alias = combined
  combined["alpha"] = 99
  test.eq(alias.get("alpha")?, 6)?
  test.eq(constants.get("alpha")?, 1)?
  test.eq(combined.get("beta")?, 4)?
  let spread_only: Map[Int] = {...constants}
  test.eq(spread_only, constants)?
  let nested: Map[List[Str]] = {empty: [], [name]: ["value"]}
  test.eq(nested.get("empty")?, [])?
  let source_row = {alpha: 1, beta: "two"}
  test.eq(source_row.beta, "two")?
  let ordinary = {...source_row}
  test.eq(ordinary.alpha, 1)?
  var values = inferred
  var seen: List[Str] = []
  for {key, value} in values {
    seen += [key]
    values = values.set("later", 9)
  }
  test.eq(seen, ["alpha", "beta", "literal.dot"])?
}

proc test_map_literals_evaluate_keys_values_spreads_and_overwrites_once(ctx: TestContext) [error] {
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
print values.get("same", 0) values.get("last", 0)
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "key same\nvalue 1\nvalue 2\nspread\nkey last\nvalue 4\n3 4\n")?
}

proc test_map_literals_failure_stops_before_value_and_later_entries(ctx: TestContext) [error] {
  let output = test.run_script(ctx, r"""error BuildError = Stopped(message: Str)
proc key() [io] -> Result[Str, BuildError] {
  print "key"
  return Err(BuildError.Stopped(message: "stop map"))
}
proc value() [io] -> Int { print "value"; return 1 }
let values = {["first"]: value(), [(key()?)]: value(), later: value()}
print values.len()
""")?
  test.ok(! output.success, output.stderr)?
  test.contains(output.stderr, "stop map")?
  test.eq(output.stdout, "value\nkey\n")?
}

proc test_map_literals_reject_non_string_keys_bad_spreads_and_incompatible_values(ctx: TestContext) [error] {
  for source in [
    "let value = {[1]: 2}\n",
    "let key: Any = \"name\"\nlet value = {[key]: 2}\n",
    "let value = {[\"name\"]: 2, other: \"wrong\"}\n",
    "let value: Map[Int] = {one: \"wrong\"}\n",
    "let source_row = {one: 1}\nlet value = {[\"name\"]: 2, ...source_row}\n",
    "let dynamic: Any = {one: 1}\nlet value: Map[Int] = {...dynamic}\n",
    "let base = map.empty().set(\"one\", 1)\nlet value = {...base}\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.contains(result.stderr, "check.")?
  }
}

pure map_literal_return() -> Map[Int] { return {answer: 42} }
pure map_literal_tail() -> Map[Int] { {answer: 43} }
pure map_literal_parameter(input: Map[Int]) -> Int { return input.get("answer", 0) }
type MapLiteralEnvelope = {values: Map[Int]}

proc test_map_literals_expected_context_reaches_returns_arguments_and_nested_values() [error] {
  test.eq(map_literal_return().get("answer")?, 42)?
  test.eq(map_literal_tail().get("answer")?, 43)?
  test.eq(map_literal_parameter({answer: 44}), 44)?
  let nested: List[Map[Int]] = [{answer: 45}, {answer: 46}]
  test.eq(nested[1].get("answer")?, 46)?
  let envelope: MapLiteralEnvelope = {values: {answer: 47}}
  test.eq(envelope.values.get("answer")?, 47)?
  let constructed = MapLiteralEnvelope(values: {answer: 50})
  test.eq(constructed.values.get("answer")?, 50)?
  let empty_nested: Map[Map[Int]] = {}
  let set_nested = empty_nested.set("nested", {answer: 51})
  test.eq(set_nested.get("nested")?.get("answer")?, 51)?
  var replaced: Map[Int] = {answer: 48}
  replaced = {answer: 49}
  test.eq(replaced.get("answer")?, 49)?
}

pure map_literal_default(input: Map[List[Int]] = {empty: [], ["numbers"]: [1, 2]}) -> Map[List[Int]] { return input }
pure map_literal_spread_default(input: Map[Int] = {["first"]: 1, ...{second: 2}}) -> Map[Int] { return input }

proc test_map_literal_defaults_keep_context_and_independent_values() [error] {
  let first = map_literal_default()
  var second = map_literal_default()
  second["numbers"] = [9]
  test.eq(first.get("numbers")?, [1, 2])?
  test.eq(second.get("numbers")?, [9])?
  test.eq(map_literal_spread_default().get("second")?, 2)?
}

type MapLiteralDefaults = {counts: Map[Int] = {fixed: 1, ["other"]: 2}}

proc test_map_literal_record_defaults_retain_map_identity_and_aliases() [error] {
  let earlier = MapLiteralDefaults()
  var changed = MapLiteralDefaults()
  changed.counts = changed.counts.set("fixed", 9)
  test.eq(earlier.counts.get("fixed")?, 1)?
  test.eq(changed.counts.get("fixed")?, 9)?
  test.eq(changed.counts.keys(), ["fixed", "other"])?
}
