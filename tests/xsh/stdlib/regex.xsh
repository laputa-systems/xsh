test test_regex_module_and_methods {
  let re = rx"([A-Z]+)-(\d+)"
  assert re.matches("ERR-42")
  assert re.captures("ERR-42")[1] == "ERR"
  assert re.find("ERR-42 OK-7").len() == 2
  assert re.replace("ERR-42", with: "$1:$2") == "ERR:42"
  test.error_kind(regex.compile("("), "regex-compile")
}

test test_regex_bytes_capture_offsets_and_unmatched_groups {
  let captures = regex.captures_bytes("(a)?(b)", b"x\xffb", offset: 1, extended: true)?
  assert captures.len() == 10
  assert captures[0] == {start: 2, end: 3}
  assert captures[1] == null
  assert captures[2] == {start: 2, end: 3}
  assert captures[9] == null
  assert (regex.captures_bytes("z", b"abc")?).is_empty()
  assert (regex.captures_bytes("^b", b"ab", offset: 1)?).is_empty()
}

test test_regex_bytes_capture_empty_matches_and_errors {
  let empty = regex.captures_bytes("$", b"ab", offset: 2)?
  assert empty[0] == {start: 2, end: 2}
  test.error_kind(regex.captures_bytes("[", b""), "regex-match")
  test.error_kind(regex.captures_bytes("a", b"ab", offset: -1), "regex-match")
  test.error_kind(regex.captures_bytes("a", b"ab", offset: 3), "regex-match")
}
