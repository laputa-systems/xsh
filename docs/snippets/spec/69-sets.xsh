# begin example
const KINDS = set.from(["file", "dir"])

pure allowed(kind: Str, extra: Set[Str]) -> Bool {
  kind in KINDS | extra
}

proc report(words: List[Str]) [error] {
  let stop = set.from(["a", "the"])
  let first = "one"
  let named = set.from([first, "two"])

  let second = "two"
  let pair: Set[Str] = set.from([first, second])
  assert pair == named

  var seen: Set[Str] = set.empty()
  let only = set.from(["one"])
  for word in words {
    if word not in stop {
      seen = seen.add(word)
    }
  }

  # Sets iterate in key order, whatever order built them.
  let lengths = set.from([word.byte_len() for word in seen])
  assert (seen & only).to_list() == ["one"]
  assert (seen - only).len() + 1 == seen.len()
  assert words.to_set() - stop == seen
  print (json.encode(seen)?) (json.encode(lengths)?)
  assert allowed("link", set.from(["link"]))
}

# end example

report(["the", "one", "three", "one"])
