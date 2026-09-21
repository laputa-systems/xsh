proc main() [io, error] {
  # The quoting edge cases repeated as a bounded batch: empty, embedded quotes,
  # newlines, non-ASCII scalars, and the safe set.
  var words: List[Str] = []
  var index = 0
  while index < 200 {
    words = words.extend(
      ["", "'", "a'b'c", """two
words""", "h\u{e9}llo", "_@%+=:,./-", "a/b_c-1.2", "日本"],
    )
    index = index + 1
  }
  print f"${shlex.join(words).byte_len()}"
}
