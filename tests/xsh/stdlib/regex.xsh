proc test_regex_module_and_methods() [error] {
  let re = regex.compile("([A-Z]+)-(\\d+)")?
  re.matches("ERR-42")
  re.captures("ERR-42")[1] == "ERR"
  re.find("ERR-42 OK-7").len() == 2
  re.replace("ERR-42", "$1:$2") == "ERR:42"
  test.error_kind(regex.compile("("), "regex-compile")?
}
