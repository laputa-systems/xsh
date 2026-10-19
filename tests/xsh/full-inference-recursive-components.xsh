test test_full_inference_self_recursive_scc_generalizes_after_checking { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure descend(value, count) {
  if count == 0 { value } else { descend(value, count - 1) }
}
let integer = descend(42, 2)
let text = descend("demo", 3)
assert integer == 42
assert text == "demo"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_mutual_recursive_scc_is_declaration_order_independent { |ctx|
  let forward = test.run_xsh(
    ctx,
    r"""pure left(value, count) {
  if count == 0 { value } else { right(value, count - 1) }
}
pure right(value, count) {
  if count == 0 { value } else { left(value, count - 1) }
}
let integer = left(17, 4)
let text = right("mutual", 3)
assert integer == 17
assert text == "mutual"
print $integer
print $text
""",
  )?
  let reverse = test.run_xsh(
    ctx,
    r"""pure right(value, count) {
  if count == 0 { value } else { left(value, count - 1) }
}
pure left(value, count) {
  if count == 0 { value } else { right(value, count - 1) }
}
let integer = left(17, 4)
let text = right("mutual", 3)
assert integer == 17
assert text == "mutual"
print $integer
print $text
""",
  )?
  assert forward.success, forward.stderr
  assert reverse.success, reverse.stderr
  assert forward.stdout == "17\nmutual\n"
  assert reverse.stdout == forward.stdout
}
