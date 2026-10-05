error OrderError = Conflict(path: Path, owner: Str) | Pair(second: Str, first: Str) | Triple(zulu: Int, mike: Int, alpha: Int)

pure pair_fields(error: Error) -> Str {
  match Err(error) {
    Err(OrderError.Pair {second, first}) => f"second={second} first={first}"
    Err(OrderError.Triple {zulu, mike, alpha}) => f"zulu={zulu} mike={mike} alpha={alpha}"
    Err(OrderError.Conflict {path: file, owner}) => f"path={file.display()} owner={owner}"
    _ => "other"
  }
}

test test_positional_error_arguments_fill_fields_in_declaration_order {
  assert pair_fields(OrderError.Conflict(p"x", "o")) == "path=x owner=o"
  assert pair_fields(OrderError.Pair("s", "f")) == "second=s first=f"
  assert pair_fields(OrderError.Triple(1, 2, 3)) == "zulu=1 mike=2 alpha=3"
  assert pair_fields(OrderError.Triple(1, alpha: 3, mike: 2)) == "zulu=1 mike=2 alpha=3"
}

test test_error_equality_ignores_argument_order {
  assert OrderError.Pair("s", "f") == OrderError.Pair(first: "f", second: "s")
  assert OrderError.Pair("s", "f") == OrderError.Pair(second: "s", first: "f")
  assert OrderError.Pair("s", "f") != OrderError.Pair("f", "s")
}

test test_target_typed_error_arguments_fill_fields_in_declaration_order {
  let inferred: OrderError = .Pair("s", "f")
  assert pair_fields(inferred) == "second=s first=f"
  assert inferred == OrderError.Pair("s", "f")
}

test test_positional_error_arguments_evaluate_in_source_order { |ctx|
  let executed = test.run_script(
    ctx,
    r"""error E = Pair(second: Int, first: Int)
proc marked(value: Int) -> Int {
  print $value
  return value
}
match Err(E.Pair(marked(1), marked(2))) {
  Err(E.Pair {second, first}) => print f"second={second} first={first}"
  _ => print "other"
}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """1
2
second=1 first=2
"""
}

test test_positional_error_arguments_are_checked_against_declaration_order { |ctx|
  let swapped = test.run_script(
    ctx,
    """error E = Conflict(path: Path, owner: Str)
let conflict = E.Conflict("o", p"x")
""",
  )?
  assert ! swapped.success, swapped.stderr
  assert "check.type-mismatch" in swapped.stderr
  assert "expected Path, found Str" in swapped.stderr

  let extra = test.run_script(
    ctx,
    """error E = Conflict(path: Path, owner: Str)
let conflict = E.Conflict(p"x", "o", "extra")
""",
  )?
  assert ! extra.success, extra.stderr
  assert "too many error constructor arguments" in extra.stderr
}
