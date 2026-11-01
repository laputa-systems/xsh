test test_method_result_passes_to_inferred_fallback_parameter { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc apply(value) [] -> Str { value ?? "" }
proc caller(word: Str) [] -> Str { apply(word.byte_slice(0)) }
pure number(value) { value ?? 0 }
proc parse(text: Str) [] -> Int { number(text.parse_int()) }
pure unsigned(value) -> UInt { value ?? 0 }
proc parse_unsigned(text: Str) [] -> UInt { unsigned(text.parse_uint()) }
assert parse_unsigned("19") == 19
assert parse_unsigned("wrong") == 0
assert parse("19") == 19
assert parse("wrong") == 0
assert caller("word") == "word"
assert apply(null) == ""
print caller("word")
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "word\n"
}

test test_method_result_rejects_incompatible_fallback_parameter { |ctx|
  test.expect(
    ctx,
    r"""proc apply(value) [] -> Str { value ?? "" }
proc caller(word: Str) [] -> Str { apply(word.byte_len()) }
let _ = caller("word")
""",
    status: 2,
    stderr: ["[check.type-mismatch]", "expected Str, found Int"],
  )?
}

test test_dynamic_fallback_parameter_is_rejected { |ctx|
  test.expect(
    ctx,
    r"""pure fallback(value) { value ?? 0 }
proc caller(value: Any) [] -> Int { fallback(value) }
""",
    status: 2,
    stderr: ["[check.result-fallback]"],
  )?
}
