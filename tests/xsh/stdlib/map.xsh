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
