pure scan(text: Bytes) -> Int {
  var blanks = 0
  var comments = 0
  for line in text.lines() {
    let trimmed = line.trim()
    if trimmed == b"" {
      blanks += 1
    } else if trimmed.starts_with(b"#") {
      comments += 1
    }
  }
  return text.count_lines() - blanks - comments
}
test scan_case {
  assert scan(b"# x\nvalue\n\n") == 1
}
