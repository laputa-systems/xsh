test test_projection_native_arguments_keep_order_cleanup_and_trace_boundaries [error] { |ctx|
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
  test.ok(plain.success, plain.stderr)?
  test.eq(plain.stdout, "base\nbase cleanup\nindex\nexpected\nbase\nbase cleanup\nouter cleanup\ntrue\nexpected\nbase\nbase cleanup\nindex\n")?
  let traced = test.run_xsht_trace(ctx, source, ["--trace", "--raw"])?
  test.ok(traced.success, traced.stderr)?
  test.eq(traced.stdout, plain.stdout)?
  let calls = [line for line in traced.stderr.lines() if "kind=module.call" in line and "test.eq" in line]
  let results = [line for line in traced.stderr.lines() if "kind=module.result" in line and "test.eq" in line]
  test.eq(calls.len(), 2, traced.stderr)?
  test.eq(results.len(), 2, traced.stderr)?
}
