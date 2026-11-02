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
  let output = test.run_xsh(ctx, r"""pure paths(values) -> Result[List[Path]] { [item as Path for item in values] }
let first = paths([b"one"])?
let second = paths(["two"])?
print $first.len()
print $second.len()
""")?
  assert output.success, output.stderr
  assert output.stdout == "1\n1\n", output.stdout
  let rejected = test.run_xsh(ctx, r"""pure paths(values) -> Result[List[Path]] { [item as Path for item in values] }
let _ = paths([1])
""")?
  assert rejected.success == false
  assert "err[check." in rejected.stderr, rejected.stderr
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

test test_index_and_slice_iteration_constrain_conversion { |ctx|
  for declaration in [
    r"""pure tail_paths(values: List[Bytes]) -> Result[List[Path]] {
  let _ = bytes_count(values[0])
  [item as Path for item in values[1..]]
}""",
    r"""pure tail_paths(values) -> Result[List[Path]] {
  let _ = bytes_count(values[0])
  [item as Path for item in values[1..]]
}""",
  ] {
    let result = test.expect(ctx, r"""pure bytes_count(value: Bytes) -> Int { value.len() }
""" + declaration + r"""
assert tail_paths([b"one", b"two"])?.len() == 1
let nonempty: NonEmpty[Bytes] = [b"three", b"four", b"five"]
assert tail_paths(nonempty)?.len() == 2
""", status: 0)?
    assert result.stderr == "", result.stderr
  }
}

test test_slice_iteration_conversion_follows_each_element_type { |ctx|
  let output = test.run_xsh(ctx, r"""pure tail_paths(values) -> Result[List[Path]] {
  [item as Path for item in values[1..]]
}
let bytes_tail = tail_paths([b"one", b"two"])?
let text_tail = tail_paths(["one", "two"])?
print $bytes_tail.len()
print $text_tail.len()
""")?
  assert output.success, output.stderr
  assert output.stdout == "1\n1\n", output.stdout
  for argument in ["b\"xy\"", "[1, 2]"] {
    let rejected = test.run_xsh(ctx, r"""pure tail_paths(values) -> Result[List[Path]] {
  [item as Path for item in values[1..]]
}
let _ = tail_paths(""" + argument + ")\n")?
    assert rejected.success == false, argument
    assert "err[check.operator-type]" in rejected.stderr, rejected.stderr
  }
}
