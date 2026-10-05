error StepError {
    Failed(message: Str)
}

proc fails() {
  error.fail("boom")
}

# Runs `body` after the shared helpers and returns what it printed.
proc trace(ctx: TestContext, body: Str) [fs, process, error] -> Result[Str] {
  let output = test.run_script(
    ctx,
    """error StepError {
  Failed(message: Str)
}

proc fails() {
  error.fail("boom")
}

""" + body,
  )?
  assert output.success, output.stderr
  output.stdout
}

test test_errdefer_runs_only_when_the_function_fails { |ctx|
  let printed = trace(
    ctx,
    """proc step(fail: Bool) -> Result[Int] {
  errdefer { print f"undo {fail}" }
  if fail { fails()? }
  1
}

let good = step(false)
let bad = step(true)
print "end"
""",
  )?
  assert printed == "undo true\nend\n"
}

test test_errdefer_shares_one_order_with_defer { |ctx|
  let printed = trace(
    ctx,
    """proc step(fail: Bool) -> Result[Int] {
  defer { print "defer 1" }
  errdefer { print "errdefer 1" }
  defer { print "defer 2" }
  errdefer { print "errdefer 2" }
  if fail { fails()? }
  1
}

let bad = step(true)
print "--"
let good = step(false)
""",
  )?
  assert printed == "errdefer 2\ndefer 2\nerrdefer 1\ndefer 1\n--\ndefer 2\ndefer 1\n"
}

test test_errdefer_runs_for_every_way_a_failure_leaves { |ctx|
  let printed = trace(
    ctx,
    """proc by_statement() {
  errdefer { print "statement-position Result[Unit]" }
  fails()
}

proc by_unit_tail() -> Result[Unit] {
  errdefer { print "Result[Unit] tail" }
  fails()
}

proc by_assert() {
  errdefer { print "assert" }
  assert 1 == 2
}

proc by_run() {
  errdefer { print "run" }
  run false
}

let a = try { by_statement() }
let b = by_unit_tail()
let c = try { by_assert() }
let d = try { by_run() }
""",
  )?
  assert printed == "statement-position Result[Unit]\nResult[Unit] tail\nassert\nrun\n"
}

# A runtime failure is not captured by `try`, and still leaves with an error.
test test_errdefer_runs_for_a_runtime_failure { |ctx|
  let output = test.run_script(
    ctx,
    """proc pick(items: List[Int]) -> Int {
  errdefer { print "errdefer" }
  items[3]
}

let caught = try { pick([1]) }
print "never"
""",
  )?
  assert output.status != 0
  assert output.stdout == "errdefer\n"
  assert "index-out-of-range" in output.stderr
}

test test_errdefer_runs_when_the_function_returns_an_err { |ctx|
  let printed = trace(
    ctx,
    """proc by_return(fail: Bool) -> Result[Int, StepError] {
  errdefer { print "function" }
  {
    errdefer { print "block" }
    return Err(StepError.Failed(message: "returned")) when fail
  }
  return 1
}

proc by_tail(fail: Bool) -> Result[Int, StepError] {
  if fail {
    errdefer { print "tail" }
    Err(StepError.Failed(message: "tail"))
  } else {
    errdefer { print "never" }
    1
  }
}

let a = by_return(true)
let b = by_return(false)
let c = by_tail(true)
let d = by_tail(false)
""",
  )?
  assert printed == "block\nfunction\ntail\n"
}

test test_errdefer_does_not_run_for_break_continue_or_a_block_value { |ctx|
  let printed = trace(
    ctx,
    """for index in [1, 2, 3] {
  errdefer { print "loop" }
  continue when index == 1
  break when index == 2
}

let data: Result[Int, StepError] = {
  errdefer { print "value block" }
  Err(StepError.Failed(message: "data"))
}
print "end"
""",
  )?
  assert printed == "end\n"
}

test test_errdefer_under_try_and_retry { |ctx|
  let printed = trace(
    ctx,
    """proc captured() -> Int {
  errdefer { print "outside try" }
  let caught = try {
    errdefer { print "inside try" }
    fails()?
    1
  }
  caught ?? {|_| 0 }
}

print captured()
var attempts = 0
let settled = retry [1ms, 1ms] {
  attempts += 1
  errdefer { print f"attempt {attempts}" }
  if attempts < 3 { fails()? }
  attempts
}
print \${settled?}
""",
  )?
  assert printed == "inside try\n0\nattempt 1\nattempt 2\n3\n"
}

test test_errdefer_runs_once_a_later_cleanup_fails { |ctx|
  let printed = trace(
    ctx,
    """proc step() -> Result[Int] {
  defer { print "first registered" }
  errdefer { print "errdefer" }
  defer fails()
  1
}

match try { step()? } {
  Ok(_) => print "ok"
  Err(failure) => print f"failed: {failure.message}"
}
""",
  )?
  assert printed == "errdefer\nfirst registered\nfailed: boom\n"
}

test test_a_failing_errdefer_leaves_the_original_failure_primary { |ctx|
  let output = test.run_script(
    ctx,
    """error StepError {
  Failed(message: Str)
}

proc fails() {
  error.fail("undo failed")
}

proc step() -> Result[Int, StepError] {
  errdefer { print "still runs" }
  errdefer fails()
  return Err(StepError.Failed(message: "primary"))
}

match step() {
  Ok(_) => print "ok"
  Err(failure) => print f"failed: {failure.message}"
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "still runs\nfailed: primary\n"
  assert "cleanup error" in output.stderr
  assert "undo failed" in output.stderr
}

test test_errdefer_in_a_stream_producer_stopped_early { |ctx|
  let printed = trace(
    ctx,
    """stream numbers() -> Stream[Int] {
  errdefer { print "errdefer" }
  defer { print "defer" }
  yield 1
  yield 2
}

let firsts = numbers() |> take(1) |> collect()
print firsts.len()
""",
  )?
  assert printed == "defer\n1\n"
}

test test_errdefer_at_the_top_level_follows_the_script { |ctx|
  let passing = test.run_script(
    ctx,
    """errdefer { print "errdefer" }
defer { print "defer" }
print "body"
""",
  )?
  assert passing.success, passing.stderr
  assert passing.stdout == "body\ndefer\n"

  let failing = test.run_script(
    ctx,
    """errdefer { print "errdefer" }
defer { print "defer" }
print "body"
error.fail("script failed")?
errdefer { print "never registered" }
""",
  )?
  assert failing.status == 3
  assert failing.stdout == "body\ndefer\nerrdefer\n"

  let exiting = test.run_script(
    ctx,
    """errdefer { print "errdefer" }
exit 4
""",
  )?
  assert exiting.status == 4
  assert exiting.stdout == "errdefer\n"
}

test test_errdefer_removes_a_partial_file_only_on_failure { |ctx|
  let root = test.temp_dir(ctx, name: "errdefer-partial")?
  let kept = fp"{root}/kept"
  let dropped = fp"{root}/dropped"
  assert write_then(kept, fail: false) == Ok(1)
  assert kept.read_text()? == "partial"
  let failed = write_then(dropped, fail: true)
  assert failed != Ok(1)
  assert ! dropped.exists()?
}

proc write_then(target: Path, fail: Bool) -> Result[Int] {
  target.write("partial")
  errdefer fs.remove(target, missing_ok: true)
  if fail { fails() }
  1
}

test test_errdefer_obeys_the_defer_checks { |ctx|
  let escaping = test.run_script(
    ctx,
    """proc step() -> Int {
  errdefer { return 1 }
  2
}
""",
  )?
  assert escaping.status != 0
  assert "check.defer-control-flow" in escaping.stderr

  let in_pure = test.run_script(
    ctx,
    """pure step() -> Int {
  errdefer { print "x" }
  2
}
""",
  )?
  assert in_pure.status != 0
  assert "check.pure-defer" in in_pure.stderr

  let reserved = test.run_script(ctx, "let errdefer = 1\n")?
  assert reserved.status != 0
}

test test_errdefer_formats_as_written { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "errdefer.xsh",
    contents: bytes.from_text("""proc step(target: Path) -> Result[Unit] {
  errdefer   fs.remove( target,missing_ok:true )
  errdefer{ print "undo" }
  defer{ print "done" }
  target.write("x")
}
"""),
  )?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == """proc step(target: Path) -> Result[Unit] {
  errdefer fs.remove(target, missing_ok: true)
  errdefer { print "undo" }
  defer { print "done" }
  target.write("x")
}
"""
}
