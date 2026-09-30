pure cached_label(name: Str) -> Str {
  f"cached ${name}"
}

test test_utils_cache [error] {
  (utils.cache(cached_label, ["value"])) == ("cached value")
  (utils.cache(cached_label, ["value"])) == ("cached value")
}
