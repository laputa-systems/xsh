proc effect_union_inputs(ctx: TestContext) [fs, error] -> Result[{root: Path, source: Str}] {
  let root = test.temp_dir(ctx, name: "effect-union")?
  let fixtures = fp"{ctx.file.parent()}/../fixtures"
  fp"{root}/effects.xsh".write(fp"{fixtures}/modules/full-inference-effect-union/effects.xsh".read_text()?)?
  let source = fp"{fixtures}/runtime/exported-effect-union.xsh".read_text()?
  {root, source}
}

test test_imported_effect_union_preserves_promised_bounds_and_independent_types { |ctx|
  let inputs = effect_union_inputs(ctx)?
  let output = test.run_xsh(ctx, inputs.source + "\nverify_effect_union()?\n", env: {XSH_MODULE_PATH: inputs.root})?
  assert output.success, output.stderr
  assert output.stderr == "", output.stderr
  assert output.stdout == "integer callback\ntext callback\nunion preserved\n", output.stdout
}

test test_imported_effect_union_reports_a_missing_capability { |ctx|
  let inputs = effect_union_inputs(ctx)?
  let output = test.run_xsh(ctx, inputs.source + "\nproc denied() [io, error] { let _ = selected(clock, emit, 7)? }\n", env: {XSH_MODULE_PATH: inputs.root})?
  assert output.status == 2, output.stderr
  assert "[check.effect-violation]" in output.stderr, output.stderr
  assert "time" in output.stderr, output.stderr
  assert "[check.local-inference]" not in output.stderr, output.stderr
  assert output.stdout == "", output.stdout
}

test test_imported_effect_union_reports_a_lexically_excluded_capability { |ctx|
  let inputs = effect_union_inputs(ctx)?
  let output = test.run_xsh(ctx, inputs.source + "\nproc denied() [time, io, error] { without time { let _ = selected(clock, emit, 7)? } }\n", env: {XSH_MODULE_PATH: inputs.root})?
  assert output.status == 2, output.stderr
  assert "[check.effect-violation]" in output.stderr, output.stderr
  assert "without time" in output.stderr, output.stderr
  assert "[check.local-inference]" not in output.stderr, output.stderr
  assert output.stdout == "", output.stdout
}

test test_imported_effect_union_keeps_unknown_dynamic_effects_rejected { |ctx|
  let inputs = effect_union_inputs(ctx)?
  let output = test.run_xsh(ctx, inputs.source + "\nproc denied(callback: Proc) [] -> Int { let _ = callback.call(); 42 }\n", env: {XSH_MODULE_PATH: inputs.root})?
  assert output.status == 2, output.stderr
  assert "[check.effect-violation]" in output.stderr, output.stderr
  assert "unknown" in output.stderr, output.stderr
  assert "[check.local-inference]" not in output.stderr, output.stderr
  assert output.stdout == "", output.stdout
}
