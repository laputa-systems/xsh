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
  let empty_counts: Map[Int] = {}
  var counts = empty_counts.set("beta", 2).set("alpha", 1)
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
  let empty_values: Map[MapIterationPayload] = {}
  let values = empty_values
    .set("second", {label: "two", amount: 2})
    .set("first", {label: "one", amount: 1})
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
  let empty_values: Map[Int] = {}
  return empty_values.set("first", 4)
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
  let empty_values: Map[Int] = {}
  let values = empty_values.set("alpha", 1).set("beta", 2).set("gamma", 3)
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
  let empty: Map[Int] = {}
  return empty.set("beta", 2).set("alpha", 1)
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
