proc full_inference_callable_relay_modules(ctx: TestContext) [fs, error] -> Result[Path] {
  let modules = test.temp_dir(ctx, name: "full-inference-callable-relays")?
  fp"{modules}/relays.xsh".write(
    r"""##! Generic callable relays for full inference tests.

## Returns its input through the proc result envelope.
export proc wrap(value) {
  value
}

## Yields each value lazily and records the pull order.
export stream replay(value, count) {
  repeat count times {
    print f"pull:{value}"
    yield value
  }
}
""",
  )?
  modules
}

test test_full_inference_imported_generic_proc_and_stream_aliases_retain_schemes { |ctx|
  let modules = full_inference_callable_relay_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use relays as flow
let wrap = flow.wrap
let integer = wrap(42)?
let text = wrap("demo")?
let replay = flow.replay
let integer_producer = replay(7, 2)
print "integer producer created"
let integers = integer_producer |> collect()
let text_producer = replay("row", 1)
print "text producer created"
let rows = text_producer |> collect()
assert integer == 42
assert text == "demo"
assert integers == [7, 7]
assert rows == ["row"]
print $integer
print $text
print $integers
print $rows
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "integer producer created\npull:7\npull:7\ntext producer created\npull:row\n42\ndemo\n[7, 7]\n[row]\n"
}

test test_full_inference_compatible_generic_callable_selection_instantiates_per_call { |ctx|
  let cases = [
    {
      selection: "left",
      expected: "left:42\nleft:demo\n",
    },
    {
      selection: "right",
      expected: "right:42\nright:demo\n",
    },
  ]
  for case in cases {
    let output = test.run_xsh(
      ctx,
      r"""pure left(value) {
  {source: "left", value: value}
}
pure right(value) {
  {source: "right", value: value}
}
let choose_left = env.get_or("XSH_FULL_INFERENCE_SELECTION", "left")? == "left"
let selected = if choose_left {
  let candidate = left
  candidate
} else {
  let candidate = right
  candidate
}
let integer = selected(42)
let text = selected("demo")
print f"{integer.source}:{integer.value}"
print f"{text.source}:{text.value}"
""",
      env: {XSH_FULL_INFERENCE_SELECTION: case.selection},
    )?
    assert output.success, output.stderr
    assert output.stdout == case.expected, output.stdout
  }
}

test test_full_inference_conditional_selection_rejects_incompatible_callables { |ctx|
  test.expect(
    ctx,
    r"""pure integer(value: Int) -> Int {
  value
}
pure text(value: Str) -> Str {
  value
}
let choose_integer = env.get_or("XSH_FULL_INFERENCE_SELECTION", "integer")? == "integer"
let selected = if choose_integer {
  let candidate = integer
  candidate
} else {
  let candidate = text
  candidate
}
let invalid = selected(42)
""",
    status: 2,
    stderr: ["[check.callable-mismatch]"],
  )?
}
