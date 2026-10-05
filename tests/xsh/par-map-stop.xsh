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

proc failing_stage(marker: Str, root: Path) [fs, process, time, error] -> Result[Int] {
  [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
    defer fp"{root}/cleanup-{n}".write("x")
    if n == 2 {
      # The other items have started their children by now.
      time.sleep(300ms)
      fail "item failed"
    }

    # A stopped item that handles the failure of its child cannot go on.
    let stopped = try { run sleep $marker }
    fp"{root}/went-on-{n}".write(f"{stopped is Ok(_)}")
    n
  } |> count()
}

# An item that fails stops the stage: no further item starts, and the items
# still running are stopped the way a deadline stops them, so the stage does
# not wait out their children. The stage reports the failure of the item that
# failed, not of an earlier item it stopped, and every item runs its cleanup.
test test_par_map_item_failure_stops_running_items { |ctx|
  let root = test.temp_dir(ctx, name: "par-map-failure")?
  let marker = "311.0404"
  let started = time.now()
  let result = failing_stage(marker, root)
  match result {
    Ok(_) => assert false
    Err(failure) => assert "item failed" in failure.message, failure.message
  }

  assert time.now() - started < 20000
  assert survivors(marker)? == 0
  for n in [1, 2, 3, 4] {
    assert fp"{root}/cleanup-{n}".exists()?, f"item {n} ran no cleanup"
    assert ! fp"{root}/went-on-{n}".exists()?, f"item {n} went on"
  }
}

proc failed_leaf() [error] -> Result[Int] {
  fail "item failed"
}

proc failing_by(way: Str, marker: Str) [process, time, error] -> Result[Int] {
  [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
    if n == 3 {
      time.sleep(300ms)
      match way {
        "question" => assert failed_leaf()? == 0
        "run" => run sh -c "exit 7"
        else => assert false, "item failed"
      }
    }

    run sleep $marker
    n
  } |> count()
}

# Every way an item fails stops the stage the same way: a `?`, a failed
# command, and a failed assertion.
test test_par_map_every_item_failure_stops_running_items {
  for {way, marker, expected} in [
    {way: "question", marker: "311.0901", expected: "item failed"},
    {way: "run", marker: "311.0902", expected: "7"},
    {way: "assert", marker: "311.0903", expected: "item failed"},
  ] {
    let started = time.now()
    let result = try { failing_by(way, marker)? }
    match result {
      Ok(_) => assert false, way
      Err(failure) => assert expected in failure.message, f"{way}: {failure.message}"
    }

    assert time.now() - started < 20000, way
    assert survivors(marker)? == 0, way
  }
}

proc failing_reduce(marker: Str) [process, time, error] -> Result[Map[Str, Int]] {
  [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
    if n == 3 {
      time.sleep(300ms)
      fail "item failed"
    }

    run sleep $marker
    n
  } |> reduce-by(sum: true) { |n| {key: "all", value: n} }
}

# `par-map` followed by `reduce-by` runs as one stage that reduces on the
# workers. It gives the value the two stages give apart, with or without an
# identity `flat-map` between them.
test test_par_map_reduce_stage_reduces_on_the_workers {
  let squares = range(1, 9) |> par-map(jobs: 4) { |n| n * n } |> reduce-by(sum: true) { |n|
    {key: if n % 2 == 0 { "even" } else { "odd" }, value: n}
  }
  assert squares["even"] == 120
  assert squares["odd"] == 84
  assert squares.len() == 2

  let serial = range(1, 9) |> par-map(jobs: 1) { |n| n * n } |> reduce-by(sum: true) { |n|
    {key: if n % 2 == 0 { "even" } else { "odd" }, value: n}
  }
  assert serial == squares

  let flattened = range(1, 9) |> par-map(jobs: 4) { |n| [n, n] } |> flat-map { |row| row } |> reduce-by(max: true) { |n|
    {key: "largest", value: n}
  }
  assert flattened["largest"] == 8
}

# The one stage stops as `par-map` does: an item's failure stops the items
# still running and is the stage's failure, and a deadline stops them all.
test test_par_map_reduce_stage_stops_its_workers {
  let marker = "311.0707"
  let started = time.now()
  let result = failing_reduce(marker)
  match result {
    Ok(_) => assert false
    Err(failure) => assert "item failed" in failure.message, failure.message
  }

  assert time.now() - started < 20000
  assert survivors(marker)? == 0

  let late = "311.0808"
  let timed = within 200ms {
    [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
      run sleep $late
      n
    } |> reduce-by(sum: true) { |n| {key: "all", value: n} }
  }
  match timed {
    Ok(_) => assert false
    Err(failure) => assert failure is Timeout
  }

  assert survivors(late)? == 0
}

proc first_large(marker: Str) [process, error] -> Result[Int] {
  let mapped = [1, 2, 3, 4] |> par-map(jobs: 4) { |n|
    return 100 + n when n == 3
    run sleep $marker
    n
  }
  mapped.len()
}

# `return` from inside an item leaves the enclosing function, and is not a
# failure: the items already running finish first, so of the items that ran
# the earliest one that left decides the stage.
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
