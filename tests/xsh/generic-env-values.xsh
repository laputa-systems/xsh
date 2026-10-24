test generic_environment_values { |ctx|
  let source = p"tests/fixtures/runtime/generic-env-values.xsh".read_text()?
  test.expect(ctx, source + "\nverify_generic_env_values()?\n", status: 0)?
}

test generic_environment_values_reject_unconverted_domains { |ctx|
  let source = p"tests/fixtures/runtime/generic-env-values.xsh".read_text()?
  for body in [
    "let _ = direct_env_value(null)",
    "let value: Any = \"opaque\"; let _ = direct_env_value(value)",
    "let _ = direct_env_value([1, 2])",
    "let _ = direct_env_value([\"one\", \"two\"])",
    "let value: Result[Str, Str] = Err(\"data\"); let _ = direct_env_value(value)",
  ] {
    let output = test.run_xsh(ctx, source + "\n" + body + "\n")?
    assert output.status == 2
    assert "check.local-inference" not in output.stderr
  }
}
