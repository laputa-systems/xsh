test test_index_and_iteration_constrain_conversion { |ctx|
  for declaration in [
    r"""pure paths(values: List[Bytes]) -> Result[List[Path]] {
  let _ = bytes_count(values[0])
  [item as Path for item in values]
}""",
    r"""pure paths(values) -> Result[List[Path]] {
  let _ = bytes_count(values[0])
  [item as Path for item in values]
}""",
  ] {
    let result = test.expect(ctx, r"""pure bytes_count(value: Bytes) -> Int { value.len() }
""" + declaration + r"""
assert paths([b"one"])?.len() == 1
let nonempty: NonEmpty[Bytes] = [b"two"]
assert paths(nonempty)?.len() == 1
""", status: 0)?
    assert result.stderr == "", result.stderr
  }
}

test test_index_conversion_does_not_train_from_callers { |ctx|
  let result = test.run_xsh(ctx, r"""pure paths(values) -> Result[List[Path]] { [item as Path for item in values] }
let _ = paths([b"one"])
""")?
  assert result.success == false
  assert "err[check.conversion]" in result.stderr, result.stderr
}

test test_index_conversion_rejects_maps_and_dynamic_inputs { |ctx|
  let program = r"""pure bytes_count(value: Bytes) -> Int { value.len() }
pure paths(values) -> Result[List[Path]] { let _ = bytes_count(values[0]); [item as Path for item in values] }
"""
  for argument in [r"""let value: Map[Int, Bytes] = {}""", r"""let value: Any = [b"one"]"""] {
    let result = test.run_xsh(ctx, program + argument + "\nlet _ = paths(value)\n")?
    assert result.success == false
    assert "err[check." in result.stderr, result.stderr
  }
}

test test_index_and_iteration_constrain_text_conversion { |ctx|
  for declaration in [
    r"""pure paths(values: List[Str]) -> Result[List[Path]] { let _ = text_count(values[0]); [item as Path for item in values] }""",
    r"""pure paths(values) -> Result[List[Path]] { let _ = text_count(values[0]); [item as Path for item in values] }""",
  ] {
    let result = test.expect(ctx, r"""pure text_count(value: Str) -> Int { value.byte_len() }
""" + declaration + r"""
assert paths(["one"])?.len() == 1
""", status: 0)?
    assert result.stderr == "", result.stderr
  }
}
