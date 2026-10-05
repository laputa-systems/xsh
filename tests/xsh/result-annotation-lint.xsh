# Fails until `lint.redundant-result-unit` stops treating a family-typed
# `Result[Unit, Family]` return as the inferred `Result[Unit]`. Removing the
# annotation leaves the `.Variant(...)` in the body with no family to select
# from, so the rewritten file is rejected and the fix round is abandoned.
test test_family_typed_unit_result_annotation_is_not_redundant { |ctx|
  let root = test.temp_dir(ctx, name: "family-typed-unit-result")?
  let candidate = fp"{root}/main.xsh"
  let source = r"""error ScriptError = Failed(kind: Str, message: Str)

proc check(kind: Str) -> Result[Unit, ScriptError] {
  return Err(.Failed(kind: "usage", message: f"{kind} again")) when kind == "b"
}

for kind in ["a", "b"] {
  match check(kind) {
    Err(ScriptError.Failed {kind: label, message}) => print f"{label}: {message}"
    Ok(_) => print "ok"
  }
}
"""
  candidate.write_atomic(source)
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr

  let linted = run.capture --text "xsht" lint --only lint.redundant-result-unit $candidate ?
  assert linted.status.exited_with(0), linted.stderr
  assert "lint.redundant-result-unit" not in linted.stderr, linted.stderr

  let fixing = run.capture --text "xsht" lint --fix $candidate ?
  assert "check.inferred-variant" not in fixing.stderr, fixing.stderr
  let fixed = candidate.read_text()?
  assert "proc check(kind: Str) -> Result[Unit, ScriptError] {" in fixed, fixed
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
}
