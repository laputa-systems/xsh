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
  match failure {
    Err(error) => test.ok("base (ctx: inner) (ctx: outer)" in error.message)?
    Ok(_) => test.fail("expected contextual failure")?
  }
  match original {
    Err(error) => test.eq(error.message, "base")?
    Ok(_) => test.fail("expected original failure")?
  }
  match ctx_data_result() {
    Err(error) => test.eq(error.message, "untouched")?
    Ok(_) => test.fail("expected direct error data")?
  }
  let data = ctx "stored data" { error.fail("stored") }
  match data {
    Err(error) => test.eq(error.message, "stored")?
    Ok(_) => test.fail("expected stored error data")?
  }
}

test test_ctx_value_and_named_ctx_bindings_remain_ordinary [error] { |ctx|
  test.ok(ctx.xsh_bin.display().count_chars() > 0)?
  let value = ctx "compute" { 42 }
  test.eq(value, 42)?
  if true {
    let ctx = {message: "ordinary"}
    test.eq(ctx.message, "ordinary")?
  }
  let handled = ctx "handled" {
    "local".parse_int() ?? { |_| 7 }
  }
  test.eq(handled, 7)?
}

test test_ctx_label_evaluates_once_and_failed_label_uses_only_enclosing_context [error] { |ctx|
  let output = test.run_script(ctx, r"""proc label() -> Str { print "label"; "operation" }
ctx label() { print "body" }
ctx "enclosing" { ctx f"${"not an integer".parse_int()?}" { print "skipped" } }
""")?
  test.ok(! output.success, output.stderr)?
  test.eq(output.stdout, "label\nbody\n")?
  test.ok("ctx: enclosing" in output.stderr)?
  test.ok(("ctx: operation" not in output.stderr))?
}

test test_ctx_finishes_defers_before_contextualizing_primary_or_cleanup_failure [error] { |ctx|
  let output = test.run_script(ctx, r"""ctx "region" {
  defer { print "cleanup" }
  error.fail("primary")?
}
""")?
  test.ok(! output.success, output.stderr)?
  test.eq(output.stdout, "cleanup\n")?
  test.ok("primary (ctx: region)" in output.stderr)?
  let cleanup = test.run_script(ctx, r"""ctx "cleanup region" {
  defer { print "last" }
  defer { error.fail("cleanup failed")? }
  print "body"
}
""")?
  test.ok(! cleanup.success, cleanup.stderr)?
  test.eq(cleanup.stdout, "body\nlast\n")?
  test.ok("cleanup failed (ctx: cleanup region)" in cleanup.stderr)?
}

test test_ctx_does_not_convert_abort_into_an_error [error] { |ctx|
  let output = test.run_script(ctx, r"""ctx "abort region" {
  defer { print "cleanup" }
  abort(17)
}
""")?
  test.eq(output.status, 17)?
  test.eq(output.stdout, "cleanup\n")?
  test.ok(("ctx: abort region" not in output.stderr))?
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "1\ninner\nouter\ndone\n")?
  let failed = test.run_script(ctx, r"""stream values() [error] -> Stream[Int] {
  ctx "producer" { yield 1; error.fail("late")? }
}
let values = values() |> collect
""")?
  test.ok(! failed.success, failed.stderr)?
  test.ok("late (ctx: producer)" in failed.stderr)?
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "cleanup:1\ncleanup:2\nvalue cleanup\n0 7\n")?
}

error CtxFailure = Failed(message: Str, code: Int) : InvalidData
pure ctx(value: Int) -> Int { value + 1 }

test test_ctx_preserves_nominal_payloads_and_callable_ctx_names [error] {
  test.eq(ctx(4), 5)?
  let original: Result[Unit, CtxFailure] = Err(CtxFailure.Failed(message: "base", code: 7))
  let contextual = retry [] { ctx "nominal" { original? } }
  test.error_kind(contextual, "CtxFailure.Failed")?
  match contextual {
    Err(CtxFailure.Failed {message, code}) => {
      test.eq(message, "base")?
      test.eq(code, 7)?
    }
    _ => test.fail("expected nominal payload")?
  }
}

test test_ctx_rejects_non_string_labels_and_keeps_statement_boolean_assertions [error] { |ctx|
  let wrong = test.run_script(ctx, "ctx 7 { print \"skipped\" }\n")?
  test.ok(! wrong.success, wrong.stderr)?
  test.ok("check.type" in wrong.stderr)?
  let assertion = test.run_script(ctx, "ctx \"assertion\" { false }\n")?
  test.ok(! assertion.success, assertion.stderr)?
  test.ok("AssertionError" in assertion.stderr)?
  test.ok("ctx: assertion" in assertion.stderr)?
  let value = ctx "predicate data" { false }
  test.eq(value, false)?
}

proc ctx_function_tail() [] -> Int { ctx "tail" { 7 } }
pure ctx_inferred_tail() { ctx "inferred" { 9 } }
test test_ctx_function_tail_and_inference_keep_consumed_values [error] {
  let inferred: Int = ctx_inferred_tail()
  test.eq(inferred, 9)?
  test.eq(ctx_function_tail(), 7)?
}

test test_ctx_secondary_cleanup_diagnostics_keep_enclosing_region [error] { |ctx|
  let output = test.run_script(ctx, r"""ctx "outer" { ctx "inner" {
  defer { error.fail("cleanup")? }
  error.fail("primary")?
} }
""")?
  test.ok(! output.success, output.stderr)?
  test.ok("primary (ctx: inner) (ctx: outer)" in output.stderr)?
  test.ok("cleanup (ctx: inner) (ctx: outer)" in output.stderr)?
}

test test_ctx_callee_cleanup_failures_keep_callers_regions [error] { |ctx|
  let output = test.run_script(ctx, r"""proc fail() [error] -> Result[Unit] {
  defer { error.fail("cleanup")? }
  return error.fail("primary")?
}
ctx "outer" { ctx "inner" { fail()? } }
""")?
  test.ok(! output.success, output.stderr)?
  test.ok("primary (ctx: inner) (ctx: outer)" in output.stderr)?
  test.ok("cleanup (ctx: inner) (ctx: outer)" in output.stderr)?
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
  test.ok(! output.success, output.stderr)?
  test.ok("primary (ctx: inner) (ctx: outer)" in output.stderr)?
  test.ok("cleanup (ctx: inner) (ctx: outer)" in output.stderr)?
}
