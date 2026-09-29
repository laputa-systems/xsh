type InferredPrivateModule = module {
  export pure label(value: Str) -> Str
}

pure inferred_label(name: Str) { name.trim().lower() }
pure inferred_forward(name: Str) { inferred_label(name) }
pure inferred_bool(value: Int) { value > 0 }
pure inferred_returns(value: Int) {
  if value < 0 { return -1 }
  value + 1
}
pure inferred_record(value: Int) { {value, label: "ready"} }

proc test_private_pure_inference_values() [error] {
  test.eq(inferred_forward(" Label "), "label")?
  test.eq(inferred_bool(-1), false)?
  test.eq(inferred_returns(-2), -1)?
  test.eq(inferred_returns(2), 3)?
  test.eq(inferred_record(4).value, 4)?
}

proc test_private_pure_inference_declaration_order(ctx: TestContext) [fs, error] {
  let result = test.run_script(ctx, """
pure first(value: Int) { second(value) }
pure second(value: Int) { value + 1 }
print first(2)
""", [], {}, b"", "inferred-order.xsh")?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "3\n")?
}

proc test_private_pure_inference_rejects_underdetermined_boundaries(ctx: TestContext) [fs, error] {
  for source in [
    "pure empty() { [] }\n",
    "pure captured() { prefix }\nlet prefix = \"later\"\n",
    "pure dynamic(value: Any) { value }\n",
    "pure recursive(value: Int) { recursive(value) }\n",
    "pure first(value: Int) { second(value) }\npure second(value: Int) { first(value) }\n",
    "pure parsed(value: Str) { value.parse_int()? }\n",
    "pure mixed(value: Bool) { if value { return 1 } }\n",
    "pure mismatch(value: Bool) { if value { 1 } else { \"one\" } }\n",
    "pure wrapping(value: Bool) { if value { Ok(1) } else { 1 } }\n",
    "error Failure = Bad(message: Str)\npure failed() { Err(Failure.Bad(\"bad\")) }\n",
    "export pure public_value() { 1 }\n",
  ] {
    let result = test.run_script(ctx, source, [], {}, b"", "inferred-rejected.xsh")?
    test.ok(!result.success, source)?
  }
}

proc test_private_pure_inference_reachable_paths_and_containers(ctx: TestContext) [fs, error] {
  let result = test.run_script(ctx, """
pure early() { return 2; return "unreachable" }
pure selected(flag: Bool) { if flag { [] } else { [1, 2] } }
pure reversed(flag: Bool) { if flag { [1, 2] } else { [] } }
pure outcome(value: Str) { value.parse_int() }
pure propagated(value: Str) { let parsed = value.parse_int()?; Ok(parsed) }
pure optional(flag: Bool) { if flag { 1 } else { null } }
print early() selected(true).len() reversed(false).len() (outcome("3")?) (optional(false) ?? 0) (propagated("4")?)
""", [], {}, b"", "inferred-shapes.xsh")?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "2 0 0 3 0 4\n")?
}

proc test_private_pure_inference_module_private_capture(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "inferred-private-module")?
  let module_path = fp"${root}/helper.xsh"
  module_path.write("""
##! Module with an inferred private helper.
let prefix = "label:"
pure private_label(value: Str) { prefix + value.trim() }
## Renders a label through the private helper.
export pure label(value: Str) -> Str { private_label(value) }
""")?
  let loaded = module.load(module_path)?.require(InferredPrivateModule)?
  test.eq(loaded.label(" ready "), "label:ready")?
}

proc test_private_pure_inference_lexical_dependencies(ctx: TestContext) [fs, error] {
  let result = test.run_script(ctx, """
pure shadow() { let first = 1; first }
let first = shadow()
pure parameter(first: Int) { first + 1 }
let {left, right} = {left: 2, right: 3}
pure destructured() { left + right }
pure pattern(value: Int?) { match value { null => 0, first => first } }
print $first parameter(1) destructured() (pattern(2) ?? 0)
""", [], {}, b"", "inferred-dependencies.xsh")?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "1 2 5 2\n")?
}
