pure assignment_regex() -> Regex {
  rx"(?i)^\s*[a-z_]+=([0-9]+)$"
}

test test_regex_literals_preserve_raw_patterns_and_existing_operations {
  let assignment = assignment_regex()
  assert assignment.matches("  SIZE=42")
  assert assignment.captures("SIZE=42")[1] == "42"
  assert assignment.replace("SIZE=42", "$1") == "42"
  assert rx"[a-z]+".find("a 1 bc").len() == 2
  assert rx"\$\{literal\}".matches(r"${literal}")
  assert rx"""(?x)
    ^ (a+) # repeated letters
    (b+) $
""".matches("aabb")
  for index in range(10) {
    assert assignment_regex().matches(f"COUNT={index}")
  }
}

pure default_regex(pattern = rx"^é+$") -> Regex {
  pattern
}

test test_regex_literal_defaults_and_dynamic_compile_errors {
  assert default_regex().matches("éé")
  assert default_regex().find("éé")[0].end == 4
  let dynamic_pattern = "[0-9]+"
  let dynamic = regex.compile(dynamic_pattern)?
  assert dynamic.matches("42")
  test.error_kind(regex.compile("("), "regex-compile")?
}

test test_regex_literal_errors_fail_preparation_before_execution { |ctx|
  for source in [
    """print "must not execute"
pure unused() -> Regex { rx"(" }
""",
    """print "must not execute"
if false { let _ = rx\"""[\""" }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert output.stdout == ""
    assert "check.regex-literal" in output.stderr
  }
}
