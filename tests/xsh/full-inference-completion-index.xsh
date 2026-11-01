test test_inferred_completion_preserves_error_with_indexed_input { |ctx|
  let output = test.run_xsh(ctx, r"""pure identity(value: Str) -> Result[Str] { value }
proc page(items) { identity(items[0])? }
proc forward(items) { page(items)? }
assert page(["ready"])? == "ready"
assert forward(["ready"])? == "ready"
let values: Map[UInt, Str] = {[0]: "mapped"}
assert page(values)? == "mapped"
print "ok"
""")?
  assert output.success, output.stderr
  assert output.stdout == "ok\n", output.stdout
}

test test_inferred_completion_rejects_bad_index_inputs { |ctx|
  let output = test.run_xsh(ctx, r"""pure identity(value: Str) -> Result[Str] { value }
proc page(items) { identity(items[0])? }
print page({[false]: "invalid"})
""")?
  assert ! output.success, output.stdout
  assert "err[check." in output.stderr, output.stderr
}

test test_inferred_completion_keeps_unsigned_key_validation { |ctx|
  let output = test.run_xsh(ctx, r"""pure identity(value: Str) -> Result[Str] { value }
proc page(items, key: Int) { identity(items[key])? }
let values: Map[UInt, Str] = {[0]: "mapped"}
assert page(values, 0)? == "mapped"
assert page(values, -1) is Err(_)
print "ok"
""")?
  assert output.success, output.stderr
  assert output.stdout == "ok\n", output.stdout
}

test test_indexed_completion_preserves_non_error_payloads { |ctx|
  let output = test.run_xsh(ctx, r"""pure reject(value: Str) -> Result[Str, Str] { Err(value) }
proc page(items) { reject(items[0])? }
let failure: Result[Str, Str] = page(["declined"])
assert failure is Err("declined")
print "ok"
""")?
  assert output.success, output.stderr
  assert output.stdout == "ok\n", output.stdout
}

test test_indexed_completion_preserves_guard_requirements { |ctx|
  let output = test.run_xsh(ctx, r"""pure identity(value: Str) -> Result[Str, Str] { Err(value) }
proc page(items, marker) {
  if marker != null { let _ = marker.byte_len() }
  identity(items[0])?
}
let marker: Str? = "present"
let result: Result[Str, Str] = page(["declined"], marker)
assert result is Err("declined")
print "ok"
""")?
  assert output.success, output.stderr
  assert output.stdout == "ok\n", output.stdout
}

test test_indexed_completion_joins_conditional_validation_with_text_errors { |ctx|
  let output = test.run_xsh(ctx, r"""pure reject(value: Str) -> Result[Str, Str] { Err(value) }
proc page(items, key: Int) { reject(items[key])? }
proc forward(items, key: Int) { page(items, key)? }
proc apply(items, key: Int, action) { action(items[key])? }
let values: Map[UInt, Str] = {[0]: "declined"}
let raw: Result[Str, Str] = apply(["raw"], 0, reject)
let normalized: Result[Str, Error] = page(values, 0)
let forwarded: Result[Str, Error] = forward(values, 0)
let dynamic: Result[Str, Error] = apply(values, 0, reject)
let invalid: Result[Str, Error] = page(values, -1)
assert raw is Err("raw")
assert invalid is Err(_)
for failure in [normalized, forwarded, dynamic] {
  if let Err(error) = failure { assert error.message == "declined" } else { assert false }
}
print "conditional errors preserved"
""")?
  assert output.success, output.stderr
  assert output.stdout == "conditional errors preserved\n"
}

test test_indexed_completion_preserves_nominal_error_payloads { |ctx|
  let output = test.run_xsh(ctx, r"""error Declined = Text
pure reject(value: Str) -> Result[Str, Declined] { Err(Declined.Text(value)) }
proc page(items) { reject(items[0])? }
let failure: Result[Str, Declined] = page(["nominal"])
if let Err(error) = failure { assert error.message == "nominal" } else { assert false }
print "nominal completion preserved"
""")?
  assert output.success, output.stderr
  assert output.stdout == "nominal completion preserved\n"
}
