pure first_cached(cached: Str?) -> Str {
  # begin example
  if cached != null {
    return cached
  }

  # end example
  "miss"
}

print first_cached("hit")
