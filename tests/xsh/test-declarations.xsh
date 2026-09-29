test test_declaration_is_checked_without_execution { |ctx|
  let result = test.run_xsh(ctx, "test registered {\n  false\n}\nrun printf ready\n")?
  result.status == 0
  result.stdout == "ready"
}

test test_declaration_body_is_checked { |ctx|
  let result = test.run_xsh(ctx, "test checked {\n  missing_name()\n}\n")?
  result.status != 0
}

test context_parameter_is_immutable { |ctx|
  let result = test.run_xsh(ctx, "test immutable { |ctx|\n  ctx = ctx\n}\n")?
  result.status != 0
  result.stderr.contains("immutable")
}

test context_parameter_has_test_context_type { |ctx|
  let result = test.run_xsh(ctx, "test typed { |ctx|\n  let name: Int = ctx.name\n}\n")?
  result.status != 0
  result.stderr.contains("check.type-mismatch")
}

test declarations_cannot_be_called { |ctx|
  let result = test.run_xsh(ctx, "test hidden {}\nhidden()\n")?
  result.status != 0
}

test declarations_cannot_be_nested_or_exported { |ctx|
  for source in ["proc outer() { test nested {} }", "export test public {}"] {
    let result = test.run_xsh(ctx, source)?
    result.status != 0
  }
}

test declarations_enforce_effects_and_result_unit { |ctx|
  for source in ["test restricted [] { let _ = time.now() }", "test wrong { return Ok(3) }", "test wrong { |a, b| }"] {
    let result = test.run_xsh(ctx, source)?
    result.status != 0
  }
}

test main_does_not_execute { |ctx|
  let result = test.run_xsh(ctx, "test main { false }\nrun printf ready\n")?
  result.status == 0
  result.stdout == "ready"
}

test discard_context_is_allowed { |_| }
