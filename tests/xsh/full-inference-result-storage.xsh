test test_fresh_result_storage_uses_the_replacement_error_domain { |ctx|
  let good = test.expect(ctx, r"""error Failure = Missing
pure same(left, right) { var selected = left; selected = right; selected }
pure relay(value) { same(Ok(9), value) }
let typed: Result[Int, Failure] = Err(Failure.Missing())
let direct = same(Ok(9), typed)
let named = same(right: typed, left: Ok(9))
let nested = same(same(Ok(9), Ok(9)), typed)
let forwarded = relay(typed)
let verified: List[Result[Int, Failure]] = [direct, named, nested, forwarded]
assert verified == [typed, typed, typed, typed]
""", status: 0)?
  assert good.stderr == "", good.stderr
  test.expect(ctx, r"""error Failure = Missing
pure same(left, right) { var selected = left; selected = right; selected }
let incompatible: Result[Int, Failure] = same(Ok(9), Ok(9))
""", status: 2, stderr: ["[check.type-mismatch]"])?
}

test test_fresh_result_storage_uses_the_replacement_success_domain { |ctx|
  let good = test.expect(ctx, r"""pure same(left, right) { var selected = left; selected = right; selected }
pure relay(value) { same(Err("failure"), value) }
pure guarded(text: Str?, value: Result[Int, Str]) {
  if text != null { same(Err("failure"), value) } else { value }
}
pure guarded_success(text: Str?) {
  if text != null { same(Ok(1), Ok(2)) } else { same(Ok(3), Ok(4)) }
}
let typed: Result[Int, Str] = Ok(7)
let direct = same(Err("failure"), typed)
let named = same(right: typed, left: Err("failure"))
let nested = same(same(Err("first"), Err("second")), typed)
let forwarded = relay(typed)
let verified: List[Result[Int, Str]] = [direct, named, nested, forwarded]
assert verified == [typed, typed, typed, typed]
assert guarded("present", typed) == typed
assert guarded(null, typed) == typed
assert guarded_success("present") == Ok(2)
assert guarded_success(null) == Ok(4)
""", status: 0)?
  assert good.stderr == "", good.stderr
  test.expect(ctx, r"""pure same(left, right) { var selected = left; selected = right; selected }
let fixed: Result[Int, Str] = Err("fixed")
let other: Result[Str, Str] = Ok("text")
let incompatible = same(fixed, other)
""", status: 2, stderr: ["[check.type-mismatch]"])?
}
