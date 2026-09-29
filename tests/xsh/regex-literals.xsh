pure assignment_regex() -> Regex {
  rx"(?i)^\s*[a-z_]+=([0-9]+)$"
}

test test_regex_literals_preserve_raw_patterns_and_existing_operations [error] {
  let assignment = assignment_regex()
  test.ok(assignment.matches("  SIZE=42"))?
  test.eq(assignment.captures("SIZE=42")[1], "42")?
  test.eq(assignment.replace("SIZE=42", "$1"), "42")?
  test.eq(rx"[a-z]+".find("a 1 bc").len(), 2)?
  test.ok(rx"\$\{literal\}".matches(r"${literal}"))?
  test.ok(rx"""(?x)
    ^ (a+) # repeated letters
    (b+) $
""".matches("aabb"))?
  for index in range(10) {
    test.ok(assignment_regex().matches(f"COUNT=$index"))?
  }
}

pure default_regex(pattern = rx"^é+$") -> Regex {
  pattern
}

test test_regex_literal_defaults_and_dynamic_compile_errors [error] {
  test.ok(default_regex().matches("éé"))?
  test.eq(default_regex().find("éé")[0].end, 4)?
  let dynamic_pattern = "[0-9]+"
  let dynamic = regex.compile(dynamic_pattern)?
  test.ok(dynamic.matches("42"))?
  test.error_kind(regex.compile("("), "regex-compile")?
}
