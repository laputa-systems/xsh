proc test_set_module() [error] {
  let empty = set.empty()
  ("alpha" not in empty)
  let items = set.from(["alpha", "beta", "alpha"])
  ("alpha" in items)
  ("beta" in items)
  items.keys().len() == 2
  let added = set.add(items, "gamma")
  ("gamma" in added)
  let removed = set.remove(added, "alpha")
  ("alpha" not in removed)
}
