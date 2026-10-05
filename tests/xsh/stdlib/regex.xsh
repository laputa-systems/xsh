test test_regex_module_and_methods {
  let re = rx"([A-Z]+)-(\d+)"
  assert re.matches("ERR-42")
  assert re.captures("ERR-42")[1] == "ERR"
  assert re.find("ERR-42 OK-7").len() == 2
  assert re.replace("ERR-42", "$1:$2") == "ERR:42"
  test.error_kind(regex.compile("("), "regex-compile")
}
