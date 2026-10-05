test test_set_module {
  let empty: Set[Str] = set.empty()
  assert "alpha" not in empty
  let items: Set[Str] = set.from(["alpha", "beta", "alpha"])
  assert "alpha" in items
  assert "beta" in items
  assert items.len() == 2
  let added = items.add("gamma")
  assert "gamma" in added
  let removed = added.remove("alpha")
  assert "alpha" not in removed
  assert removed.to_list() == ["beta", "gamma"]
}
