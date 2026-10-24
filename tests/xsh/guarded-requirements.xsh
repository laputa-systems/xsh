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
