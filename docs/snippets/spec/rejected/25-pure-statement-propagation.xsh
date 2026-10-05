pure parse(text: Str) -> Result[Unit] {
  assert text != ""
}

pure length(text: Str) -> Int {
  parse(text) # error: check.try-context
  text.byte_len()
}
