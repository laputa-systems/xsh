test test_set_module {
  let empty = set.empty()
  assert "alpha" not in empty
  let items = set.from(["alpha", "beta", "alpha"])
  assert "alpha" in items
  assert "beta" in items
  assert items.keys().len() == 2
  let added = set.add(items, "gamma")
  assert "gamma" in added
  let removed = set.remove(added, "alpha")
  assert "alpha" not in removed
}
