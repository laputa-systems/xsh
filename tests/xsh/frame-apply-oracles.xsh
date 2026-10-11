test test_frame_apply_function_reference_value { |ctx|
  test.expect(
    ctx,
    """
pure increment(value: Int) -> Int { value + 1 }
let alias = increment
assert alias(41) == 42
""",
    status: 0,
  )?
}

test test_frame_apply_path_from_value { |ctx|
  test.expect(
    ctx,
    """
let candidate = Path("folder/file")
assert candidate == p"folder/file"
""",
    status: 0,
  )?
}

test test_frame_apply_path_from_preserves_type_errors { |ctx|
  test.expect(
    ctx,
    "let value: Any = 42\nlet candidate = Path(value.require(Str)?)\n",
    status: 3,
    stderr: ["schema"],
  )?
}

test test_frame_apply_empty_map { |ctx|
  test.expect(
    ctx,
    "let empty: Map[Str, Int] = {}\nassert empty.len() == 0\n",
    status: 0,
  )?
}

test test_frame_apply_bytes_concat_keeps_element_order { |ctx|
  test.expect(
    ctx,
    """
let joined = bytes.concat([b"a", b"b"])
assert joined == b"ab"
""",
    status: 0,
  )?
}

test test_frame_apply_range_evaluates_bounds_in_source_order { |ctx|
  test.expect(
    ctx,
    """
var order: List[Str] = []
let values = range(
  if true { order += ["start"]; 2 } else { 0 },
  if true { order += ["end"]; 5 } else { 0 },
).collect()
assert values == [2, 3, 4]
assert order == ["start", "end"]
""",
    status: 0,
  )?
}

test test_frame_apply_enum_tag_value { |ctx|
  test.expect(
    ctx,
    """
enum FrameApplyChoice { Empty, Selected(Int) }
let selected: FrameApplyChoice = Selected(42)
match selected {
  Selected(value) => { assert value == 42 },
  Empty => { assert false },
}
""",
    status: 0,
  )?
}

test test_frame_apply_slice_evaluates_receiver_and_bounds_in_order { |ctx|
  test.expect(
    ctx,
    """
var order: List[Str] = []
let selected = (if true { order += ["receiver"]; [10] } else { [20] })[if true { order += ["start"]; 0 } else { 1 }..if true { order += ["end"]; 1 } else { 0 }]
assert selected == [10]
assert order == ["receiver", "start", "end"]
""",
    status: 0,
  )?
}

test test_frame_apply_string_byte_length { |ctx|
  test.expect(ctx, "assert \"é\".byte_len() == 2\n", status: 0)?
}

test test_frame_apply_string_byte_at_keeps_optional_values { |ctx|
  test.expect(
    ctx,
    "assert \"é\".byte_at(1) == 169\nassert \"é\".byte_at(2) == null\n",
    status: 0,
  )?
}

test test_frame_apply_string_predicate { |ctx|
  test.expect(
    ctx,
    "assert \"alpha\".starts_with(\"al\")\nassert !\"alpha\".ends_with(\"al\")\n",
    status: 0,
  )?
}

test test_frame_apply_regex_compile_keeps_errors_lazy { |ctx|
  test.expect(
    ctx,
    """
let source = env.get("XSH_FRAME_REGEX")?
let invalid = env.get("XSH_FRAME_INVALID_REGEX")?
let pattern = if true {
  regex.compile(source)?
} else {
  regex.compile(invalid)?
}
assert pattern.matches("xxx")
assert match regex.compile(invalid) { Err(_) => true, Ok(_) => false }
""",
    status: 0,
    env: {XSH_FRAME_REGEX: "^x+$", XSH_FRAME_INVALID_REGEX: "("},
  )?
}

test test_frame_apply_json_encode_keeps_errors_lazy { |ctx|
  test.expect(
    ctx,
    """
let invalid: Any = b"not-json"
let payload: Any = {value: 7}
let encoded = if true {
  json.encode(payload)?
} else {
  json.encode(invalid)?
}
assert encoded == "{\\"value\\":7}"
assert match json.encode(invalid) { Err(_) => true, Ok(_) => false }
""",
    status: 0,
  )?
}

test test_frame_apply_byte_at_default_fold_routes { |ctx|
  test.expect(
    ctx,
    """
pure byte_or(text: Str, index: Int) -> Int {
  let value = text.byte_at(index) ?? -1
  value
}
assert byte_or("x", 0) == 120
assert byte_or("x", 1) == -1
""",
    status: 0,
  )?
}
