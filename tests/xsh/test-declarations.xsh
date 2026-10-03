test test_declaration_is_checked_without_execution { |ctx|
  let result = test.run_xsh(ctx, "test registered {\n  false\n}\nrun printf ready\n")?
  assert result.status == 0
  assert result.stdout == "ready"
}

test test_declaration_body_is_checked { |ctx|
  let result = test.run_xsh(ctx, "test checked {\n  missing_name()\n}\n")?
  assert result.status != 0
}

test context_parameter_is_immutable { |ctx|
  let result = test.run_xsh(ctx, "test immutable { |ctx|\n  ctx = ctx\n}\n")?
  assert result.status != 0
  assert "immutable" in result.stderr
}

test context_parameter_has_test_context_type { |ctx|
  let result = test.run_xsh(ctx, "test typed { |ctx|\n  let name: Int = ctx.name\n}\n")?
  assert result.status != 0
  assert "check.type-mismatch" in result.stderr
}

test declarations_cannot_be_called { |ctx|
  let result = test.run_xsh(ctx, "test hidden {}\nhidden()\n")?
  assert result.status != 0
}

test declarations_cannot_be_nested_or_exported { |ctx|
  for source in ["proc outer() { test nested {} }", "export test public {}"] {
    let result = test.run_xsh(ctx, source)?
    assert result.status != 0
  }
}

test declarations_enforce_effects_and_result_unit { |ctx|
  for source in ["test restricted [] { let _ = time.now() }", "test wrong { return Ok(3) }", "test wrong { |a, b| }"] {
    let result = test.run_xsh(ctx, source)?
    assert result.status != 0
  }
}

test main_does_not_execute { |ctx|
  let result = test.run_xsh(ctx, "test main { false }\nrun printf ready\n")?
  assert result.status == 0
  assert result.stdout == "ready"
}

test discard_context_is_allowed { |_| }

test declaration_names_reject_duplicates_and_callable_collisions { |ctx|
  for source in ["test same {}\ntest same {}\n", "pure same() -> Int { 1 }\ntest same {}\n"] {
    let result = test.run_xsh(ctx, source)?
    assert result.status != 0
    assert "check.duplicate-name" in result.stderr
  }
}

test imported_declarations_do_not_execute { |ctx|
  let root = test.temp_dir(ctx, name: "imported-test-declaration")?
  fp"${root}/helper.xsh".write_atomic(r"""##! Helper with a declared test.
## The shared value.
export pure value() -> Int { 7 }
test helper_test {
  print "TEST EXECUTED"
  false
}
""")?
  let result = test.run_xsh(ctx, "use helper\nprint \${helper.value()}\n", env: {XSH_MODULE_PATH: root.display()})?
  assert result.status == 0
  assert result.stdout == "7\n"
}
