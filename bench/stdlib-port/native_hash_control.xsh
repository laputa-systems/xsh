proc main() [io, fs, error] {
  # The retained digest path: file hashing must stay native.
  var sink = 0
  var round = 0
  while round < 200 {
    sink = sink + hash.sha256(p"fixtures/unicode.txt")?.hex().byte_len()
    round = round + 1
  }
  print f"${sink}"
}
