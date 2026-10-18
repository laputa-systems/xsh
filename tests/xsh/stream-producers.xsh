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

test test_process_stream_is_stopped_before_its_scope_defers { |ctx|
  let pidfile = test.temp_path(ctx, name: "owned-process-stream-pid")
  {
    let owned = run.stream --text --accept=[0] sh -c "echo $$ > $1; exec sleep 30" sh $pidfile
    repeat 100 times {
      break when pidfile.exists()
      time.sleep(10ms)
    }
    assert pidfile.exists()?, "the process stream must start its child before the owning scope exits"
    let pid = pidfile.read_text()?.trim() as Int
    defer {
      let kept = owned
      assert process.kill(pid, signal: "0") is Err(_), "the stream process must be reaped before the owning scope's defer"
    }
  }
}

test test_default_process_byte_stream_yields_before_child_completion { |ctx|
  let acknowledged = test.temp_path(ctx, name: "default-process-byte-stream-ack")
  let rows = run.stream --bytes --timeout=2s sh -c "printf '\\001\\002'; while [ ! -e \"$1\" ]; do sleep 0.01; done; printf '\\003'" sh $acknowledged ?
  var chunks = []
  for chunk in rows {
    chunks += [chunk]
    if chunks.len() == 1 {
      acknowledged.write("received")?
    }
  }
  assert bytes.concat(chunks) == b"\x01\x02\x03"
}

test test_discarded_process_streams_remain_owned_until_scope_exit { |ctx|
  let first_pidfile = test.temp_path(ctx, name: "discarded-process-stream-first-pid")
  let second_pidfile = test.temp_path(ctx, name: "discarded-process-stream-second-pid")
  var pids = []
  var quiet_before_defers = false
  defer {
    for pid in pids {
      if process.kill(pid, signal: "0") is Ok(_) {
        let _ = process.kill(pid, signal: "KILL")
      }
    }
  }
  {
    var current = null
    current = run.stream --text --accept=[0] sh -c "echo $$ > $1; exec sleep 30" sh $first_pidfile
    repeat 100 times {
      break when first_pidfile.exists()
      time.sleep(10ms)
    }
    assert first_pidfile.exists()?, "the first stream process must start"
    let first_pid = first_pidfile.read_text()?.trim() as Int
    pids += [first_pid]
    current = null
    let first_survived_discard = process.kill(first_pid, signal: "0") is Ok(_)

    current = run.stream --text --accept=[0] sh -c "echo $$ > $1; exec sleep 30" sh $second_pidfile
    repeat 100 times {
      break when second_pidfile.exists()
      time.sleep(10ms)
    }
    assert second_pidfile.exists()?, "the replacement stream process must start"
    let second_pid = second_pidfile.read_text()?.trim() as Int
    pids += [second_pid]
    current = null
    let both_survived_reassignment = process.kill(first_pid, signal: "0") is Ok(_) and process.kill(second_pid, signal: "0") is Ok(_)
    defer {
      quiet_before_defers = pids.len() == 2 and
        process.kill(first_pid, signal: "0") is Err(_) and
        process.kill(second_pid, signal: "0") is Err(_)
    }
    assert first_survived_discard, "discarding a stream does not release its scope-owned activity"
    assert both_survived_reassignment, "reassigning a stream does not release either scope-owned activity"
  }
  assert quiet_before_defers, "scope-owned streams stop before the scope's defer"
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

test test_stream_producer_mutations_refresh_consumer_captures { |ctx|
  let expected = [
    "pipeline:1:1",
    "method:2:2",
    "for:3:3",
    "comprehension:4:4",
    "returns:1,2,3,4",
    "checks:4",
    "resumed:1:1",
    "resumed:12:12",
    "resumed-return:22",
    "delegated:1:1",
    "delegated:12:12",
    "delegated-return:22",
    "cancelled:1:1",
    "closed:11",
    "cancelled-after:111",
    "cancelled-return:111",
    "",
  ].join("\n")
  let output = test.expect(
    ctx,
    r"""var checks = 0
stream values() -> Stream[Int] {
  checks += 1
  yield checks
}
var resumed = 0
stream resumed_values() -> Stream[Int] {
  resumed += 1
  yield resumed
  resumed += 1
  yield resumed
}
var delegated = 0
stream delegated_values() -> Stream[Int] {
  delegated += 1
  yield delegated
  delegated += 1
  yield delegated
}
stream delegated_parent() -> Stream[Int] { yield @delegated_values() }
var cancelled = 0
stream cancelled_values() [io] -> Stream[Int] {
  defer {
    print f"closed:{cancelled}"
    cancelled += 100
  }
  cancelled += 1
  yield cancelled
}
proc observe_pipeline() -> Int {
  let rows = values() |> collect()
  print f"pipeline:{rows[0]}:{checks}"
  checks
}
proc observe_method() -> Int {
  let rows = values().collect()
  print f"method:{rows[0]}:{checks}"
  checks
}
proc observe_for() -> Int {
  for item in values() {
    print f"for:{item}:{checks}"
  }
  checks
}
proc observe_comprehension() -> Int {
  let rows = [item for item in values()]
  print f"comprehension:{rows[0]}:{checks}"
  checks
}
proc observe_resumed() -> Int {
  for item in resumed_values() {
    print f"resumed:{item}:{resumed}"
    resumed += 10
  }
  resumed
}
proc observe_delegated() -> Int {
  for item in delegated_parent() {
    print f"delegated:{item}:{delegated}"
    delegated += 10
  }
  delegated
}
proc observe_cancelled() [io] -> Int {
  for item in cancelled_values() {
    print f"cancelled:{item}:{cancelled}"
    cancelled += 10
    break
  }
  print f"cancelled-after:{cancelled}"
  cancelled
}
let results = [observe_pipeline(), observe_method(), observe_for(), observe_comprehension()]
print f"returns:{results[0]},{results[1]},{results[2]},{results[3]}"
print f"checks:{checks}"
print f"resumed-return:{observe_resumed()}"
print f"delegated-return:{observe_delegated()}"
print f"cancelled-return:{observe_cancelled()}"
""",
    status: 0,
    stdout: [expected],
  )?
  assert output.stdout == expected, output.stdout
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
