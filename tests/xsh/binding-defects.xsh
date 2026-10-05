# Three rules about conditional binding and narrowing that the checker or the
# runtime once got wrong.

error LookupError = Bad

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

# The block of a `guard let` must leave the enclosing continuation whatever
# the subject is: after it the name is bound, and a failed Result binds none.
test test_result_guard_let_block_must_leave { |ctx|
  let output = test.run_script(
    ctx,
    """error ParseError = Empty

pure parse(text: Str) -> Result[Int, ParseError] {
  return Err(.Empty()) when text == ""
  Ok(text.byte_len())
}

proc show(text: Str) [io] {
  guard let size = parse(text) else {
    print "failed"
  }

  print \$size
}

proc name(text: Str) [io] {
  guard let size = parse(text) else { |failure|
    print \${failure.message}
  }

  print \$size
}

proc leave(text: Str) [io] {
  guard let size = parse(text) else {
    print "failed"
    return
  }

  print \$size
}

show("")
name("")
leave("")
""",
  )?
  assert output.status == 2, output.stderr
  assert output.stdout == ""
  assert count(output.stderr, "err[check.guard-fallthrough]") == 2, output.stderr
  assert count(output.stderr, "err[") == 2, output.stderr
  assert ":9:" in output.stderr, output.stderr
  assert ":17:" in output.stderr, output.stderr
}

pure lookup(key: Str) -> Result[Int, LookupError]? {
  return null when key == "none"
  let bad: Result[Int, LookupError] = Err(.Bad("bad key"))
  return bad when key == "bad"
  let good: Result[Int, LookupError] = Ok(key.byte_len())
  good
}

pure describe(key: Str) -> Str {
  let found = lookup(key)
  guard let outcome = found else {
    return "absent"
  }

  match outcome {
    Ok(size) => f"found {size}"
    Err(failure) => f"failed: {failure.message}"
  }
}

# A `Result[T]?` is a value: `null`, `Ok`, or `Err`. A plain `let` binds it,
# and an `Err` inside it fails nothing until the program asks.
test test_optional_result_is_bound_as_a_value {
  assert describe("none") == "absent"
  assert describe("abc") == "found 3"
  assert describe("bad") == "failed: bad key"

  let again = lookup("bad")
  assert again != null
}

pure shout(name: Str?, loud: Bool) -> Str {
  if name == null {
    "none"
  } else if loud {
    name.upper()
  } else {
    name.lower()
  }
}

pure pick(first: Str?, second: Str?) -> Str {
  if first == null {
    "no first"
  } else if second == null {
    first
  } else if first == second {
    first + second.upper()
  } else {
    first + second
  }
}

# The branches after `x == null` see `x` as present, through every `else if`
# and in the final `else`.
test test_null_test_narrows_every_later_branch {
  assert shout(null, true) == "none"
  assert shout("Ab", true) == "AB"
  assert shout("Ab", false) == "ab"
  assert pick(null, "b") == "no first"
  assert pick("a", null) == "a"
  assert pick("a", "a") == "aA"
  assert pick("a", "b") == "ab"
}

# A branch whose own condition tested nothing about `x` learns nothing more.
test test_else_if_does_not_narrow_what_it_did_not_test { |ctx|
  let output = test.run_script(
    ctx,
    """pure shout(name: Str?, other: Str?) -> Str {
  if other == null {
    "none"
  } else if name == null {
    other.upper()
  } else {
    name.upper() + other
  }
}

pure broken(name: Str?, loud: Bool) -> Str {
  if loud {
    "loud"
  } else {
    name.upper()
  }
}

print shout("a", "b") broken("a", false)
""",
  )?
  assert output.status == 2, output.stderr
  assert count(output.stderr, "err[check.optional-method]") == 1, output.stderr
  assert ":15:" in output.stderr, output.stderr
}
