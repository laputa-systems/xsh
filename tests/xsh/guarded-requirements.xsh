test guarded_requirement_imports { |ctx|
  let modules = test.temp_dir(ctx, name: "guarded-requirement-modules")?
  for name in ["guarded_render", "guarded_preload"] {
    fp"{modules}/{name}.xsh".write(fp"tests/fixtures/modules/guarded-requirements/{name}.xsh".read_text()?)?
  }
  for entry in [
    {name: "render", function: "verify_guarded_render"},
    {name: "preload", function: "verify_guarded_preload"},
  ] {
    let source = fp"tests/fixtures/runtime/guarded-requirements-{entry.name}.xsh".read_text()?
    let output = test.run_xsh(ctx, source + f"\n{entry.function}()?\n", env: {XSH_MODULE_PATH: modules})?
    assert output.status == 0, output.stderr
    assert output.stdout == ""
  }
}

test guarded_requirements_without_payload_consumption { |ctx|
  let source = p"tests/fixtures/runtime/guarded-requirements-benign.xsh".read_text()?
  test.expect(ctx, source + "\nverify_guarded_benign()?\n", status: 0)?
}

test guarded_callback_effects_and_absence { |ctx|
  let source = p"tests/fixtures/runtime/guarded-requirements-callbacks.xsh".read_text()?
  test.expect(ctx, source + "\nverify_guarded_callbacks()?\n", status: 0)?
}

test guarded_callback_effect_imports { |ctx|
  let modules = test.temp_dir(ctx, name: "guarded-callback-modules")?
  fp"{modules}/guarded_callbacks.xsh".write(p"tests/fixtures/modules/guarded-requirements/guarded_callbacks.xsh".read_text()?)?
  let source = p"tests/fixtures/runtime/guarded-requirements-callbacks.xsh".read_text()?
    .replace("let absent = guarded_callback_absence()", with: "let absent = guarded_callbacks.absent()")
    .replace("let _ = guarded_fixed_callback(", with: "let _ = guarded_callbacks.fixed(")
    .replace("let _ = guarded_inferred_callback(", with: "let _ = guarded_callbacks.relay(")
  let output = test.run_xsh(ctx, "use guarded_callbacks\n" + source + "\nverify_guarded_callbacks()?\n", env: {XSH_MODULE_PATH: modules})?
  assert output.status == 0, output.stderr
  assert output.stdout == ""
}
