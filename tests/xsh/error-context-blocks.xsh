pure ctx_data_result() -> Result[Unit] {
  ctx "data" { return error.fail("untouched") }
}

test test_ctx_propagation_attaches_inner_to_outer_and_preserves_error_data [error] {
  let original = error.fail("base")
  let failure = retry [] {
    ctx "outer" {
      ctx "inner" { original? }
    }
  }
  if let Err(error) = failure {
    "base (ctx: inner) (ctx: outer)" in error.message
  } else {
    test.fail("expected contextual failure")?
  }
  if let Err(error) = original {
    error.message == "base"
  } else {
    test.fail("expected original failure")?
  }
  if let Err(error) = ctx_data_result() {
    error.message == "untouched"
  } else {
    test.fail("expected direct error data")?
  }
  let data = ctx "stored data" { error.fail("stored") }
  if let Err(error) = data {
    error.message == "stored"
  } else {
    test.fail("expected stored error data")?
  }
}

test test_ctx_value_and_named_ctx_bindings_remain_ordinary [error] { |harness|
  harness.xsh_bin.display().count_chars() > 0
  let value = ctx "compute" { 42 }
  value == 42
  {
    let ctx = {message: "ordinary"}
    ctx.message == "ordinary"
  }
  let handled = ctx "handled" {
    "local".parse_int() ?? { |_| 7 }
  }
  handled == 7
}

test test_ctx_label_evaluates_once_and_failed_label_uses_only_enclosing_context [error] { |ctx|
  let output = test.run_script(ctx, r"""proc label() -> Str { print "label"; "operation" }
ctx label() { print "body" }
ctx "enclosing" { ctx f"${"not an integer".parse_int()?}" { print "skipped" } }
""")?
  assert ! output.success, output.stderr
  output.stdout == "label\nbody\n"
  "ctx: enclosing" in output.stderr
  "ctx: operation" not in output.stderr
}

test test_ctx_finishes_defers_before_contextualizing_primary_or_cleanup_failure [error] { |ctx|
  let output = test.run_script(ctx, r"""ctx "region" {
  defer { print "cleanup" }
  error.fail("primary")?
}
""")?
  assert ! output.success, output.stderr
  output.stdout == "cleanup\n"
  "primary (ctx: region)" in output.stderr
  let cleanup = test.run_script(ctx, r"""ctx "cleanup region" {
  defer { print "last" }
  defer { error.fail("cleanup failed")? }
  print "body"
}
""")?
  assert ! cleanup.success, cleanup.stderr
  cleanup.stdout == "body\nlast\n"
  "cleanup failed (ctx: cleanup region)" in cleanup.stderr
}

test test_ctx_does_not_convert_abort_into_an_error [error] { |ctx|
  let output = test.run_script(ctx, r"""ctx "abort region" {
  defer { print "cleanup" }
  abort(17)
}
""")?
  output.status == 17
  output.stdout == "cleanup\n"
  "ctx: abort region" not in output.stderr
}

test test_ctx_stream_suspension_retains_region_and_runs_cleanup_on_early_exit [error] { |ctx|
  let output = test.run_script(ctx, r"""stream values() [error] -> Stream[Int] {
  defer { print "outer" }
  ctx "producer" {
    defer { print "inner" }
    yield 1
    yield 2
    error.fail("late")?
  }
}
for value in values() { print $value; break }
print "done"
""")?
  assert output.success, output.stderr
  output.stdout == "1\ninner\nouter\ndone\n"
  let failed = test.run_script(ctx, r"""stream values() [error] -> Stream[Int] {
  ctx "producer" { yield 1; error.fail("late")? }
}
let values = values() |> collect
""")?
  assert ! failed.success, failed.stderr
  "late (ctx: producer)" in failed.stderr
}

test test_ctx_keeps_loop_transfers_and_value_evaluation_before_defers [error] { |ctx|
  let output = test.run_script(ctx, r"""var count = 0
for item in [1, 2, 3] {
  ctx "loop" {
    defer { print f"cleanup:$item" }
    continue when item == 1
    break when item == 2
    count += item
  }
}
let value = ctx "value" { defer { print "value cleanup" }; 7 }
print $count $value
""")?
  assert output.success, output.stderr
  output.stdout == "cleanup:1\ncleanup:2\nvalue cleanup\n0 7\n"
}

error CtxFailure = Failed(message: Str, code: Int) : InvalidData
test test_ctx_preserves_nominal_payloads_and_callable_ctx_names [error] { |harness|
  let called = test.run_script(harness, r"""pure ctx(value: Int) -> Int { value + 1 }
print ${ctx(4)}
""")?
  assert called.success, called.stderr
  called.stdout == "5\n"
  let original: Result[Unit, CtxFailure] = Err(CtxFailure.Failed(message: "base", code: 7))
  let contextual = retry [] { ctx "nominal" { original? } }
  test.error_kind(contextual, "CtxFailure.Failed")?
  if let Err(CtxFailure.Failed {message, code}) = contextual {
    message == "base"
    code == 7
  } else {
    test.fail("expected nominal payload")?
  }
}

test test_ctx_rejects_non_string_labels_and_keeps_statement_boolean_assertions [error] { |ctx|
  let wrong = test.run_script(ctx, "ctx 7 { print \"skipped\" }\n")?
  assert ! wrong.success, wrong.stderr
  "check.type" in wrong.stderr
  let assertion = test.run_script(ctx, "ctx \"assertion\" { false }\n")?
  assert ! assertion.success, assertion.stderr
  "AssertionError" in assertion.stderr
  "ctx: assertion" in assertion.stderr
  let value = ctx "predicate data" { false }
  !value
}

proc ctx_function_tail() [] -> Int { ctx "tail" { 7 } }
pure ctx_inferred_tail() { ctx "inferred" { 9 } }
test test_ctx_function_tail_and_inference_keep_consumed_values [error] {
  let inferred = ctx_inferred_tail()
  inferred == 9
  ctx_function_tail() == 7
}

test test_ctx_secondary_cleanup_diagnostics_keep_enclosing_region [error] { |ctx|
  let output = test.run_script(ctx, r"""ctx "outer" { ctx "inner" {
  defer { error.fail("cleanup")? }
  error.fail("primary")?
} }
""")?
  assert ! output.success, output.stderr
  "primary (ctx: inner) (ctx: outer)" in output.stderr
  "cleanup (ctx: inner) (ctx: outer)" in output.stderr
}

test test_ctx_callee_cleanup_failures_keep_callers_regions [error] { |ctx|
  let output = test.run_script(ctx, r"""proc fail() [error] -> Result[Unit] {
  defer { error.fail("cleanup")? }
  return error.fail("primary")?
}
ctx "outer" { ctx "inner" { fail()? } }
""")?
  assert ! output.success, output.stderr
  "primary (ctx: inner) (ctx: outer)" in output.stderr
  "cleanup (ctx: inner) (ctx: outer)" in output.stderr
}

test test_ctx_suspended_call_cleanup_keeps_regions [error] { |ctx|
  let output = test.run_script(ctx, r"""proc fail() [error] -> Result[Unit] {
  defer { error.fail("cleanup")? }
  return error.fail("primary")?
}
stream rows() [error] -> Stream[Int] {
  ctx "outer" { ctx "inner" { yield 1; fail()? } }
}
for item in rows() { print $item }
""")?
  assert ! output.success, output.stderr
  "primary (ctx: inner) (ctx: outer)" in output.stderr
  "cleanup (ctx: inner) (ctx: outer)" in output.stderr
}
