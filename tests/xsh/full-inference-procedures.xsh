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

test test_full_inference_proc_immutable_prefix_preserves_the_tail_type { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc copy(value) [] {
  let copied = value
  copied
}
proc copy_integer(value: Int) [] -> Result[Int] {
  let copied = value
  copied
}
let inferred_integer = copy(42)?
let grounded_integer = copy_integer(42)?
let inferred_text = copy("demo")?
assert inferred_integer == 42
assert grounded_integer == 42
assert inferred_text == "demo"
print $inferred_integer
print $grounded_integer
print $inferred_text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n42\ndemo\n"
}

test test_full_inference_proc_branches_keep_one_int_or_str_result { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc mark(label: Str) [io] -> Bool {
  print $label
  return true
}
proc choose(flag, left, right) [io] {
  if flag {
    let selected = mark("left branch")
    left
  } else {
    let selected = mark("right branch")
    right
  }
}
proc choose_integer(flag: Bool, left: Int, right: Int) [io] -> Result[Int] {
  if flag {
    let selected = mark("left branch")
    left
  } else {
    let selected = mark("right branch")
    right
  }
}
proc choose_text(flag: Bool, left: Str, right: Str) [io] -> Result[Str] {
  if flag {
    let selected = mark("left branch")
    left
  } else {
    let selected = mark("right branch")
    right
  }
}
let inferred_integer = choose(true, 42, 7)?
let grounded_integer = choose_integer(true, 42, 7)?
let inferred_text = choose(false, "left", "right")?
let grounded_text = choose_text(false, "left", "right")?
assert inferred_integer == 42
assert grounded_integer == 42
assert inferred_text == "right"
assert grounded_text == "right"
print $inferred_integer
print $grounded_integer
print $inferred_text
print $grounded_text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "left branch\nleft branch\nright branch\nright branch\n42\n42\nright\nright\n"
}

test test_full_inference_generic_statement_only_proc_returns_unit { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc announce(value) [io] {
  print $value
}
proc announce_integer(value: Int) [io] -> Result[Unit] {
  print $value
}
proc announce_text(value: Str) [io] -> Result[Unit] {
  print $value
}
let _ = announce(42)?
let _ = announce("demo")?
let _ = announce_integer(42)?
let _ = announce_text("demo")?
print "complete"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n42\ndemo\ncomplete\n"
}
