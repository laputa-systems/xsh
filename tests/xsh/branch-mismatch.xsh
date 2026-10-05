# A branch of an `if`, a `match`, or a block is checked against what is
# expected of the whole expression, so a branch of the wrong type is the
# mistake: it is reported once, at the branch, and not again for the
# expression that contains it.

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  output.stderr
}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

test test_a_branch_of_the_wrong_type_is_reported_once_at_the_branch { |ctx|
  let cases = [
    {
      source: "pure label(n: Int) -> Str {\n  if n > 1 { n } else { \"x\" }\n}\n",
      at: ":2:14\n",
    },
    {
      source: "proc label(n: Int) -> Str {\n  if n > 1 { \"x\" } else { n }\n}\n",
      at: ":2:27\n",
    },
    {
      source: "pure label(n: Int) -> Str {\n  match n {\n    1 => n\n    else => \"x\"\n  }\n}\n",
      at: ":3:10\n",
    },
    {
      source: "pure label(n: Int) -> Str {\n  let text: Str = if n > 1 { n } else { \"x\" }\n  text\n}\n",
      at: ":2:30\n",
    },
    {
      source: "pure label(n: Int) -> Str {\n  let text: Str = match n {\n    1 => \"x\"\n    else => n\n  }\n  text\n}\n",
      at: ":4:13\n",
    },
    {
      source: "pure label(n: Int) -> Str {\n  if n > 1 {\n    if n > 2 { n } else { \"y\" }\n  } else {\n    \"x\"\n  }\n}\n",
      at: ":3:16\n",
    },
  ]
  for case in cases {
    let stderr = check_errors(ctx, case.source)?
    assert count(stderr, "err[") == 1, stderr
    assert "err[check.type-mismatch]" in stderr and "expected Str, found Int" in stderr, stderr
    assert case.at in stderr, stderr
  }
}

# Two branches of the wrong type are two mistakes, and an argument of the
# wrong type inside an argument of the wrong type is two as well: neither
# call is a branch of the other.
test test_separate_mismatches_are_each_reported { |ctx|
  let branches = check_errors(ctx, "pure label(n: Int) -> Str {\n  if n > 1 { n } else { n + 1 }\n}\n")?
  assert count(branches, "err[check.type-mismatch]") == 2, branches
  assert ":2:14\n" in branches and ":2:25\n" in branches, branches

  let nested = check_errors(
    ctx,
    "pure inner(text: Str) -> Int {\n  text.len()\n}\n\npure outer(text: Str) -> Int {\n  text.len()\n}\n\nlet n = 1\nlet total = outer(inner(n))\nprint $total\n",
  )?
  assert count(nested, "err[check.type-mismatch]") == 2, nested
  assert "expected Str, found Int" in nested, nested
}
