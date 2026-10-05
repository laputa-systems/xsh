pure first_cached(cached: Str?) -> Str {
  # begin example
  return cached when cached != null
  # end example
  "miss"
}

print first_cached("hit")
