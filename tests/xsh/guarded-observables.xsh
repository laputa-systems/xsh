proc guarded_observables_modules(ctx: TestContext) [fs, error] -> Result[Path, Error] {
  let modules = test.temp_dir(ctx, name: "guarded-observables")?
  fp"{modules}/guarded-observables.xsh".write(p"tests/fixtures/modules/guarded-observables.xsh".read_text()?)?
  modules
}

test guarded_observables_preserve_values_effects_and_cleanup { |ctx|
  let modules = guarded_observables_modules(ctx)?
  let source = p"tests/fixtures/runtime/guarded-observables.xsh".read_text()?
  let output = test.run_xsh(ctx, source + "\nverify_guarded_observables()?\n", env: {XSH_MODULE_PATH: modules})?
  assert output.success, output.stderr
  assert output.stdout == "observer:4\nobserver:7\nobserver:-1\nobserver:4\nobserver:7\nobserver:-1\n"
}

test guarded_observables_preserve_failure_cause_and_context { |ctx|
  let modules = guarded_observables_modules(ctx)?
  let source = p"tests/fixtures/runtime/guarded-observables.xsh".read_text()?
  let output = test.run_xsh(ctx, source + "\npropagate_guarded_failure()?\n", env: {XSH_MODULE_PATH: modules})?
  assert output.status == 3
  assert output.stdout == "observer:-1\nargument,callback,defer\n1\n"
  assert "GuardedCallbackFailure.Rejected" in output.stderr, output.stderr
  assert "observer failed" in output.stderr, output.stderr
  assert "observer cause" in output.stderr, output.stderr
  assert "guarded observer" in output.stderr, output.stderr
}

test guarded_observables_retain_written_and_mutable_restrictions { |ctx|
  for example in [
    {source: "pure wrong(value: Str?) { if value != null { value.parent() } else { null } }\nlet _ = wrong(\"child\")\n", diagnostic: "[check.unknown-method]"},
    {source: "type Observer = proc(value: Int) [io, error] -> Result[Int, Error]\nproc wrong(action: Observer?, value: Int) [] -> Result[Int?, Error] { if action != null { action(value)? } else { null } }\n", diagnostic: "[check.effect-violation]"},
    {source: "pure wrong(value: Int?) { var selected = value; if selected != null { selected = null; selected + 1 } else { 0 } }\nlet _ = wrong(7)\n", diagnostic: "[check.operator-type]"},
  ] {
    let output = test.run_xsh(ctx, example.source)?
    assert output.status == 2
    assert example.diagnostic in output.stderr, output.stderr
  }
}
