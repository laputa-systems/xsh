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
