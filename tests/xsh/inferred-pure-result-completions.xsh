error PureCompletionProblem = Rejected

type PureCompletionRow = {value: Int}

pure completion_integer(failed: Bool) {
  if failed { return Err(PureCompletionProblem.Rejected()) }
  7
}

pure completion_word(failed: Bool) {
  if ! failed { return "kept" }
  Err(PureCompletionProblem.Rejected())
}

pure completion_row(failed: Bool) {
  if failed { return Err(PureCompletionProblem.Rejected()) }
  {value: 7}
}

pure completion_integer_typed(failed: Bool) -> Result[Int, PureCompletionProblem] {
  if failed { return Err(PureCompletionProblem.Rejected()) }
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
  assert match completion_integer(true) { Err(_) => true; Ok(_) => false }
  assert match completion_word(true) { Err(_) => true; Ok(_) => false }
  assert match completion_row(true) { Err(_) => true; Ok(_) => false }
}
