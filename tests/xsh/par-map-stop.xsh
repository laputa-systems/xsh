# However a `par-map` stage ends, its workers have stopped and the child
# processes they started are gone when it returns. Each test gives its
# children a sleep length no other process uses and looks for it afterwards.

proc survivors(marker: Str) [process, error] -> Result[Int] {
  process.list()? |> where { |row| marker in row.argv } |> count()
}

# The stage is given up at a deadline: the workers stop their children
# instead of running them to the end.
test test_par_map_under_a_deadline_leaves_no_child_running {
  let marker = "311.0101"
  let started = time.now()
  let result = within 200ms {
    [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
      run sleep $marker
      n
    } |> count()
  }
  match result {
    Ok(_) => assert false
    Err(failure) => assert failure is Timeout
  }

  assert time.now() - started < 10000
  assert survivors(marker)? == 0
}

# A test the harness cancels at its time limit is the same early exit, taken
# from outside the stage. The workers report nothing and must not panic.
test test_par_map_in_a_cancelled_test_stops_its_workers_and_children { |ctx|
  let root = test.temp_dir(ctx, name: "par-map-cancel")?
  let marker = "311.0202"
  fp"{root}/tests".mkdir()
  fp"{root}/xsht-config.ini".write("test_roots = tests\n")
  fp"{root}/tests/cancelled.xsh".write(f"""test test_slow_stage {{ |ctx|
  test.timeout(ctx, 300ms)
  let done = [1, 2, 3, 4] |> par-map(jobs: 4) {{ |n|
    run sleep {marker}
    n
  }} |> count()
  assert done == 4
}}
""")
  let started = time.now()
  var outcome = run.capture --text "true" ?
  cd root {
    outcome = run.capture --text --accept=[0, 1, 2, 3, 101] "xsht" test "tests/cancelled.xsh" ?
  }

  let report = outcome.stdout + outcome.stderr
  assert ! outcome.status.exited_with(0), report
  assert "1 failed" in report, report
  assert "panicked" not in report, report
  assert "receiver dropped" not in report, report
  assert time.now() - started < 20000
  assert survivors(marker)? == 0
}

# A signal that ends the script ends the stage: every worker's child gets it.
test test_par_map_in_a_signalled_script_leaves_no_child_running { |ctx|
  let marker = "311.0303"
  let started = time.now()
  let output = test.run_script(
    ctx,
    f"""let done = [1, 2, 3, 4] |> par-map(jobs: 4) {{ |n|
  if n == 1 {{
    time.sleep(200ms)
    process.kill(process.current_pid()?, signal: "TERM")
  }}
  run sleep {marker}
  n
}} |> count()
print f"finished {{done}}"
""",
  )?
  assert output.status != 0
  assert "finished" not in output.stdout
  assert "panicked" not in output.stderr, output.stderr
  assert time.now() - started < 20000
  assert survivors(marker)? == 0
}

proc failing_stage(marker: Str) [process, time, error] -> Result[Int] {
  [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
    if n == 2 {
      fail "item failed"
    }

    run sleep $marker
    n
  } |> count()
}

# An item that fails stops the stage from starting more items. Items that
# are already running finish first, so nothing outlives the stage.
test test_par_map_item_failure_waits_for_running_items {
  let marker = "0.311404"
  let result = failing_stage(marker)
  match result {
    Ok(_) => assert false
    Err(failure) => assert "item failed" in failure.message
  }

  assert survivors(marker)? == 0
}

proc first_large(marker: Str) [process, error] -> Result[Int] {
  let mapped = [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
    return 100 + n when n == 3
    run sleep $marker
    n
  }
  mapped.len()
}

# `return` from inside an item leaves the enclosing function the same way.
test test_par_map_early_return_waits_for_running_items {
  let marker = "0.311505"
  assert first_large(marker)? == 103
  assert survivors(marker)? == 0
}

# The stage is eager: it maps every item before anything downstream runs, so
# a later stage that stops early or a `break` around the pipeline finds no
# worker still running.
test test_par_map_runs_to_its_end_before_a_downstream_stage_stops {
  let marker = "0.311606"
  var seen = []
  for round in [1, 2] {
    let firsts = [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
      run sleep $marker
      n * round
    } |> take(1) |> collect()
    seen += firsts
    break when round == 1
  }

  assert seen == [1]
  assert survivors(marker)? == 0
}
