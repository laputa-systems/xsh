test test_constructor_application_preserves_closed_callable_boundaries { |ctx|
  let file = test.temp_file(ctx, name: "constructor-definition.xsh", contents: bytes.from_text(r"""type Box[T] = {value: T, items: List[T] = []}
type Marker[T] = {name: Str}
pure make(value: UInt) -> Box[UInt] { Box(value: value) }
pure forwarded(value: UInt) -> Box[UInt] { make(value) }
pure text(value: Str) -> Box[Str] { Box(value: value) }
pure marker() -> Marker[UInt] { Marker(name: "kept") }
let count = forwarded(7)
let word = text("word")
"""))?
  let checked = run.capture --text "xsht" check $file ?
  assert checked.status.exited_with(0), checked.stderr
  checked.stdout == ""
}

test test_constructor_application_preserves_partial_spread_context_and_owned_defaults { |ctx|
  let executed = test.run_script(ctx, r"""type Pair[T] = {left: T, right: T, items: List[T] = []}
pure pair(left: UInt, right: UInt) -> Pair[UInt] {
  Pair(...{left: left}, right: right)
}
let value = pair(7, 8)
assert value.left == 7, "left spread field"
assert value.right == 8, "explicit right field"
assert value.items.len() == 0, "declaration default"
print paired
""")?
  assert executed.success, executed.stderr
  executed.stdout == "paired\n"
}
