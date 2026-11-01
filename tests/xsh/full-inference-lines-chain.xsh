test test_lines_chain_uses_body_evidence { |ctx|
  for declaration in [
    r"""pure lines(text: Str) -> List[Str] { text.lines().extend(["sentinel"]) }""",
    r"""pure lines(text) -> List[Str] { text.lines().extend(["sentinel"]) }""",
  ] {
    let result = test.expect(ctx, declaration + r"""
assert lines("x\ny") == ["x", "y", "sentinel"]
""", status: 0)?
    assert result.stderr == "", result.stderr
  }
}

test test_byte_lines_chain_uses_body_evidence { |ctx|
  for declaration in [
    r"""pure lines(text: Bytes) -> List[Bytes] { text.lines().extend([b"sentinel"]) }""",
    r"""pure lines(text) -> List[Bytes] { text.lines().extend([b"sentinel"]) }""",
  ] {
    let result = test.expect(ctx, declaration + r"""
assert lines(b"x\ny") == [b"x", b"y", b"sentinel"]
""", status: 0)?
    assert result.stderr == "", result.stderr
  }
}

test test_lines_without_body_domain_stays_ambiguous { |ctx|
  let result = test.run_xsh(ctx, r"""pure lines(text) { text.lines() }
let _ = lines("a")
""")?
  assert result.success == false
  assert "err[check.ambiguous-overload]" in result.stderr, result.stderr
}

test test_lines_iteration_uses_item_evidence { |ctx|
  for declaration in [
    r"""pure first_length(data: Bytes) -> Int {
  var length = 0
  for item in data.lines() { length = item.len(); break }
  length
}""",
    r"""pure first_length(data) -> Int {
  var length = 0
  for item in data.lines() { length = item.len(); break }
  length
}""",
  ] {
    let result = test.expect(ctx, declaration + r"""
assert first_length(b"abc\n") == 3
""", status: 0)?
    assert result.stderr == "", result.stderr
  }
}

test test_lines_comprehension_uses_item_evidence { |ctx|
  for declaration in [
    r"""pure lines(data: Bytes) -> List[Bytes] { [line for line in data.lines()] }""",
    r"""pure lines(data) -> List[Bytes] { [line for line in data.lines()] }""",
  ] {
    let result = test.expect(ctx, declaration + r"""
assert lines(b"x\ny") == [b"x", b"y"]
""", status: 0)?
    assert result.stderr == "", result.stderr
  }
}

test test_pure_iteration_rejects_effectful_path_lines { |ctx|
  let result = test.run_xsh(ctx, r"""pure inspect(data: Path) -> Int {
  var count = 0
  for item in data.lines() { count = count + item.byte_len() }
  count
}
""")?
  assert result.success == false
  assert "err[check." in result.stderr, result.stderr
}

test test_lines_body_domain_rejects_another_receiver { |ctx|
  let result = test.run_xsh(ctx, r"""pure lines(text) -> List[Str] { text.lines().extend(["sentinel"]) }
let _ = lines(b"x")
""")?
  assert result.success == false
  assert "err[check." in result.stderr, result.stderr
}
