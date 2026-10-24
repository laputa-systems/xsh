test test_full_inference_null_comparisons_preserve_generic_values { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure equal_left(value) { null == value }
pure equal_right(value) { value == null }
pure unequal_left(value) { null != value }
pure unequal_right(value) { value != null }
pure observe_equal_left(value) { let _ = null == value; value }
pure observe_equal_right(value) { let _ = value == null; value }
pure observe_unequal_left(value) { let _ = null != value; value }
pure observe_unequal_right(value) { let _ = value != null; value }

let opaque: Result[Int, Str] = Err("retained")
let present: Int? = 7
let missing: Int? = null
assert !equal_left(7)
assert !equal_right("kept")
assert !equal_left(opaque)
assert !equal_right(present)
assert equal_left(missing)
assert equal_right(missing)
assert unequal_left("kept")
assert unequal_right(7)
assert unequal_right(opaque)
assert unequal_left(present)
assert !unequal_left(missing)
assert !unequal_right(missing)

assert observe_equal_left(7) == 7
assert observe_equal_left("kept") == "kept"
assert observe_equal_left(opaque) == opaque
assert observe_equal_right(7) == 7
assert observe_equal_right("kept") == "kept"
assert observe_equal_right(opaque) == opaque
assert observe_unequal_left(7) == 7
assert observe_unequal_left("kept") == "kept"
assert observe_unequal_left(opaque) == opaque
assert observe_unequal_right(7) == 7
assert observe_unequal_right("kept") == "kept"
assert observe_unequal_right(opaque) == opaque
let kept: Int? = observe_equal_left(present)
let absent: Int? = observe_unequal_left(missing)
assert kept == 7
assert absent == null
print "null comparisons preserve values"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "null comparisons preserve values\n"
}

test test_full_inference_fallback_preserves_selected_call_conversions { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure fallback(value) { value ?? 0 }
pure constant_with_tag(tag) { let _ = tag; fallback(31) }
pure nested(value) { value ?? Ok(0) }
let recovered = fallback
assert fallback(7) == 7
assert fallback(value: 23) == 23
assert recovered(37) == 37
assert constant_with_tag("kept") == 31
assert fallback(null) == 0
let present: Int? = 11
let missing: Int? = null
assert fallback(present) == 11
assert fallback(missing) == 0
let success: Result[Int, Str] = Ok(13)
let failure: Result[Int, Str] = Err("retained")
assert fallback(success) == 13
assert fallback(failure) == 0
let optional_result: Result[Int, Str]? = failure
assert nested(optional_result) == failure
print "fallback conversions preserve values"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "fallback conversions preserve values\n"
}

test test_full_inference_fallback_keeps_independent_numeric_requirements { |ctx|
  test.expect(
    ctx,
    r"""pure fallback(value) {
  let _ = value ?? 0
  value + 1
}
let invalid = fallback(7)
""",
    status: 2,
    stderr: ["[check.operator-type]", "Add is not defined for Int? and Int"],
  )?
}

test test_full_inference_nullable_target_preserves_source_presence { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Item = {value: Int}
pure accepts(value: Item?) -> Int { let _ = value; 0 }
pure read(value) -> Int { let _ = accepts(value); value.value }
pure forward(value) { accepts(value) }
pure forward_named(value) { accepts(value: value) }
pure forward_splice(value) { accepts(@[value]) }
let present: Item? = {value: 11}
let missing: Item? = null
assert read({value: 7}) == 7
assert forward({value: 7}) == 0
assert forward(present) == 0
assert forward(missing) == 0
assert forward(null) == 0
assert forward_named({value: 7}) == 0
assert forward_named(null) == 0
assert forward_splice({value: 7}) == 0
assert forward_splice(null) == 0
print "source presence preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "source presence preserved\n"
}

test test_full_inference_nullable_target_keeps_nullable_field_rejection { |ctx|
  test.expect(
    ctx,
    r"""type Item = {value: Int}
pure accepts(value: Item?) -> Int { let _ = value; 0 }
pure read(value: Item?) -> Int { let _ = accepts(value); value.value }
let missing: Item? = null
let invalid = read(missing)
""",
    status: 2,
    stderr: ["[check.field-access]"],
  )?
}

test test_full_inference_nullable_target_keeps_any_validation_boundary { |ctx|
  test.expect(
    ctx,
    r"""type Item = {value: Int}
pure accepts(value: Item?) -> Int { let _ = value; 0 }
let unchecked: Any = {value: 7}
let invalid = accepts(unchecked)
""",
    status: 2,
    stderr: ["[check.dynamic-boundary]"],
  )?
}

test test_full_inference_nullable_target_keeps_unsigned_validation { |ctx|
  for source in [
    r"""pure accepts(value: UInt?) -> Int { let _ = value; 0 }
pure forward(value: Int) { accepts(value) }
let dynamic: Int = -1
let rejected = forward(dynamic)
""",
    r"""pure accepts(value: UInt?) -> Int { let _ = value; 0 }
pure forward(value) { accepts(value) }
let dynamic: Int = -1
let rejected = forward(dynamic)
""",
  ] {
    test.expect(ctx, source, status: 3, stderr: ["type-error", "UInt"])?
  }
}

test test_full_inference_optional_guard_preserves_original_immutable_reads { |ctx|
  let source = p"tests/fixtures/runtime/generic-guard-original-slot.xsh".read_text()?
  test.expect(ctx, source + "\nverify_guard_original_slot()?\n", status: 0)?
}

test test_full_inference_line_loop_items_remain_valid_generic_arguments { |ctx|
  let source = p"tests/fixtures/runtime/generic-line-loop-arguments.xsh".read_text()?
  test.expect(ctx, source + "\nverify_line_loop_arguments()?\n", status: 0)?
}
