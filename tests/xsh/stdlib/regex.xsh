proc test_regex_module_and_methods() [error] {
  let re = regex.compile("([A-Z]+)-(\\d+)")?
  test.ok(re.matches("ERR-42"))?
  test.eq(re.captures("ERR-42")[1], "ERR")?
  test.eq(re.find("ERR-42 OK-7").len(), 2)?
  test.eq(re.replace("ERR-42", "$1:$2"), "ERR:42")?
  test.error_kind(regex.compile("("), "regex-compile")?
}
