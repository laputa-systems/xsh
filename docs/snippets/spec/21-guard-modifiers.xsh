type Entry = {kind: Str, size: Int}

pure first_cached(cached: Str?) -> Str? {
  # begin example
  return cached when cached != null
  # end example
  null
}

pure files(entries: List[Entry]) -> List[Entry] {
  var kept: List[Entry] = []
  for entry in entries {
    # begin example
    continue unless entry.kind == "file"
    # end example
    kept += [entry]
  }

  kept
}

stream nonempty(rows: List[Entry]) -> Stream[Entry] {
  for row in rows {
    # begin example
    yield row when row.size > 0
    # end example
  }
}
