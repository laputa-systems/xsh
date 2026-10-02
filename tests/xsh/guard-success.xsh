pure guard_success_value(result: Result[Int, Str], fallback: Int) -> Int {
  let kept = fallback - fallback
  {
    guard let kept = result else { |_failure| return kept }
    { let kept = fallback; let _ = kept }
    kept
  }
}

error GuardFailure = Missing(message: Str)

pure guard_failure_message(result: Result[Int]) -> Str {
  guard let kept = result else { |failure| return failure.message }
  "${kept}"
}

test test_guard_failure_binding_carries_the_original_error_root [error] {
  guard_failure_message(Ok(7)) == "7"
  guard_failure_message(Err(GuardFailure.Missing("missing"))) == "missing"
}

test test_guard_success_binding_publishes_only_the_success_continuation [error] {
  guard_success_value(Ok(7), 8) == 7
  guard_success_value(Err("missing"), 8) == 0
}

test test_guard_success_binding_does_not_escape_into_sibling_scopes [error] { |ctx|
  let output = test.run_script(ctx, r"""
pure outside(result: Result[Int, Str]) -> Int {
  { guard let kept = result else { |_failure| return 0 }; let _ = kept }
  { kept }
}
print ${outside(Ok(7))}
""")?
  assert !output.success, "success binding escaped its lexical block"
  assert "kept" in output.stderr, output.stderr
}

test test_guard_success_binding_is_immutable [error] { |ctx|
  let output = test.run_script(ctx, r"""
pure changed(result: Result[Int, Str]) -> Int {
  guard let kept = result else { |_failure| return 0 }
  kept = 9
  kept
}
print ${changed(Ok(7))}
""")?
  assert !output.success, "success binding accepted a write"
  assert "immutable" in output.stderr, output.stderr
}

test test_guard_success_binding_is_unavailable_in_its_initializer [error] { |ctx|
  let output = test.run_script(ctx, r"""
pure self_bound() -> Int {
  guard let kept = Ok(kept) else { |_failure| return 0 }
  kept
}
print ${self_bound()}
""")?
  assert !output.success, "success binding was visible in its initializer"
  assert "kept" in output.stderr, output.stderr
}

test test_guard_success_binding_is_unavailable_in_its_failure_handler [error] { |ctx|
  let output = test.run_script(ctx, r"""
pure failed(result: Result[Int, Str]) -> Int {
  guard let kept = result else { |_failure| return kept }
  kept
}
print ${failed(Err("missing"))}
""")?
  assert !output.success, "success binding was visible in its failure handler"
  assert "kept" in output.stderr, output.stderr
}
