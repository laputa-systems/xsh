proc main() [io] {
  var words: List[Str] = []
  var i = 0
  while i < 200 {
    words = words.extend(["plain", "two words", "can't", "a/b_c-1.2", "日本"])
    i = i + 1
  }
  print "${shlex.join(words).byte_len()}"
}
