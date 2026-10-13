test test_declaration_is_checked_without_execution { |ctx|
  let result = test.run_xsh(
    ctx,
    """test registered {
  assert false
}
run printf ready
""",
  )?
  assert result.status == 0
  assert result.stdout == "ready"
}

test test_declaration_bool_statement_is_rejected { |ctx|
  let result = test.run_xsh(
    ctx,
    """test registered {
  1 == 2
}
""",
  )?
  assert result.status != 0
  assert "check.bool-statement" in result.stderr, result.stderr
}

test test_declaration_body_is_checked { |ctx|
  let result = test.run_xsh(
    ctx,
    """test checked {
  missing_name()
}
""",
  )?
  assert result.status != 0
}

test context_parameter_is_immutable { |ctx|
  let result = test.run_xsh(
    ctx,
    """test immutable { |ctx|
  ctx = ctx
}
""",
  )?
  assert result.status != 0
  assert "immutable" in result.stderr
}

test context_parameter_has_test_context_type { |ctx|
  let result = test.run_xsh(
    ctx,
    """test typed { |ctx|
  let name: Int = ctx.name
}
""",
  )?
  assert result.status != 0
  assert "check.type-mismatch" in result.stderr
}

test declarations_cannot_be_called { |ctx|
  let result = test.run_xsh(
    ctx,
    """test hidden {}
hidden()
""",
  )?
  assert result.status != 0
}

test declarations_cannot_be_nested_or_exported { |ctx|
  for source in ["proc outer() { test nested {} }", "export test public {}"] {
    let result = test.run_xsh(ctx, source)?
    assert result.status != 0
  }
}

test declarations_enforce_effects_and_result_unit { |ctx|
  for source in ["test restricted [] { let _ = time.now() }", "test wrong { return Ok(3) }", "test wrong { |a, b| }"] {
    let result = test.run_xsh(ctx, source)?
    assert result.status != 0
  }
}

test main_does_not_execute { |ctx|
  let result = test.run_xsh(
    ctx,
    """test main { assert false }
run printf ready
""",
  )?
  assert result.status == 0
  assert result.stdout == "ready"
}

test discard_context_is_allowed { |_| }

test declaration_names_reject_duplicates_and_callable_collisions { |ctx|
  for source in [
    """test same {}
test same {}
""",
    """pure same() -> Int { 1 }
test same {}
""",
  ] {
    let result = test.run_xsh(ctx, source)?
    assert result.status != 0
    assert "check.duplicate-name" in result.stderr
  }
}

test imported_declarations_do_not_execute { |ctx|
  let root = test.temp_dir(ctx, name: "imported-test-declaration")?
  fp"{root}/helper.xsh".write_atomic(r"""##! Helper with a declared test.
## The shared value.
export pure value() -> Int { 7 }
test helper_test {
  print "TEST EXECUTED"
  assert false
}
""")
  let result = test.run_xsh(
    ctx,
    """use helper
print \${helper.value()}
""",
    env: {XSH_MODULE_PATH: root},
  )?
  assert result.status == 0
  assert result.stdout == """7
"""
}

test frame_routed_run_script_child_selects_eager_values { |ctx|
  let _ = test.expect(
    ctx,
    """let seed = [
  {value: [3, 5]},
]
let rows = [@seed]
let selected_index: Int = if rows[-1].value[-1] == 5 { 0 } else { 99 }
let selected: Int = [7][selected_index]
""",
    status: 0,
  )?
}

test frame_routed_run_xsh_child_selects_eager_values { |ctx|
  let result = test.run_xsh(
    ctx,
    """let seed = [
  {value: [3, 5]},
]
let rows = [@seed]
let selected_index: Int = if rows[-1].value[-1] == 5 { 0 } else { 99 }
let selected: Int = [7][selected_index]
""",
  )?
  assert result.status == 0, result.stderr
}

test frame_routed_child_keeps_assertion_failure_details { |ctx|
  test.expect(ctx, "assert false, \"frame assertion\"\n", status: 3, stderr: ["frame assertion"])?
  test.expect(
    ctx,
    "assert 1 == 2\n",
    status: 3,
    stderr: ["1 == 2", "left: 1", "right: 2"],
  )?
}

test frame_routed_typed_scalars_keep_short_circuit_and_error_flow { |ctx|
  let typed = r"""pure frame_typed_and(keep: Bool) -> Bool {
  keep and 1 / 0 == 0
}
pure frame_typed_or(keep: Bool) -> Bool {
  keep or 1 / 0 == 0
}
pure frame_typed_sum(left: Int, right: Int) -> Int {
  left + right
}
pure frame_typed_byte(text: Str, index: Int) -> Int {
  let byte = text.byte_at(index) ?? -1
  byte
}
"""
  test.expect(
    ctx,
    typed + "assert !frame_typed_and(false)\nassert frame_typed_or(true)\nassert frame_typed_sum(20, 22) == 42\nassert frame_typed_byte(\"x\", 0) == 120\nassert frame_typed_byte(\"x\", 1) == -1\n",
    status: 0,
  )?

  test.expect(ctx, "var selected: Bool = false\nselected = true\nassert selected\n", status: 0)?
  test.expect(
    ctx,
    "type FrameTypedRow = {value: Int}\nvar row: FrameTypedRow = {value: 4}\nrow.value += 1\nassert row.value == 5\n",
    status: 0,
  )?

  test.expect(
    ctx,
    r"""let no: Result[Bool, Str] = Ok(false)
let bad: Result[Bool, Str] = Err("cannot stat bad")
let yes: Result[Bool, Str] = Ok(true)
var value: Int = 7
if (no and bad) or yes { value += 1 }
assert value == 8

""",
    status: 0,
  )?

  test.expect(
    ctx,
    "let bad: Result[Bool, Str] = Err(\"cannot stat bad\")\nif bad and true { assert false }\n",
    status: 3,
    stderr: ["propagated error"],
  )?

  test.expect(
    ctx,
    "var value: Int = 9223372036854775807\nvalue += 1\n",
    status: 3,
    stderr: ["integer-overflow"],
  )?
}

test ordinary_child_keeps_recursive_fallback_for_unsupported_expressions { |ctx|
  test.expect(ctx, "let value = loop { break 0 }\n", status: 0)?
}
