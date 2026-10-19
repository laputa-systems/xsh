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
