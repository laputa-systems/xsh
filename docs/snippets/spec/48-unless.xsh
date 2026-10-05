type Entry = {kind: Str, size: Int}

pure files(entries: List[Entry]) -> Int {
  var count = 0
  for entry in entries {
    # begin example
    continue unless entry.kind == "file"
    # end example
    count += 1
  }

  count
}

print files([Entry(kind: "file", size: 1), Entry(kind: "dir", size: 0)])
