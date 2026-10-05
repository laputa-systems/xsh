# begin example
const KINDS = {"file", "dir"}

pure allowed(kind: Str, extra: Set[Str]) -> Bool {
  kind in KINDS | extra
}

proc report(words: List[Str]) [error] {
  # An element that is not a bare name makes braces a set.
  let stop = {"a", "the"}
  let first = "one"
  let named = {first, "two"}

  # Bare names alone are a record unless a set is expected.
  let second = "two"
  let pair: Set[Str] = {first, second}
  assert pair == named

  # A set has no empty or one-element form of its own without a comma.
  var seen: Set[Str] = set.empty()
  let only = {"one",}
  for word in words {
    if word not in stop {
      seen = seen.add(word)
    }
  }

  # Sets iterate in key order, whatever order built them.
  let lengths = {word.byte_len() for word in seen}
  assert (seen & only).to_list() == ["one"]
  assert (seen - only).len() + 1 == seen.len()
  assert words.to_set() - stop == seen
  print (json.encode(seen)?) (json.encode(lengths)?)
  assert allowed("link", {"link",})
}

# end example

report(["the", "one", "three", "one"])
