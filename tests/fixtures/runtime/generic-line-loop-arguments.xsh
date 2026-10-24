pure line_identity(value) { value }
proc collected_lines(text: Str, wrong: Bool) [] -> List[Str] {
  let _ = wrong
  var values: List[Str] = []
  for line in text.lines() {
    values += [line_identity(line)]
  }
  values
}
proc verify_line_loop_arguments() -> Result[Unit] {
  assert collected_lines("first\r\n\nlast\n", false) == ["first", "", "last"]
  assert collected_lines("", true) == []
}
