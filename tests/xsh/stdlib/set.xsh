proc test_set_module() [error] {
  let empty: Map[Bool] = set.empty()
  test.ok(! set.has(empty, "alpha"))?
  let items = set.from(["alpha", "beta", "alpha"])
  test.ok(set.has(items, "alpha"))?
  test.ok(set.has(items, "beta"))?
  test.eq(items.keys().len(), 2)?
  let added = set.add(items, "gamma")
  test.ok(added.has("gamma"))?
  let removed = set.remove(added, "alpha")
  test.ok(! removed.has("alpha"))?
}
