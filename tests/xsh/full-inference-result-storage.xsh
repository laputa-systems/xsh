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
