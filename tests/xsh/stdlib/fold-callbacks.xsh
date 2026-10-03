test test_fold_and_reduce_reject_callback_result_lifting { |ctx|
  for stage in ["fold", "reduce"] {
    let source = "let total = [1, 2] |> " + stage + "(0) { |acc, item| Ok(acc + item) }\n"
    let output = test.run_script(ctx, source)?
    {
      let assertion_condition = !output.success
      let assertion_message = stage
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = "check.type-mismatch" in output.stderr
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_fold_and_reduce_allow_explicit_callback_propagation { |ctx|
  for stage in ["fold", "reduce"] {
    let source = "let total = [1, 2] |> " + stage + "(0) { |acc, item| Ok(acc + item)? }\nprint $total\n"
    let output = test.run_script(ctx, source)?
    {
      let {success: assertion_condition, stderr: assertion_message, ..} = output
      assert assertion_condition, assertion_message
    }
    assert (output.stdout) == ("3\n")
  }
}

test test_fold_and_reduce_preserve_result_accumulators_as_data { |ctx|
  for stage in ["fold", "reduce"] {
    let source = r"""error CombineError = Stop(item: Int)
let initial: Result[Int, CombineError] = Ok(0)
let outcome = [1, 2, 3] |> """ + stage + r"""(initial) { |acc, item|
  match acc {
    Ok(value) => if item == 2 { Err(CombineError.Stop(item)) } else { Ok(value + item) }
    Err(error) => Err(error)
  }
}
match outcome {
  Err(CombineError.Stop {item: 2}) => print "retained"
  _ => print "unexpected"
}
"""
    let output = test.run_script(ctx, source)?
    {
      let {success: assertion_condition, stderr: assertion_message, ..} = output
      assert assertion_condition, assertion_message
    }
    assert (output.stdout) == ("retained\n")
  }
}

test test_fold_explicit_callback_error_closes_scopes_before_next_pull { |ctx|
  let output = test.run_script(ctx, r"""error CombineError = Stop(item: Int)
pure combine(acc: Int, item: Int) -> Result[Int, CombineError] {
  if item == 2 { Err(CombineError.Stop(item)) } else { acc + item }
}
stream numbers() [io] -> Stream[Int] {
  defer { print "source closed" }
  for item in [1, 2, 3] {
    print f"pull ${item}"
    yield item
  }
}
let outcome = try {
  numbers() |> fold(0) { |acc, item|
    defer { print f"callback closed ${item}" }
    combine(acc, item)?
  }
}
match outcome {
  Err(CombineError.Stop {item: 2}) => print "caught"
  _ => print "unexpected"
}
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert (output.stdout) == ("pull 1\ncallback closed 1\npull 2\ncallback closed 2\nsource closed\ncaught\n")
}

test test_reduce_by_requires_explicit_record_callback_propagation { |ctx|
  let rejected = test.run_script(ctx, r"""let totals = [1, 2] |> reduce-by(sum: true) { |item|
  Ok({key: "total", value: item})
}
""")?
  {
    let assertion_condition = !rejected.success
    let assertion_message = rejected.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "check.type-mismatch" in rejected.stderr
    let assertion_message = rejected.stderr
    assert assertion_condition, assertion_message
  }
  let accepted = test.run_script(ctx, r"""let totals = [1, 2] |> reduce-by(sum: true) { |item|
  Ok({key: "total", value: item})?
}
print ${totals.get("total")?}
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = accepted
    assert assertion_condition, assertion_message
  }
  assert (accepted.stdout) == ("3\n")
}
