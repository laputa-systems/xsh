error PureCompletionProblem = Rejected

type PureCompletionRow = {value: Int}

pure completion_integer(failed: Bool) {
  return Err(PureCompletionProblem.Rejected()) when failed
  7
}

pure completion_word(failed: Bool) {
  return "kept" unless failed
  Err(PureCompletionProblem.Rejected())
}

pure completion_row(failed: Bool) {
  return Err(PureCompletionProblem.Rejected()) when failed
  {value: 7}
}

pure completion_integer_typed(failed: Bool) -> Result[Int, PureCompletionProblem] {
  return Err(.Rejected()) when failed
  7
}

test inferred_pure_results_wrap_success_and_preserve_failures {
  let integer: Result[Int, PureCompletionProblem] = completion_integer(false)
  let word: Result[Str, PureCompletionProblem] = completion_word(false)
  let row: Result[PureCompletionRow, PureCompletionProblem] = completion_row(false)
  assert integer == Ok(7)
  assert word == Ok("kept")
  assert row == Ok({value: 7})
  assert integer == completion_integer_typed(false)
  assert completion_integer(true) is Err(_)
  assert completion_word(true) is Err(_)
  assert completion_row(true) is Err(_)
}
