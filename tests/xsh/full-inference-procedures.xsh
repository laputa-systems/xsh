proc full_inference_procedure_modules(ctx: TestContext) [fs, error] -> Result[Path] {
  let modules = test.temp_dir(ctx, name: "full-inference-procedure-modules")?
  let fixture = fp"{ctx.file.parent()}/../fixtures/modules/full-inference-procedures/wrappers.xsh"
  fp"{modules}/wrappers.xsh".write(fixture.read_text()?)?
  modules
}

test test_full_inference_private_and_imported_generic_procs_preserve_result_values { |ctx|
  let modules = full_inference_procedure_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use wrappers as imported
proc private_wrap(value) { value }
let private_integer = private_wrap(42)?
let private_text = private_wrap("private")?
let imported_integer = imported.wrap(7)?
let imported_text = imported.wrap("imported")?
let nested: Result[Int] = imported.wrap(Ok(7))?
let error_payload: Result[Int, Str] = imported.wrap(Err("kept"))?
assert private_integer == 42
assert private_text == "private"
assert imported_integer == 7
assert imported_text == "imported"
assert nested == Ok(7)
assert error_payload == Err("kept")
print $private_integer
print $private_text
print $imported_integer
print $imported_text
print "nested result preserved"
print "error payload preserved"
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\nprivate\n7\nimported\nnested result preserved\nerror payload preserved\n"
}

test test_full_inference_finite_io_callback_passes_through_generic_proc { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc report(value: Str) [io] -> Str {
  print $value
  return value
}
proc apply(callback, value) [io] {
  callback(value)
}
let result = apply(report, "ground helper")?
assert result == "ground helper"
print $result
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "ground helper\nground helper\n"
}

test test_full_inference_explicit_empty_effect_bound_rejects_io_callback { |ctx|
  test.expect(
    ctx,
    r"""proc report(value: Str) [io] -> Str {
  print $value
  return value
}
proc deny(callback, value) [] {
  callback(value)
}
let result = deny(report, "forbidden")?
""",
    status: 2,
    stderr: ["[check.effect-violation]"],
  )?
}

test test_full_inference_proc_mutable_capture_remains_monomorphic { |ctx|
  test.expect(
    ctx,
    r"""var captured = [1]
proc remember(value) {
  captured = captured.push(value)
  value
}
let integer = remember(42)?
let text = remember("demo")?
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}
