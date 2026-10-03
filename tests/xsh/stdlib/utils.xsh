pure cached_label(name: Str) -> Str {
  f"cached ${name}"
}

test test_utils_cache {
  assert (utils.cache(cached_label, ["value"])) == ("cached value")
  assert (utils.cache(cached_label, ["value"])) == ("cached value")
}
