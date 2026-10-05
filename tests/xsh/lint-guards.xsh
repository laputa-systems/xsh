type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file`, applies the fixes of `rule` alone, and returns
# the text the file then holds.
proc fixed_by(file: Path, rule: Str, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let fixed = lint(file, ["--fix", "--only", rule])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1) or fixed.status.exited_with(2), fixed.stderr
  file.read_text()
}

# Requires `file` to check without a diagnostic.
proc assert_checks(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

test test_boolean_guard_fix_keeps_failure_body_comments_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "boolean-guard")?
  let file = fp"{root}/guard.xsh"
  let source = "proc validate(jobs: Int) [error] {\n  if jobs <= 0 {\n    # Preserve domain error identity.\n    return error.fail(\"jobs must be positive\")\n  }\n\n  let _ = jobs\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.boolean-guard"])?
  assert "warn[lint.boolean-guard]" in reported.stderr, reported.stderr

  let fixed = fixed_by(file, "lint.boolean-guard", source)?
  assert fixed == source.replace("if jobs <= 0 {", with: "guard jobs > 0 else {")
  assert "# Preserve domain error identity." in fixed, fixed
  assert_checks(file)
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
  let again = lint(file, [])?
  assert "lint.boolean-guard" not in again.stderr, again.stderr
}

# `value <= 0.0` is false for NaN and so is `value > 0.0`: only the negation
# keeps a NaN on the failure branch.
test test_boolean_guard_float_fix_retains_nan_negation { |ctx|
  let root = test.temp_dir(ctx, name: "boolean-guard-float")?
  let file = fp"{root}/guard.xsh"
  let source = "pure positive(value: Float) -> Bool {\n  if value <= 0.0 { return false }\n  true\n}\n"
  file.write(source)
  assert_checks(file)
  let fixed = fixed_by(file, "lint.boolean-guard", source)?
  assert fixed == source.replace("if value <= 0.0 {", with: "guard ! (value <= 0.0) else {")
  assert_checks(file)
}

test test_boolean_guard_fix_refuses_fallthrough_and_binding_forms { |ctx|
  let root = test.temp_dir(ctx, name: "boolean-guard-refused")?
  let file = fp"{root}/guard.xsh"
  for source in [
    "proc validate(ok: Bool) [] { if ! ok { print \"fallthrough\" } }\n",
    "proc validate(ok: Bool) [] { if ! ok { return } else { return } }\n",
    "proc validate(ok: Bool) [] { let _ = ok; if ! ok { return } }\n",
    "proc validate(outcome: Result[Int]) [] { if let Err(failure) = outcome { return } }\n",
  ] {
    file.write(source)
    let reported = lint(file, ["--only", "lint.boolean-guard"])?
    assert "err[" not in reported.stderr, f"{source}{reported.stderr}"
    assert "lint.boolean-guard" not in reported.stderr, f"{source}{reported.stderr}"
    assert fixed_by(file, "lint.boolean-guard", source)? == source, source
  }
}
