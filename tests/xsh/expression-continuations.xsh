test test_projection_native_arguments_keep_order_cleanup_and_trace_boundaries { |ctx|
  let source = r"""
type Leaf = {value: Int}
proc rows(fail: Bool) [error] -> Result[List[Leaf]] {
  defer { print "base cleanup" }
  print "base"
  if fail { error.fail("base failed")? }
  Ok([{value: 7}])
}
proc position() [] -> Int { print "index"; 0 }
proc expected() [] -> Int { print "expected"; 7 }
test.eq(rows(false)?[position()].value, expected())?
let captured: Result[Unit] = try {
  defer { print "outer cleanup" }
  test.eq(rows(true)?[position()].value, expected())?
}
print (captured is Err(_))
test.eq(right: expected(), left: rows(false)?[position()].value)?
"""
  let plain = test.run_script(ctx, source)?
  assert plain.success, plain.stderr
  assert plain.stdout == """base
base cleanup
index
expected
base
base cleanup
outer cleanup
true
expected
base
base cleanup
index
"""
  let traced = test.run_xsht_trace(ctx, source, ["--trace", "--raw"])?
  assert traced.success, traced.stderr
  assert traced.stdout == plain.stdout
  let calls = [line for line in traced.stderr.lines() if "kind=module.call" in line and "test.eq" in line]
  let results = [line for line in traced.stderr.lines() if "kind=module.result" in line and "test.eq" in line]
  assert calls.len() == 2, traced.stderr
  assert results.len() == 2, traced.stderr
}

pure continuation_sign(positive: Bool) -> Int {
  return 1 when positive
  -1
}

pure continuation_root(scratch: Bool) -> Path {
  return /tmp/scratch when scratch
  /tmp/continuation
}

test test_leading_operators_continue_only_when_they_cannot_start_a_statement {
  assert continuation_sign(false) == -1
  assert continuation_root(false) == /tmp/continuation
  let total = 1
    + 2
    * 3
  assert total == 7
  let missing: Str? = null
  let label = missing
    # a comment line does not end the expression
    ?? "fallback"
  assert label == "fallback"
  let bounded = total > 0
    and total < 10
  assert bounded
  let trimmed = " x "
    .trim()
  assert trimmed == "x"
}
