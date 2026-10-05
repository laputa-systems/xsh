# Declared `stream` producers, driven through fixture programs that leave a
# marker file for each row they run and for each `defer` they close with. The
# markers are what tells a body that ran from one that did not.

# The declared `stream` contract at its boundaries: the call does not run the
# body, a pull runs it to the next `yield`, a consumer that stops early leaves
# the later rows unexecuted, and every way a producer can end runs its `defer`
# exactly once.
test test_stream_producers_are_lazy_and_stop_where_the_consumer_stops { |ctx|
  let source = p"tests/fixtures/runtime/lazy-stream-producers.xsh".read_text()?
  let dir = fp"{test.temp_dir(ctx, name: "lazy-stream")?}/markers"
  let output = test.expect(
    ctx,
    source,
    status: 0,
    env: {XSH_LAZY_STREAM_DIR: dir, XSH_LAZY_STREAM_EXPECT_FAILURE: "0"},
  )?
  assert output.stdout == [
    # The wrapper read its text at the call; the body had not run.
    "body started at call=false",
    # The rows come from the text the call retained, not from the file.
    "rows=3 values=3",
    # An early stop ran one row and the defer; the later rows never ran.
    "first=3 rows=1 closed=true",
    # A body that never started has no defers to run.
    "never started closed=false other=3",
    # Returning out of the consuming loop stops the producer there.
    "return stopped=3 closed=true",
    # `take` keeps its items and stops; `break` stops on its item.
    "taken=2 rows=2 closed=true",
    "break item=3",
    "break rows=1 closed=true",
    # The malformed third row is not reached by the early stop.
    "checked first=3 rows=1",
    # Unreadable text fails at the call, before any row is interpreted.
    "missing input rejected",
    "",
  ].join("\n"), output.stdout
  assert ! fp"{dir}/closed-dropped".exists()?

  # Consuming past the malformed row fails the consumer with the failure the
  # row declared, and the producer's defer still ran exactly once.
  let failed = test.run_script(ctx, source, [], {XSH_LAZY_STREAM_DIR: dir, XSH_LAZY_STREAM_EXPECT_FAILURE: "1"})?
  assert ! failed.success, failed.stdout
  assert "RowError.Malformed" in failed.stderr, failed.stderr
  assert fp"{dir}/closed-failing".exists()?
}

# The frame engine resolves a call with no arguments without walking an
# argument list, so it has to make the same producer decision the argument
# path makes. When it did not, the body ran with nowhere for `yield` to report
# and the call failed as a function that did not return.
test test_zero_argument_stream_producers_run_from_every_call_position { |ctx|
  let source = p"tests/fixtures/runtime/zero-argument-stream.xsh".read_text()?
  let log = fp"{test.temp_dir(ctx, name: "zero-argument-stream")?}/producer.log"
  let output = test.expect(ctx, source, status: 0, env: {XSH_ZERO_ARGUMENT_STREAM_LOG: log})?
  # Direct `for`, a binding, a call from inside a proc's loop, a call from
  # inside another producer, two pipeline stages, and a bounded terminal that
  # stops the producer early — which still runs its `defer`.
  assert output.stdout == "direct=3\nbound=2\ntotal=3\ndoubled=2\nmapped=2\ntaken=1\nfirst=1\nstopped=1\nclosed\n", output.stdout
  assert output.stderr == "", output.stderr
}

# The worker stages are consumers too: `par-map` and the fused
# `par-map | flat-map | reduce-by` path pull a producer's rows through the same
# machinery as any other consumer, so the body runs at consumption, the stage
# maps the rows the body yielded, a mid-stream failure is the failure the row
# declared, and the producer's `defer` runs exactly once on every path.
test test_worker_stages_consume_a_producer_through_the_producer_machinery { |ctx|
  let source = p"tests/fixtures/runtime/worker-stage-producers.xsh".read_text()?
  let dir = fp"{test.temp_dir(ctx, name: "worker-stage")?}/markers"
  let output = test.expect(
    ctx,
    source,
    status: 0,
    env: {XSH_WORKER_STAGE_DIR: dir, XSH_WORKER_STAGE_EXPECT_FAILURE: "0"},
  )?
  assert output.stdout == [
    # The stage mapped every row the producer yielded, and the defer ran.
    "mapped=3 first=6 rows=3 closed=true",
    # A bounded terminal after the stage: the stage consumed the producer,
    # and it was still stopped exactly once.
    "taken=6 rows=3 closed=true",
    # The fused worker path: three rows of 3, 3, and 5 bytes.
    "fused=11 rows=3 closed=true",
    "",
  ].join("\n"), output.stdout

  # Consuming past the malformed row fails the stage with the failure the row
  # declared; the producer stopped at that row, and its defer ran once.
  let failed = test.run_script(ctx, source, [], {XSH_WORKER_STAGE_DIR: dir, XSH_WORKER_STAGE_EXPECT_FAILURE: "1"})?
  assert ! failed.success, failed.stdout
  assert "RowError.Bad" in failed.stderr, failed.stderr
  assert fp"{dir}/closed-check".exists()?
  assert ! fp"{dir}/row-check-three".exists()?
}

test test_stream_producers_check_yield_and_return_contracts { |ctx|
  test.expect(
    ctx,
    r"""
stream nums() -> Stream[Int] {
  for n in range(3) {
    yield n
  }
  return
}

let total = nums() |> sum
""",
    status: 0,
  )?

  for {source, code} in [
    {source: "yield 1\n", code: "check.yield"},
    {source: "stream bad() -> Stream[Int] {\n  return 1\n}\n", code: "check.stream-return"},
    {source: "stream bad() -> Stream[Int] {\n  yield \"no\"\n}\n", code: "check.type-mismatch"},
    {source: "stream bad() -> Stream[Int] {\n  yield range(3)\n}\n", code: "check.yield-stream"},
  ] {
    test.expect(ctx, source, status: 2, stderr: [f"[{code}]"])?
  }
}
