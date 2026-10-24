test test_generic_null_arguments_preserve_exact_closed_shapes { |ctx|
  let output = test.run_xsh(ctx, r"""
pure keep(value) { value }
pure forward(value) { keep(value) }
assert keep(null) == null
assert keep([null]) == [null]
assert keep([[null]]) == [[null]]
assert keep({item: null}).item == null
assert forward([{item: [null]}])[0].item == [null]
let shared = keep
let contextual: List[Str?] = shared([null])
assert contextual.len() == 1
assert contextual[0] == null
let mixed: List[Int?] = [null, 7]
assert keep(mixed)[1] == 7
let opaque: Any = keep(null)
""")?
  assert output.success, output.stderr
  assert output.stdout == ""
}

test test_generic_null_contexts_do_not_retype_existing_list_domains { |ctx|
  test.expect(ctx, r"""
pure keep(value) { value }
let values: List[Null] = [null]
let result: List[Str?] = keep(values)
""", status: 2, stderr: ["[check.type-mismatch]"])?
}
