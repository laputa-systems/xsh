pure assignment_regex() -> Regex {
  rx"(?i)^\s*[a-z_]+=([0-9]+)$"
}

test test_regex_literals_preserve_raw_patterns_and_existing_operations [error] {
  let assignment = assignment_regex()
  assignment.matches("  SIZE=42")
  assignment.captures("SIZE=42")[1] == "42"
  assignment.replace("SIZE=42", "$1") == "42"
  rx"[a-z]+".find("a 1 bc").len() == 2
  rx"\$\{literal\}".matches(r"${literal}")
  rx"""(?x)
    ^ (a+) # repeated letters
    (b+) $
""".matches("aabb")
  for index in range(10) {
    assignment_regex().matches(f"COUNT=$index")
  }
}

pure default_regex(pattern = rx"^é+$") -> Regex {
  pattern
}

test test_regex_literal_defaults_and_dynamic_compile_errors [error] {
  default_regex().matches("éé")
  default_regex().find("éé")[0].end == 4
  let dynamic_pattern = "[0-9]+"
  let dynamic = regex.compile(dynamic_pattern)?
  dynamic.matches("42")
  test.error_kind(regex.compile("("), "regex-compile")?
}

test test_regex_literal_errors_fail_preparation_before_execution [error] { |ctx|
  for source in [
    "print \"must not execute\"\npure unused() -> Regex { rx\"(\" }\n",
    "print \"must not execute\"\nif false { let _ = rx\"\"\"[\"\"\" }\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    output.stdout == ""
    "check.regex-literal" in output.stderr
  }
}
