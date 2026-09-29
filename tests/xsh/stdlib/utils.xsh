pure cached_label(name: Str) -> Str {
  return f"cached ${name}"
}

test test_utils_cache [error] {
  test.eq(utils.cache(cached_label, ["value"]), "cached value")?
  test.eq(utils.cache(cached_label, ["value"]), "cached value")?
}
