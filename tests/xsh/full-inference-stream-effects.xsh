test test_full_inference_stream_effect_is_lazy_through_returned_alias { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""stream values(value) {
  let _ = time.now()
  print f"pulled:{value}"
  yield value
}
proc make() -> Result[Stream[Int]] {
  let create = values
  Ok(create(42))
}
let producer = make()?
print "created"
for item in producer {
  print $item
  break
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "created\npulled:42\n42\n"
}

test test_full_inference_inferred_stream_effect_alias_rejects_empty_bound { |ctx|
  test.expect(
    ctx,
    r"""stream values(value) {
  let _ = time.now()
  print f"pulled:{value}"
  yield value
}
proc denied() [] -> Result[Stream[Int]] {
  let create = values
  Ok(create(42))
}
""",
    status: 2,
    stderr: ["[check.effect-violation]", "time"],
  )?
}

test test_full_inference_stream_effect_survives_inferred_returned_producer { |ctx|
  test.expect(
    ctx,
    r"""stream values(value) {
  let _ = time.now()
  print f"pulled:{value}"
  yield value
}
proc make() -> Result[Stream[Int]] {
  let create = values
  Ok(create(42))
}
proc denied() [] -> Result[Stream[Int]] { make() }
""",
    status: 2,
    stderr: ["[check.effect-violation]", "time"],
  )?
}

test test_full_inference_stream_effect_alias_typed_ground_control { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""stream values(value: Int) [time] -> Stream[Int] {
  let _ = time.now()
  print f"pulled:{value}"
  yield value
}
proc make() [time] -> Result[Stream[Int]] {
  Ok(values(42))
}
let producer = make()?
print "created"
for item in producer {
  print $item
  break
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "created\npulled:42\n42\n"
}

test test_full_inference_generic_returns_resolve_iteration_sources { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure select(items, flag: Bool) { let _ = flag; items }
pure relay(items) { select(...{flag: true, items: items}) }
stream values() [io] -> Stream[Int] { print "pulled"; yield 7 }
print "created"
for item in relay(values()) { assert item == 7 }
assert [item for item in relay([2, 4])] == [2, 4]
assert [item for item in relay("éx")] == ["é", "x"]
assert [item for item in relay(b"\x01\x02")] == [1, 2]
let indexed = {item: item for item in relay([3, 4])}
assert indexed.get(3)? == 3
for entry in relay({["key"]: 9}) { assert entry.key == "key"; assert entry.value == 9 }
let result: Result[List[Int]] = Ok([5])
for item in relay(result) { assert item == 5 }
print "checked"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "created\npulled\nchecked\n"
}

test test_full_inference_generic_stream_iteration_preserves_capture_errors { |ctx|
  test.expect(
    ctx,
    r"""error CaptureFailure = Failed(message: Str)
error OtherFailure = Changed(message: Str)
stream failing(other: Result[Int, OtherFailure]) [error] -> Stream[Int] { yield other? }
pure select(items, flag: Bool) { let _ = flag; items }
pure relay(items) { let arguments = {flag: true, items: items}; select(...arguments) }
proc captured(other: Result[Int, OtherFailure]) [error] -> Result[Int, CaptureFailure] {
  try { let copied = [item for item in relay(failing(other))]; copied.len() }
}
""",
    status: 2,
    stderr: ["[check.type-mismatch]", "expected CaptureFailure, found Error"],
  )?
}
