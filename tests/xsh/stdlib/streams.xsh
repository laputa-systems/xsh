proc test_stream_adapters_and_transform_stages() [fs, process, error] {
  let lines = """alpha
beta
""" |> text.lines

  test.eq(lines, ["alpha", "beta"])?
  let chunks = b"abcdef" |> bytes.chunks(2)
  test.ok(chunks[0] == b"ab")?
  test.ok(chunks[2] == b"ef")?

  let json_lines = """{"name":"alpha","size":2}
{"name":"beta","size":1}
"""
  |> json.lines
  |> sort-by .size

  test.eq(json_lines[0].name, "beta")?

  let json_stream = """{"ok":true}
{"ok":false}
""" |> json.stream

  test.eq(json_stream[1].ok, false)?

  test.eq(
    [3, 1, 2, 2]
      |> where . > 1
      |> sort
      |> unique-by .
      |> map { |n|
        n * 2
      },
    [4, 6],
  )?

  test.eq([1, 2, 3, 4] |> take(2), [1, 2])?
  test.eq([1, 2, 3, 4] |> drop(2), [3, 4])?
  test.eq([1, 2] |> repeat(2), [1, 2, 1, 2])?
  test.eq([0] |> range(1, 4), [1, 2, 3])?
  test.eq([0] |> range(4, 1), [4, 3, 2])?

  test.eq(
    ["ab", "c"]
      |> flat-map { |word|
        word.split("")
      },
    ["a", "b", "c"],
  )?

  test.eq(
    [1, 2, 3]
      |> fold(0) { |acc|
        acc + .
      },
    6,
  )?

  # Accumulator-plus-item form: the block binds the accumulator (typed by the
  # initial value) before the stream item, and the tail produces the accumulator.
  test.eq(
    [1, 2, 3]
      |> fold(0) { |acc, it|
        acc + it
      },
    6,
  )?

  test.eq(
    [1, 2, 3]
      |> reduce(10) { |acc, it|
        acc + it
      },
    16,
  )?

  # A postfix `?` inside a stream-stage closure, followed by a method call on
  # the unwrapped value, must compile and propagate normally instead of
  # tripping the compact indexed-IR `full_ir_function_blocker`. The blocker was
  # caused by the slot-based pipeline inference mis-typing a `first`/`last`/
  # `min`/`max` terminal as its input list, so the `.lower()` on the unwrapped
  # item was mistaken for a method on the list.
  let ext_lower = ["a.TXT", "b.com"]
    |> map { |s|
      (s.split(".") |> last())?.lower()
    }
    |> collect()
  test.eq(ext_lower, ["txt", "com"])?

  # A bare trailing `?` (no method tail) in a stage block is still accepted and
  # unwraps the terminal result inside the closure.
  let firsts = [["a"], ["b"], ["c"]]
    |> map { |row|
      row.get(0)?
    }
    |> collect()
  test.eq(firsts, ["a", "b", "c"])?

  # A bare accumulator-ident tail no longer trips the indexed IR builder; it
  # returns the running accumulator unchanged.
  test.eq(
    [1, 2, 3]
      |> fold(0) { |x|
        x
      },
    0,
  )?

  # Counting through fold without group-by: the accumulator is a Map and the
  # item a Str, which the two-parameter binding types correctly.
  let fold_counts = ["a", "b", "a", "c"]
    |> fold(map.empty()) { |acc, it|
      acc.set(it, acc.get(it, 0) + 1)
    }
  test.eq(fold_counts.get("a", 0), 2)?
  test.eq(fold_counts.get("b", 0), 1)?
  test.eq(fold_counts.get("c", 0), 1)?
  test.eq(fold_counts.len(), 3)?

  test.eq(
    [1, 2, 3]
      |> reduce(10) { |acc|
        acc + .
      },
    16,
  )?

  test.eq([1, 2, 3] |> sum, 6)?
  test.eq(([3, 1, 2] |> min)?, 1)?
  test.eq(([3, 1, 2] |> max)?, 3)?
  test.eq(([3, 1, 2] |> first())?, 3)?
  test.eq(([3, 1, 2] |> last())?, 2)?
  test.ok([1, 2, 3] |> any . == 2)?
  test.ok([1, 2, 3] |> all . > 0)?
  let expected_counts = map.empty().set("1", 2).set("2", 1)

  test.eq(
    ["a", "bb", "c"]
      |> count { |word|
        word.count_chars()
      },
    expected_counts,
  )?

  test.eq(
    [1, 2, 3]
      |> par-map { |value|
        value * 2
      },
    [2, 4, 6],
  )?

  test.eq([1, 2, 3, 4] |> batch --count=2, [[1, 2], [3, 4]])?
  let enumerated = ["x", "y"] |> enumerate()
  test.eq(enumerated[1].index, 1)?
  test.eq(enumerated[1].value, "y")?
  let zipped = ["left", "right"] |> zip([10, 20])
  test.eq(zipped[0].left, "left")?
  test.eq(zipped[1].right, 20)?

  let groups = [{kind: "a", value: 1}, {kind: "b", value: 2}, {kind: "a", value: 3}]
    |> group-by .kind
    |> sort-by .key

  test.eq(groups[0].key, "a")?
  test.eq(groups[0].items.len(), 2)?

  let scalar_groups = [3, 1, 2, 1]
    |> group-by { |value|
      value
    }
    |> sort-by { |bucket|
      bucket.key
    }
  test.eq(scalar_groups[0].key, 1)?
  test.eq(scalar_groups[1].key, 2)?
  test.eq(scalar_groups[2].key, 3)?

  let shuffled = [1, 2, 3, 4] |> shuffle(7)
  test.eq(shuffled.len(), 4)?
  test.eq(shuffled |> sort, [1, 2, 3, 4])?

  [1, 2]
    |> each { |value|
      test.ok(value > 0)?
    }

  test.eq(
    [1, 2]
      |> tee { |value|
        test.ok(value > 0)?
      }
      |> map { |value|
        value + 1
      },
    [2, 3],
  )?

  [{name: "small", size: 1}, {name: "large", size: 4}] |> table.print(columns: ["name", "size"])
}

proc test_fold_block_composes_pipeline_over_accumulator_field() [error] {
  let result = [0] |> fold({parts: ["first", "last"]}) { |acc, _item|
    let popped = acc.parts |> take(acc.parts.len() - 1) |> collect()
    {parts: popped}
  }
  test.eq(result.parts, ["first"])?
}

proc test_fold_block_supports_nested_if_statement_with_assignment() [error] {
  let result = [1, 2, 3] |> fold(0) { |acc, item|
    var next = acc
    if item > 1 {
      next = next + item
    }
    next
  }
  test.eq(result, 5)?
}

proc test_fold_block_supports_nested_if_as_branch_tail() [error] {
  let result = [1, 2, 3] |> fold(0) { |acc, item|
    if item == 1 {
      acc
    } else {
      if item == 2 {
        acc + 2
      } else {
        acc + 3
      }
    }
  }
  test.eq(result, 5)?
}

proc test_fold_and_reduce_run_direct_effects_in_item_order(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull ${n}"
    yield n
  }
}

proc after_item(n: Int) [io] {
  print f"defer ${n}"
}

proc main() [io, process, error] {
  let total = numbers() |> fold(0) { |acc, n|
    defer after_item(n)
    run true ?
    print f"fold ${n}"
    acc + n
  }
  print f"total=${total}"
  let reduced = [1, 2] |> reduce(0) { |acc, n|
    print f"reduce ${n}"
    acc + n
  }
  print f"reduced=${reduced}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\nfold 0\ndefer 0\npull 1\nfold 1\ndefer 1\npull 2\nfold 2\ndefer 2\ntotal=3\nreduce 1\nreduce 2\nreduced=3\n")?
}

proc test_fold_error_stops_and_closes_live_source(ctx: TestContext) [fs, error] {
  let pulled = test.temp_path(ctx, name: "fold-pulled")
  let closed = test.temp_path(ctx, name: "fold-closed")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(pulled: Path, closed: Path) [fs, error] -> Stream[Int] {
  defer closed.write("closed")?
  for n in range(5) {
    pulled.write(f"pull \${n}")?
    yield n
  }
}

proc main() [fs, error] {
  let total = numbers(Path("${pulled.display()}"), Path("${closed.display()}"))
    |> fold(0) { |acc, n| acc + 10 / (1 - n) }
  print \${total}
}
""",
  )?
  test.ok(! output.success, output.stdout)?
  test.eq(pulled.read_text()?, "pull 1")?
  test.eq(closed.read_text()?, "closed")?
}

proc test_sum_type_error_stops_and_closes_live_source(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
proc close() [io] { print "closed" }

stream numbers() [io] -> Stream[Any] {
  defer close()
  print "pull 1"
  yield 1
  print "pull bad"
  yield "bad"
  print "pull 3"
  yield 3
}

proc main() [io] {
  let total = numbers() |> sum()
  print f"total=${total}"
}
""",
  )?
  test.ok(! output.success, output.stdout)?
  test.contains(output.stderr, "sum expected Int stream")?
  test.eq(output.stdout, "pull 1\npull bad\nclosed\n")?
}

proc test_keyed_stages_errors_stop_live_source(ctx: TestContext) [error] {
  for terminal in ["group-by", "count", "unique-by"] {
    let source = f"""
proc close() [io] { print "closed" }

stream numbers() [io] -> Stream[Int] {
  defer close()
  for n in range(3) {
    print f"pull \${n}"
    yield n
  }
}

proc main() [io, error] {
  let _result = numbers() |> ${terminal} { |n| 1 / (1 - n) }
}
"""
    let output = test.run_script(ctx, source)?
    test.ok(! output.success, output.stdout)?
    test.eq(output.stdout, "pull 0\npull 1\nclosed\n")?
  }
}

proc test_keyed_stages_project_before_next_live_pull(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers(label: Str) [io] -> Stream[Int] {
  for n in [2, 1, 2] {
    print f"${label} pull ${n}"
    yield n
  }
}

proc key(n: Int) [io] -> Int {
  print f"key ${n}"
  return n
}

proc main() [io, error] {
  let groups = numbers("group") |> group-by { |n| key(n) }
  print f"groups=${groups[0].key}:${groups[0].items.len()},${groups[1].key}:${groups[1].items.len()}"
  let counts = numbers("count") |> count { |n| key(n) }
  print f"counts=${counts.get("1", 0)},${counts.get("2", 0)}"
  let unique = numbers("unique") |> unique-by { |n| key(n) }
  print f"unique=${unique[0]},${unique[1]}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "group pull 2\nkey 2\ngroup pull 1\nkey 1\ngroup pull 2\nkey 2\ngroups=2:2,1:1\ncount pull 2\nkey 2\ncount pull 1\nkey 1\ncount pull 2\nkey 2\ncounts=1,2\nunique pull 2\nkey 2\nunique pull 1\nkey 1\nunique pull 2\nkey 2\nunique=2,1\n")?
}

proc test_zip_evaluates_right_before_pulling_and_stops_at_shorter_side(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
proc close() [io] { print "closed" }

stream numbers() [io] -> Stream[Int] {
  defer close()
  for n in range(4) {
    print f"pull ${n}"
    yield n
  }
}

proc right() [io] -> List[Int] {
  print "right"
  return [10, 20]
}

proc main() [io, error] {
  let pairs = numbers() |> zip(right())
  print f"pairs=${pairs[0].left}:${pairs[0].right},${pairs[1].left}:${pairs[1].right}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "right\npull 0\npull 1\nclosed\npairs=0:10,1:20\n")?
}

proc test_zip_right_error_does_not_pull_left(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  print "pull"
  yield 1
}

proc right() [io, error] -> List[Int] {
  print "right"
  return ["bad".parse_int()?]
}

proc main() [io, error] {
  let pairs = numbers() |> zip(right())
  print "after"
}
""",
    ["--raw"],
  )?
  test.ok(! output.success)?
  test.eq(output.stdout, "right\n")?
  test.contains(output.stderr, "invalid")?
  test.contains(output.stderr, "kind=stream.stage.exit name=\"zip\"")?
}

proc test_zip_collects_right_stream_before_pulling_left(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream left() [io] -> Stream[Int] {
  for n in [1, 2, 3] {
    print f"left ${n}"
    yield n
  }
}

stream right() [io] -> Stream[Int] {
  for n in [10, 20] {
    print f"right ${n}"
    yield n
  }
}

proc main() [io, error] {
  let pairs = left() |> zip(right())
  print f"pairs=${pairs[0].left}:${pairs[0].right},${pairs[1].left}:${pairs[1].right}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "right 10\nright 20\nleft 1\nleft 2\npairs=1:10,2:20\n")?
}

proc test_zip_result_length_is_available_to_format_interpolation(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [] -> Stream[Int] { yield 1 }

proc main() [io, error] {
  let pairs = numbers() |> zip([10])
  print f"pairs=${pairs.len()}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pairs=1\n")?
}

proc test_last_min_max_live_terminals_finish_and_close_producers(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
proc close(name: Str) [io] { print f"closed ${name}" }

stream numbers(name: Str) [io] -> Stream[Int] {
  defer close(name)
  for n in [3, 1, 2] {
    print f"${name} ${n}"
    yield n
  }
}

proc main() [io, error] {
  let last = numbers("last") |> last()?
  print f"last=${last}"
  let min = numbers("min") |> min()?
  print f"min=${min}"
  let max = numbers("max") |> max()?
  print f"max=${max}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "last 3\nlast 1\nlast 2\nclosed last\nlast=2\nmin 3\nmin 1\nmin 2\nclosed min\nmin=1\nmax 3\nmax 1\nmax 2\nclosed max\nmax=3\n")?
}

proc test_terminal_each_as_final_proc_statement_returns_unit(ctx: TestContext) [error] {
  # A nested script keeps `each` as the final statement of its own procedure.
  # The checker and runtime must agree that the drained stage returns Unit.
  let output = test.run_script(
    ctx,
    """
proc main() [io] {
  ["one", "two", "three"]
    |> each { |word| print $word }
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, """one
two
three
""")?
  test.eq(output.stderr, "")?
}

proc test_each_live_source_runs_body_before_next_pull(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull ${n}"
    yield n
  }
}

proc main() [io] {
  numbers() |> each { |n| print f"each ${n}" }
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\neach 0\npull 1\neach 1\npull 2\neach 2\n")?
}

proc test_each_error_stops_and_closes_live_source(ctx: TestContext) [fs, error] {
  let pulled = test.temp_path(ctx, name: "each-pulled")
  let closed = test.temp_path(ctx, name: "each-closed")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(pulled: Path, closed: Path) [fs, error] -> Stream[Int] {
  defer closed.write("closed")?
  for n in range(5) {
    pulled.write(f"pull \${n}")?
    yield n
  }
}

proc main() [fs, error] {
  numbers(Path("${pulled.display()}"), Path("${closed.display()}"))
    |> each { |n| let _ = 10 / (1 - n) }
}
""",
  )?
  test.ok(! output.success, output.stdout)?
  test.eq(pulled.read_text()?, "pull 1")?
  test.eq(closed.read_text()?, "closed")?
}

proc test_if_else_is_a_stream_stage_tail_value() [error] {
  let mapped = [1, 2, 3]
    |> map { |n|
      if n % 2 == 0 {
        "even"
      } else {
        "odd"
      }
    }
  test.eq(mapped, ["odd", "even", "odd"])?

  let filtered = [1, 2, 3]
    |> where { |n|
      if n > 1 {
        true
      } else {
        false
      }
    }
  test.eq(filtered, [2, 3])?

  let _ = [1, 2]
    |> each { |n|
      if n > 1 {
        let _ = n
      } else {
        let _ = n
      }
    }
}

proc test_predicate_stage_blocks_bind_local_lets() [error] {
  # A multi-statement predicate block with a local let binding must compile
  # and behave identically to the single-expression form for where/any/all.
  let nums = [1, 2, 3, 4, 5, 6]

  let filtered_block = nums
    |> where { |n|
      let rem = n % 2
      rem == 0
    }
  let filtered_expr = nums
    |> where { |n|
      n % 2 == 0
    }
  test.eq(filtered_block, filtered_expr)?
  test.eq(filtered_block, [2, 4, 6])?

  let any_block = nums
    |> any { |n|
      let twice = n * 2
      twice > 8
    }
  let any_expr = nums
    |> any { |n|
      n * 2 > 8
    }
  test.eq(any_block, any_expr)?
  test.ok(any_block)?

  let all_block = nums
    |> all { |n|
      let rem = n % 2
      rem == 0
    }
  let all_expr = nums
    |> all { |n|
      n % 2 == 0
    }
  test.eq(all_block, all_expr)?
  test.ok(! all_block)?
  test.ok(
    [2, 4, 6]
      |> all { |n|
        let rem = n % 2
        rem == 0
      },
  )?
}

proc test_implicit_standard_read_helpers_and_pipe_shorthand(ctx: TestContext) [fs, error] {
  let file = test.temp_file(ctx, name: "pipe-shorthand-input", contents: b"ok\nwarn one\nwarn two\n")?
  let file_text = fs.read_text(file)?
  let piped = file.read_bytes()?.utf8()?

  let warnings = piped
    |> text.lines
    |> where "warn" in .

  let names = [{path: "b"}, {path: "a"}]
    |> map .path
    |> sort

  test.eq(file_text, piped)?
  test.eq(warnings[0], "warn one")?
  test.eq(warnings[1], "warn two")?
  test.eq(names, ["a", "b"])?
  test.ok(cpu.count() > 0)?
}

proc test_core_commands_and_byte_pipeline(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "core-byte-pipeline")?
  let output = fp"${root}/out.txt"

  cd root {
    fs.write(p"inside.txt", "cwd")?
  }

  test.eq(fp"${root}/inside.txt".read_text()?, "cwd")?
  eprint "covered stderr"
  run printf "%s" "abc" | run tr a-z A-Z > output ?
  test.eq(output.read_text()?, "ABC")?
}

proc test_reduce_by_stream_aggregates() [error] {
  # `reduce-by` keeps one accumulator per key without group-by materialization.
  let nums = [1, 2, 3, 4, 5, 6]

  let agg = nums
    |> reduce-by --sum { |n|
      {key: if n % 2 == 0 { "even" } else { "odd" }, value: {count: 1, total: n}}
    }

  let lo = nums
    |> reduce-by --min { |n|
      {key: "all", value: n}
    }

  let hi = nums
    |> reduce-by --max { |n|
      {key: "all", value: n}
    }

  test.eq(agg.get("even", {count: 0, total: 0}), {count: 3, total: 12})?
  test.eq(agg.get("odd", {count: 0, total: 0}), {count: 3, total: 9})?
  test.eq(lo.get("all", 0), 1)?
  test.eq(hi.get("all", 0), 6)?
}

proc test_reduce_by_live_source_folds_each_item_before_next_pull(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull ${n}"
    yield n
  }
}

proc observed(n: Int) [io] -> Int {
  print f"reduce ${n}"
  return n
}

proc main() [io, error] {
  let groups = numbers()
    |> reduce-by --sum --jobs=1 { |n|
      {key: "all", value: observed(n)}
    }
  print f"total=${groups.get("all", 0)}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\nreduce 0\npull 1\nreduce 1\npull 2\nreduce 2\ntotal=3\n")?
}

proc test_reduce_by_error_stops_and_closes_live_source(ctx: TestContext) [fs, error] {
  let pulled = test.temp_path(ctx, name: "reduce-by-pulled")
  let closed = test.temp_path(ctx, name: "reduce-by-closed")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(pulled: Path, closed: Path) [fs, error] -> Stream[Int] {
  defer closed.write("closed")?
  for n in range(5) {
    pulled.write(f"pull \${n}")?
    yield n
  }
}

proc main() [fs, error] {
  let groups = numbers(Path("${pulled.display()}"), Path("${closed.display()}"))
    |> reduce-by --sum { |n|
      {key: "all", value: 10 / (1 - n)}
    }
  print \${groups.get("all", 0)}
}
""",
  )?
  test.ok(! output.success, output.stdout)?
  test.eq(pulled.read_text()?, "pull 1")?
  test.eq(closed.read_text()?, "closed")?
}

proc test_reduce_by_jobs_hint_preserves_results() [error] {
  # The accepted `--jobs` hint currently uses the same serial reducer.
  let nums = [0] |> range(0, 50000)

  let serial = nums
    |> reduce-by --sum { |n|
      {key: if n % 3 == 0 { "a" } else if n % 3 == 1 { "b" } else { "c" }, value: {count: 1, total: n}}
    }

  let par = nums
    |> reduce-by --sum --jobs=8 { |n|
      {key: if n % 3 == 0 { "a" } else if n % 3 == 1 { "b" } else { "c" }, value: {count: 1, total: n}}
    }

  for k in serial.keys() {
    test.eq(par.get(k, {count: 0, total: 0}), serial.get(k, {count: 0, total: 0}))?
  }

  test.eq(par.keys().len(), 3)?

  test.eq(
    (nums
      |> reduce-by --min --jobs=8 { |n|
        {key: "all", value: n}
      }).get("all", -1),
    0,
  )?

  test.eq(
    (nums
      |> reduce-by --max --jobs=8 { |n|
        {key: "all", value: n}
      }).get("all", -1),
    49999,
  )?
}

proc test_jobs_options_evaluate_once_before_stage(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
proc jobs(label: Str) [io] -> Int {
  print $label
  return 2
}

proc main() [io, error] {
  let grouped = [1, 2]
    |> group-by --jobs=jobs("group") { |n| n % 2 }
  print f"groups=${grouped.len()}"
  let counted = [1, 2]
    |> count --jobs=jobs("count") { |n| n % 2 }
  print f"counts=${counted.keys().len()}"
  let total_count = [1, 2] |> count --jobs=jobs("total-count")
  print f"count=${total_count}"
  let reduced = [1, 2]
    |> reduce-by --sum --jobs=jobs("reduce") { |n| {key: "all", value: n} }
  print f"total=${reduced.get("all", 0)}"
  let from_workers = [1, 2]
    |> par-map --jobs=2 { |n| n }
    |> reduce-by --sum --jobs=jobs("after-map") { |n| {key: "all", value: n} }
  print f"worker-total=${from_workers.get("all", 0)}"
  [1, 2] |> each --jobs=jobs("each") { |n| let _ = n }
  print "done"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "group\ngroups=2\ncount\ncounts=2\ntotal-count\ncount=2\nreduce\ntotal=3\nafter-map\nworker-total=3\neach\ndone\n")?
}

proc test_jobs_option_rejects_dynamic_zero_before_pulling_source(ctx: TestContext) [fs, error] {
  let evaluated = test.temp_path(ctx, name: "jobs-evaluated")
  let pulled = test.temp_path(ctx, name: "jobs-source-pulled")
  let output = test.run_script(
    ctx,
    f"""
proc zero_jobs(evaluated: Path) [fs, error] -> Int {
  evaluated.write("evaluated")?
  return 0
}

stream numbers(pulled: Path) [fs, error] -> Stream[Int] {
  pulled.write("pulled")?
  yield 1
}

proc main() [fs, error] {
  let count = numbers(Path("${pulled.display()}"))
    |> count --jobs=zero_jobs(Path("${evaluated.display()}"))
  print \${count}
}
""",
  )?
  test.ok(! output.success, output.stdout)?
  test.contains(output.stderr, "stream worker count must be positive", output.stderr)?
  test.eq(evaluated.read_text()?, "evaluated")?
  test.ok(! pulled.exists()?)?
}

proc test_par_map_jobs_rejects_dynamic_zero(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    """
proc zero_jobs() [] -> Int { return 0 }

proc main() [error] {
  let values = [1, 2] |> par-map --jobs=zero_jobs() { |n| n }
  print \${values.len()}
}
""",
  )?
  test.ok(! output.success, output.stdout)?
  test.contains(output.stderr, "stream worker count must be positive", output.stderr)?
}

proc test_reduce_by_jobs_rejects_static_zero_in_checker(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    """
proc main() [error] {
  let grouped = [1] |> reduce-by --sum --jobs=0 { |n| {key: "all", value: n} }
  print \${grouped.get("all", 0)}
}
""",
  )?
  test.ok(! output.success, output.stdout)?
  test.contains(output.stderr, "check.stream-jobs", output.stderr)?
}

proc test_par_map_reduce_by_fuses_to_worker_aggregation() [error] {
  let nums = [0] |> range(0, 50000)

  let fused = nums
    |> par-map --jobs=8 { |n|
      {
        bucket: if n % 4 == 0 { "a" } else if n % 4 == 1 { "b" } else if n % 4 == 2 { "c" } else { "d" },
        doubled: n * 2,
        count: 1,
      }
    }
    |> reduce-by --sum { |row|
      {key: row.bucket, value: {count: row.count, total: row.doubled}}
    }

  let unfused = nums
    |> par-map --jobs=8 { |n|
      {
        bucket: if n % 4 == 0 { "a" } else if n % 4 == 1 { "b" } else if n % 4 == 2 { "c" } else { "d" },
        doubled: n * 2,
        count: 1,
      }
    }
    |> reduce-by --sum --jobs=1 { |row|
      {key: row.bucket, value: {count: row.count, total: row.doubled}}
    }

  for k in unfused.keys() {
    test.eq(fused.get(k, {count: 0, total: 0}), unfused.get(k, {count: 0, total: 0}))?
  }

  test.eq(fused.keys().len(), 4)?
  test.eq(fused.get("a", {count: 0, total: 0}), {count: 12500, total: 624950000})?
}

proc test_flat_map_identity_reduce_by_matches_direct_rows() [error] {
  let nums = [0] |> range(0, 1000)

  let nested = nums
    |> par-map { |n|
      [{key: if n % 2 == 0 { "even" } else { "odd" }, count: 1, total: n}]
    }
    |> flat-map { |rows|
      rows
    }
    |> reduce-by --sum { |row|
      {key: row.key, value: {count: row.count, total: row.total}}
    }

  let direct = nums
    |> par-map { |n|
      {key: if n % 2 == 0 { "even" } else { "odd" }, count: 1, total: n}
    }
    |> reduce-by --sum { |row|
      {key: row.key, value: {count: row.count, total: row.total}}
    }

  test.eq(nested.get("even", {count: 0, total: 0}), direct.get("even", {count: 0, total: 0}))?
  test.eq(nested.get("odd", {count: 0, total: 0}), {count: 500, total: 250000})?
}

proc test_live_walk_flat_map_reduce_by_matches_collected_rows(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "live-stream-flat-map-reduce")?
  fp"${root}/nested".mkdir()?
  fp"${root}/a.txt".write("abc")?
  fp"${root}/nested/b.txt".write("de")?
  fp"${root}/nested/c.md".write("fghi")?

  let streamed = fs.walk(root)
    |> where .kind == "file"
    |> par-map --jobs=4 { |entry|
      [{ext: entry.ext, count: 1, size: entry.size}]
    }
    |> flat-map { |rows| rows }
    |> reduce-by --sum { |row|
      {key: row.ext, value: {count: row.count, size: row.size}}
    }
  let collected_rows = fs.walk(root)
    |> where .kind == "file"
    |> collect()
  let collected = collected_rows
    |> par-map --jobs=4 { |entry|
      {ext: entry.ext, count: 1, size: entry.size}
    }
    |> reduce-by --sum { |row|
      {key: row.ext, value: {count: row.count, size: row.size}}
    }
  test.eq(streamed, collected)?
  test.eq(streamed.get("txt", {count: 0, size: 0}), {count: 2, size: 5})?
  test.eq(streamed.get("md", {count: 0, size: 0}), {count: 1, size: 4})?
}

proc test_live_walk_par_map_for_matches_collected_rows(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "live-stream-par-map-for")?
  fp"${root}/nested".mkdir()?
  fp"${root}/a.txt".write("abc")?
  fp"${root}/nested/b.txt".write("de")?
  fp"${root}/nested/c.md".write("fghi")?

  var streamed_txt_count = 0
  var streamed_txt_size = 0
  var streamed_md_count = 0
  var streamed_md_size = 0
  for row in fs.walk(root)
    |> where .kind == "file"
    |> par-map --jobs=4 { |entry|
      {ext: entry.ext, count: 1, size: entry.size}
    }
    |> where .ext != "" {
    match row.ext {
      "txt" => {
        streamed_txt_count += row.count
        streamed_txt_size += row.size
      }
      "md" => {
        streamed_md_count += row.count
        streamed_md_size += row.size
      }
      _ => {}
    }
  }
  let collected_rows = fs.walk(root)
    |> where .kind == "file"
    |> collect()
  let collected = collected_rows
    |> par-map --jobs=4 { |entry|
      {ext: entry.ext, count: 1, size: entry.size}
    }
    |> reduce-by --sum { |row|
      {key: row.ext, value: {count: row.count, size: row.size}}
    }
  test.eq({count: streamed_txt_count, size: streamed_txt_size}, collected.get("txt", {count: 0, size: 0}))?
  test.eq({count: streamed_md_count, size: streamed_md_size}, collected.get("md", {count: 0, size: 0}))?
  test.eq({count: streamed_txt_count, size: streamed_txt_size}, {count: 2, size: 5})?
  test.eq({count: streamed_md_count, size: streamed_md_size}, {count: 1, size: 4})?
}

proc test_par_map_filesystem_reads_preserve_all_results(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "par-map-filesystem-reads")?
  for index in range(32) {
    fp"${root}/entry-${index}.txt".write(f"${index}\n")?
  }
  let entries = fs.files(root, stat: false)? |> collect()
  let lengths = entries |> par-map --jobs=8 { |entry|
    entry.path.read_text()?.count_chars()
  }
  test.eq(lengths.len(), 32)?
  test.eq(lengths |> sum(), 86)?
}

proc test_projected_reduce_by_sums_output_fields(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    """
let rows = [
  {key: "g", a: 1, b: 10},
  {key: "g", a: 2, b: 20},
  {key: "g", a: 3, b: 30},
]
let reduced = (rows)
  |> reduce-by --sum { |row|
    {key: row.key, value: {x: row.a, y: row.b}}
  }
let g = reduced.get("g", {x: 0, y: 0})
print f"x=\${g.x}"
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    """x=6
""",
  )?
}

proc test_stream_producers_are_lazy_and_run_defers_on_stop(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "stream-marker")
  let rows = test.temp_path(ctx, name: "stream-rows")
  let output = test.run_script(
    ctx,
    f"""
stream nums(marker: Path, rows: Path) [fs, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(5) {
    rows.write(f"row \${n}")?
    yield n
  }
}

let numbers = nums(Path("${marker.display()}"), Path("${rows.display()}"))
# The call did not run the body, so no row has been written yet, and the first
# pull stops at the first row: the defer runs and the later rows never do.
print \${Path("${rows.display()}").exists() ?}
let first = numbers |> first()?
print \${first}
print \${Path("${rows.display()}").read_text() ?}
print \${Path("${marker.display()}").read_text() ?}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    """false
0
row 0
closed
""",
  )?
}

proc test_any_and_all_stop_live_producer_after_decisive_item(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "any-stop-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(5) {
    print f"pull \${n}"
    yield n
  }
}

proc main() [fs, io, error] {
  print f"any=\${numbers(Path("${marker.display()}")) |> any . == 0}"
  print Path("${marker.display()}").read_text()?
  print f"all=\${numbers(Path("${marker.display()}")) |> all . < 0}"
  print Path("${marker.display()}").read_text()?
  let any_block = numbers(Path("${marker.display()}")) |> any { |n|
    let matched = n == 0
    matched
  }
  print f"any_block=\${any_block}"
  print Path("${marker.display()}").read_text()?
  let all_block = numbers(Path("${marker.display()}")) |> all { |n|
    let matched = n < 0
    matched
  }
  print f"all_block=\${all_block}"
  print Path("${marker.display()}").read_text()?
  print f"none=\${numbers(Path("${marker.display()}")) |> any . == 99}"
  print f"all_true=\${numbers(Path("${marker.display()}")) |> all . < 5}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    "pull 0\nany=true\nclosed\npull 0\nall=false\nclosed\npull 0\nany_block=true\nclosed\npull 0\nall_block=false\nclosed\npull 0\npull 1\npull 2\npull 3\npull 4\nnone=false\npull 0\npull 1\npull 2\npull 3\npull 4\nall_true=true\n",
  )?
}

proc test_live_tee_where_take_stops_upstream_and_closes_producer(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "tee-take-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(5) {
    print f"pull \${n}"
    yield n
  }
}

proc amount() [io] -> Int {
  print "count"
  return 2
}

proc main() [fs, io, error] {
  let taken = numbers(Path("${marker.display()}"))
    |> tee { |n| print f"tee \${n}" }
    |> where . % 2 == 0
    |> take(amount())
    |> collect()
  print f"rows=\${taken.len()} \${taken[0]} \${taken[1]}"
  print Path("${marker.display()}").read_text()?
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    "count\npull 0\ntee 0\npull 1\ntee 1\npull 2\ntee 2\nrows=2 0 2\nclosed\n",
  )?
}

proc test_live_serial_collect_runs_stages_in_item_order(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull ${n}"
    yield n
  }
}

proc main() [io] {
  let rows = numbers()
    |> tee { |n| print f"first ${n}" }
    |> tee { |n| print f"second ${n}" }
    |> collect()
  print f"rows=${rows.len()}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    "pull 0\nfirst 0\nsecond 0\npull 1\nfirst 1\nsecond 1\npull 2\nfirst 2\nsecond 2\nrows=3\n",
  )?
}

proc test_live_serial_expression_boundary_runs_stages_in_item_order(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(2) {
    print f"pull ${n}"
    yield n
  }
}

proc main() [io] {
  let rows = numbers()
    |> tee { |n| print f"first ${n}" }
    |> tee { |n| print f"second ${n}" }
  for n in rows { print f"row ${n}" }
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\nfirst 0\nsecond 0\npull 1\nfirst 1\nsecond 1\nrow 0\nrow 1\n")?
}

proc test_live_serial_for_interleaves_body_and_stops_producer(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "serial-for-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(5) {
    print f"pull \${n}"
    yield n
  }
}

proc main() [fs, io, error] {
  for n in numbers(Path("${marker.display()}"))
    |> tee { |value| print f"tee \${value}" }
    |> where . % 2 == 0 {
    print f"row \${n}"
    if n == 2 { break }
  }
  print Path("${marker.display()}").read_text()?
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\ntee 0\nrow 0\npull 1\ntee 1\npull 2\ntee 2\nrow 2\nclosed\n")?
}

proc test_live_serial_for_keeps_source_and_stage_state(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
proc source() [io] -> List[Int] {
  print "source"
  return [0, 1, 2, 3]
}

proc main() [io] {
  for row in source()
    |> drop(1)
    |> flat-map { |n| [n, n + 10] }
    |> enumerate
    |> take(3) {
    print f"row ${row.index}:${row.value}"
    if row.index == 1 { continue }
    print f"kept ${row.value}"
  }
}
""",
  )?
  test.eq(
    output.stdout,
    "source\nrow 0:1\nkept 1\nrow 1:11\nrow 2:2\nkept 2\n",
  )?
  test.ok(output.success, output.stderr)?
}

proc test_live_serial_for_take_closes_producer(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "serial-for-take-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(4) {
    print f"pull \${n}"
    yield n
  }
}

proc main() [fs, io, error] {
  for n in numbers(Path("${marker.display()}")) |> take(2) {
    print f"row \${n}"
  }
  print Path("${marker.display()}").read_text()?
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\nrow 0\npull 1\nrow 1\nclosed\n")?
}

proc test_raw_stream_for_continue_reaches_next_item(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull ${n}"
    yield n
  }
}

proc main() [io] {
  for n in numbers() {
    if n == 1 { continue }
    print f"row ${n}"
  }
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\nrow 0\npull 1\npull 2\nrow 2\n")?
}

proc test_returned_stream_delegates_rows_and_cleanup(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "returned-stream-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(4) {
    print f"pull \${n}"
    yield n
  }
}

proc source(marker: Path) [fs, io, error] -> Stream[Int] {
  print "source"
  return numbers(marker)
}

proc main() [fs, io, error] {
  for n in source(Path("${marker.display()}")) {
    print f"row \${n}"
    if n == 1 { break }
  }
  print Path("${marker.display()}").read_text()?
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "source\npull 0\nrow 0\npull 1\nrow 1\nclosed\n")?
}

proc test_live_serial_for_error_closes_trace_and_producer(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "serial-for-error-marker")
  let output = test.run_xsht_trace(
    ctx,
    f"""
stream words(marker: Path) [fs, error] -> Stream[Str] {
  defer marker.write("closed")?
  yield "1"
  yield "bad"
}

proc main() [fs, io, error] {
  for n in words(Path("${marker.display()}")) |> map .parse_int_decimal()? {
    print f"row \${n}"
  }
}
""",
    ["--trace", "--raw"],
  )?
  test.eq(output.status, 3)?
  test.contains(output.stderr, "parse-int: invalid integer `bad`")?
  test.eq(output.stderr.split("kind=stream.stage.enter", -1).len(), 2)?
  test.eq(output.stderr.split("kind=stream.stage.exit", -1).len(), 2)?
  test.eq(marker.read_text()?, "closed")?
}

proc test_live_flat_map_take_stops_within_expanded_row(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "flat-map-take-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(4) {
    print f"pull \${n}"
    yield n
  }
}

proc main() [fs, io, error] {
  let rows = numbers(Path("${marker.display()}"))
    |> flat-map { |n|
      print f"expand \${n}"
      [n, n + 10]
    }
    |> tee { |n| print f"expanded \${n}" }
    |> take(3)
    |> collect()
  print f"rows=\${rows[0]} \${rows[1]} \${rows[2]}"
  print Path("${marker.display()}").read_text()?
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\nexpand 0\nexpanded 0\nexpanded 10\npull 1\nexpand 1\nexpanded 1\nrows=0 10 1\nclosed\n")?
}

proc test_live_map_drop_and_where_block_keep_take_bounded(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "map-take-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(5) {
    print f"pull \${n}"
    yield n
  }
}

proc main() [fs, io, error] {
  let mapped = numbers(Path("${marker.display()}"))
    |> map { |n| n + 1 }
    |> drop(1)
    |> take(2)
    |> collect()
  print f"mapped=\${mapped[0]} \${mapped[1]}"
  let filtered = numbers(Path("${marker.display()}"))
    |> where { |n|
      let even = n % 2 == 0
      even
    }
    |> take(2)
    |> collect()
  print f"filtered=\${filtered[0]} \${filtered[1]}"
  let enumerated = numbers(Path("${marker.display()}"))
    |> map { |n|
      let next = n + 1
      next
    }
    |> enumerate
    |> take(2)
    |> collect()
  print f"enumerated=\${enumerated[0].index}:\${enumerated[0].value} \${enumerated[1].index}:\${enumerated[1].value}"
  print Path("${marker.display()}").read_text()?
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    "pull 0\npull 1\npull 2\nmapped=2 3\npull 0\npull 1\npull 2\nfiltered=0 2\npull 0\npull 1\nenumerated=0:1 1:2\nclosed\n",
  )?
}

proc test_live_bounded_map_error_closes_producer(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "bounded-map-error-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, error] -> Stream[Int] {
  defer marker.write("closed")?
  yield 1
  yield 2
}

proc main() [io, fs, error] {
  let taken = numbers(Path("${marker.display()}"))
    |> map { |n| ("bad".parse_int()?) + n }
    |> take(1)
    |> collect()
  print f"rows=\${taken.len()}"
}
""",
  )?
  test.ok(! output.success)?
  test.contains(output.stderr, "invalid")?
  test.eq(marker.read_text()?, "closed")?
}

proc test_live_tee_any_and_where_first_stop_upstream(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "bounded-terminal-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {
  defer marker.write("closed")?
  for n in range(5) {
    print f"pull \${n}"
    yield n
  }
}

proc main() [io, fs, error] {
  let found = numbers(Path("${marker.display()}"))
    |> tee { |n| print f"tee \${n}" }
    |> any . == 0
  print f"found=\${found}"
  print Path("${marker.display()}").read_text()?
  let block_found = numbers(Path("${marker.display()}"))
    |> tee { |n| print f"tee \${n}" }
    |> any { |n|
      let matched = n == 0
      matched
    }
  print f"block_found=\${block_found}"
  print Path("${marker.display()}").read_text()?
  let first = numbers(Path("${marker.display()}"))
    |> where . % 2 == 1
    |> first()?
  print f"first=\${first}"
  print Path("${marker.display()}").read_text()?
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 0\ntee 0\nfound=true\nclosed\npull 0\ntee 0\nblock_found=true\nclosed\npull 0\npull 1\nfirst=1\nclosed\n")?
}

proc test_live_where_first_empty_preserves_error_and_cleanup(ctx: TestContext) [fs, error] {
  let marker = test.temp_path(ctx, name: "first-empty-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, error] -> Stream[Int] {
  defer marker.write("closed")?
  yield 1
  yield 2
}
proc main() [fs, io, error] {
  let first = numbers(Path("${marker.display()}")) |> where . > 10 |> first()?
  print f"first=\${first}"
}
""",
  )?
  test.ok(! output.success)?
  test.contains(output.stderr, "empty-stream")?
  test.eq(marker.read_text()?, "closed")?
}

proc test_zero_argument_stream_producers_run_from_every_call_position(ctx: TestContext) [error] {
  # A call with no arguments reaches the frame engine's call decision without
  # walking an argument list, so a producer spelled that way has to be
  # recognized there too: a producer pushed as an ordinary frame runs its body
  # with nowhere for `yield` to report and fails as a function that did not
  # return.
  let output = test.run_script(
    ctx,
    f"""
stream once() [] -> Stream[Int] {
  yield 1
  yield 2
}

stream twice() [] -> Stream[Int] {
  for item in once() {
    yield item * 2
  }
}

proc total() [error] -> Int {
  var sum = 0
  for item in once() {
    sum = sum + item
  }
  return sum
}

var direct = 0
for item in once() {
  direct = direct + item
}
print f"direct=\${direct}"
let bound = once().collect()
print f"bound=\${bound.len()}"
print f"total=\${total()}"
let doubled = twice().collect()
print f"doubled=\${doubled.len()}"
let mapped = once() |> map { |item| item + 1 } |> collect()
print f"mapped=\${mapped.len()}"
let first = once() |> first()
print f"first=\${first ?? -1}"
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    """direct=3
bound=2
total=3
doubled=2
mapped=2
first=1
""",
  )?
}

proc test_count_and_group_by_jobs_hint_preserves_results() [error] {
  # group-by must preserve encounter order within each group.
  let nums = [0] |> range(0, 20000)

  let cpar = nums
    |> count {
      if . % 2 == 0 {
        "even"
      } else {
        "odd"
      }
    }

  let cser = nums
    |> count --jobs=1 {
      if . % 2 == 0 {
        "even"
      } else {
        "odd"
      }
    }

  let gpar = nums
    |> group-by { |n|
      n % 3
    }
    |> sort-by { |g|
      g.key
    }
    |> map { |g|
      g.items
    }

  let gser = nums
    |> group-by --jobs=1 { |n|
      n % 3
    }
    |> sort-by { |g|
      g.key
    }
    |> map { |g|
      g.items
    }

  test.eq(cpar.get("even", 0), cser.get("even", 0))?
  test.eq(cpar.get("odd", 0), cser.get("odd", 0))?
  test.eq(cpar.get("even", 0), 10000)?
  test.eq(gpar, gser)?
  test.eq(gpar.len(), 3)?
}

proc test_stream_adapters_bridge_text_bytes_and_json_lines() [process, error] {
  let captured = run.text printf "%s\n" "a.txt" "b.log" ?

  let paths = captured
    |> text.lines
    |> map { |line|
      fp"${line}"
    }

  let chunks = b"abcde" |> bytes.chunks(2)

  let rows = """{"name":"a","size":1}
{"name":"b","size":2}
"""
  |> json.lines
  |> sort-by .size

  let streamed = """{"name":"c","size":3}
""" |> json.stream

  let words = "one two".words()
  test.eq(paths[0].ext, "txt")?
  test.eq(paths[1].name, "b.log")?
  test.eq(chunks[0], b"ab")?
  test.eq(chunks[2], b"e")?
  test.eq(rows[1].name, "b")?
  test.eq(rows[0].size, 1)?
  test.eq(streamed[0].name, "c")?
  test.eq(words[1], "two")?
}

proc test_line_methods_and_adapters_are_lazy_sources(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "stream-lines")?
  let input = fp"${root}/input.txt"

  input.write("""alpha\r
beta
gamma
""")?

  test.eq((input.lines()? |> first())?, "alpha")?

  test.eq(
    ("""one
two
""".lines()
  |> drop(1)
  |> first())?,
    "two",
  )?

  test.eq(
    ("""red
blue
"""
  |> text.lines
  |> take(1))[0],
    "red",
  )?

  test.eq(
    """x
y
""".lines()
  .collect()[1],
    "y",
  )?

  test.eq(b"a\nb\n".lines().collect().len(), 2)?
  test.eq(input.bytes_lines()?.collect()[1], b"beta")?
}

proc test_terminal_newline_does_not_add_empty_line_for_round_trip() [error] {
  let lines = """a
b
""".lines()
  .collect()

  test.eq(lines, ["a", "b"])?
  test.eq(
    f"""${lines.join("\n")}
""",
    """a
b
""",
  )?
}

proc test_flat_map_consumes_live_streams_returned_by_blocks(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "flat-map-live-stream")?
  let left = fp"${root}/left.txt"
  let right = fp"${root}/right.txt"

  left.write("""a
b
""")?

  right.write("""c
d
""")?

  let lines = [left, right]
    |> flat-map { |pth|
      pth.lines()?
    }

  test.eq(lines, ["a", "b", "c", "d"])?
}

proc test_flat_map_drains_one_nested_stream_before_next_outer_pull(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream outer() [io] -> Stream[Int] {
  for n in range(3) {
    print f"outer ${n}"
    yield n
  }
}

stream inner(n: Int) [io] -> Stream[Int] {
  for offset in range(3) {
    print f"inner ${n}:${offset}"
    yield n * 10 + offset
  }
}

proc main() [io, error] {
  let first = outer() |> flat-map { |n| inner(n) } |> first()?
  print f"first=${first}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "outer 0\ninner 0:0\ninner 0:1\ninner 0:2\nfirst=0\n")?
}

proc test_sort_boundary_materializes_serial_prefix_before_key_projection(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in [2, 1, 3] {
    print f"pull ${n}"
    yield n
  }
}

proc key(n: Int) [io] -> Int {
  print f"key ${n}"
  return n
}

proc main() [io, error] {
  let rows = numbers()
    |> tee { |n| print f"tee ${n}" }
    |> sort-by { |n| key(n) }
    |> take(1)
    |> collect()
  print f"row=${rows[0]}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 2\ntee 2\npull 1\ntee 1\npull 3\ntee 3\nkey 2\nkey 1\nkey 3\nrow=1\n")?
}

proc test_sort_by_desc_option_error_precedes_live_source_pull(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  print "pull"
  yield 1
}

proc descending() [io, error] -> Bool {
  print "desc"
  let number = "bad".parse_int()?
  return number > 0
}

proc main() [io, error] {
  let sorted = numbers() |> sort-by --desc=descending() { |n| n }
  print "after"
}
""",
    ["--raw"],
  )?
  test.ok(! output.success)?
  test.eq(output.stdout, "desc\n")?
  test.contains(output.stderr, "invalid")?
  test.contains(output.stderr, "kind=stream.stage.exit name=\"sort-by\"")?
}

proc test_sort_by_desc_reverses_sort_order() [error] {
  let nums = [
    3,
    1,
    4,
    1,
    5,
    9,
    2,
    6,
  ]

  let asc = nums
    |> sort-by { |n|
      n
    }

  let desc = nums
    |> sort-by --desc { |n|
      n
    }

  let words = ["banana", "apple", "cherry"]
    |> sort-by --desc { |word|
      word
    }

  test.eq(asc[0], 1)?
  test.eq(asc[7], 9)?
  test.eq(desc[0], 9)?
  test.eq(desc[7], 1)?
  test.eq(words[0], "cherry")?
  test.eq(words[2], "apple")?
}

proc test_sort_by_compound_record_keys_and_stability() [error] {
  let records = [
    {
      name: "b",
      count: 2,
    },
    {
      name: "a",
      count: 1,
    },
    {
      name: "c",
      count: 1,
    },
  ]

  # A two-field record key compares lexicographically: count first, then name.
  let direct = records
    |> sort-by { |r|
      {c: r.count, n: r.name}
    }

  test.eq(direct, [{name: "a", count: 1}, {name: "c", count: 1}, {name: "b", count: 2}])?

  # --desc reverses the compound comparison.
  let desc = records
    |> sort-by --desc { |r|
      {c: r.count, n: r.name}
    }

  test.eq(desc, [{name: "b", count: 2}, {name: "c", count: 1}, {name: "a", count: 1}])?

  # The documented two-pass stable idiom matches the direct compound key.
  let two_pass = records
    |> sort-by { |r|
      r.name
    }
    |> sort-by { |r|
      r.count
    }

  test.eq(two_pass, direct)?

  # Stable sort keeps equal-key items in source order.
  let repeats = [
    {
      name: "x",
      count: 1,
    },
    {
      name: "y",
      count: 1,
    },
    {
      name: "z",
      count: 1,
    },
  ]
  let stable = repeats |> sort-by .count
  test.eq(stable, repeats)?

  # Whole-record sort uses the same record ordering.
  let whole = records |> sort
  test.eq(whole, direct)?
}

proc test_sort_by_rejects_non_orderable_keys_at_runtime(ctx: TestContext) [error] {
  let failed = test.run_script(
    ctx,
    """
let rows = [{name: "b"}, {name: "a"}]
let values = map.empty().set("key", ["not-orderable"])
let key = values.get("key", 0)
let out = (rows) |> sort-by { |_| key } |> collect()
for r in out { print \${r.name} }
""",
  )?

  test.ok(! failed.success, failed.stdout)?
  test.contains(failed.stderr, "sort-by", failed.stderr)?
  test.contains(failed.stderr, "List", failed.stderr)?
}

proc test_sort_by_map_accumulator_any_typed_fields() [error] {
  # Map.empty() is Map[Any], so Map.get(k, 0) yields an Any-typed field. A
  # sort-by over such a field must checker-accept the same way the runtime does
  # (the actual value is a supported scalar Int), matching the loud-failure
  # gate in lower/ops.
  let counts = map.empty()
  let keys = ["b", "a"]
  let acc = counts.set("a", 2).set("b", 1)

  let by_count = keys
    |> map { |k|
      {count: acc.get(k, 0), ext: k}
    }
    |> sort-by .count
  test.eq(by_count, [{count: 1, ext: "b"}, {count: 2, ext: "a"}])?

  # The list-comprehension equivalent accepts and sorts identically.
  let by_count_comp = [{count: acc.get(k, 0), ext: k} for k in keys] |> sort-by .count
  test.eq(by_count_comp, by_count)?
}

proc test_structured_stream_batch_count_and_argv_limits() [process, error] {
  let by_count = [1, 2, 3, 4, 5] |> batch --count=2
  let by_size = [p"aaaa", p"bbbb", p"cccc"] |> batch --max-bytes=10
  test.eq(by_count, [[1, 2], [3, 4], [5]])?
  test.eq(by_size[0], [p"aaaa", p"bbbb"])?
  test.eq(by_size[1], [p"cccc"])?

  [p"one", p"two"]
    |> batch --max-argv
    |> each { |files|
      run true @files ?
    }

  test.eq(
    [1]
      |> where false
      |> batch --count=2
      |> count(),
    0,
  )?
}

proc test_batch_max_bytes_error_stops_and_closes_live_source(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(
    ctx,
    r"""
proc close() [io] { print "closed" }

stream paths() [io] -> Stream[Path] {
  defer close()
  for value in ["ok", "oversized", "after"] {
    print f"pull ${value}"
    yield Path(value)
  }
}

proc main() [io, error] {
  let batches = paths() |> batch --max-bytes=3
  print "after"
}
""",
    ["--raw"],
  )?
  test.ok(! output.success)?
  test.eq(output.stdout, "pull ok\npull oversized\nclosed\n")?
  test.contains(output.stderr, "batch item exceeds byte budget")?
  test.contains(output.stderr, "kind=stream.stage.exit name=\"batch\"")?
}

proc test_repeat_zero_does_not_pull_live_source(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull ${n}"
    yield n
  }
}

proc main() [io, error] {
  let rows = numbers() |> repeat(0)
  print f"rows=${rows.len()}"
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "rows=0\n")?
}

proc test_count_producer_error_runs_defer_and_closes_trace(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(
    ctx,
    r"""
proc close() [io] { print "closed" }

stream numbers() [io, error] -> Stream[Int] {
  defer close()
  yield 1
  yield "bad".parse_int()?
}

proc main() [io, error] {
  let counted = numbers() |> count()
  print f"counted=${counted}"
}
""",
    ["--raw"],
  )?
  test.ok(! output.success)?
  test.eq(output.stdout, "closed\n")?
  test.contains(output.stderr, "invalid integer `bad`")?
  test.contains(output.stderr, "kind=stream.stage.exit name=\"count\"")?
}

proc test_parallel_stream_stages_are_bounded_and_deterministic() [error] {
  test.eq(
    [1, 2, 3, 4]
      |> par-map { |x|
        x * 2
      },
    [2, 4, 6, 8],
  )?

  var seen: List[Str] = []

  ["a", "b"]
    |> each --jobs=2 { |x|
      seen = seen.push(x)
    }

  test.eq(seen, ["a", "b"])?
}

proc test_each_jobs_trace_reports_serial_execution(ctx: TestContext) [error] {
  let trace = test.run_xsht_trace(
    ctx,
    r"""
proc main() [io] {
  [1, 2] |> each --jobs=2 { |n| print f"item=${n}" }
}
""",
    ["--trace", "--raw"],
  )?
  test.ok(trace.success, trace.stderr)?
  test.eq(trace.stdout, "item=1\nitem=2\n")?
  test.contains(trace.stderr, "kind=stream.stage.enter name=\"each\"")?
  test.contains(trace.stderr, "kind=stream.stage.exit name=\"each\"")?
  test.not_contains(trace.stderr, "kind=parallel.job.")?
  test.not_contains(trace.stderr, "kind=parallel.cancel")?
}

proc test_parallel_stream_preserves_filtered_order() [error] {
  test.eq(
    [0, 1, 2, 3, 4, 5]
      |> where . >= 2
      |> par-map --jobs=3 { |x|
        x * 10
      },
    [20, 30, 40, 50],
  )?
}

proc test_structured_streams_walk_filter_map_collect_and_count(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "stream-walk")?
  let nested = fp"${root}/nested"
  nested.mkdir()?
  fp"${root}/a.txt".write("a")?
  fp"${nested}/b.txt".write("b")?
  let entries = fs.files(root) |> collect()

  let names = entries
    |> map .name
    |> sort-by .

  let count = fs.files(root) |> count()
  test.eq(names, ["a.txt", "b.txt"])?
  test.eq(count, 2)?
}

proc test_direct_collect_of_lazy_module_stream_is_a_list(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "stream-direct-collect")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/b.txt".write("b")?
  fp"${root}/c.txt".write("c")?

  # A module-produced lazy stream piped straight into the collect terminal, with
  # no intervening transformation stage, must lower and run as a materialized
  # list (regression: the direct result was mis-typed as a stream, so `len` was
  # rejected and the pipeline failed to compile).
  let all = fs.files(root) |> collect()
  test.eq(all.len(), 3)?
}

proc test_table_print_wraps_cells_to_terminal_width(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    """
let rows = [{name: "very-long-command-name-that-keeps-going", size: 123}]
(rows) |> table.print(columns: ["name", "size"])
""",
    [],
    {COLUMNS: "40"},
  )?

  test.ok(output.success, output.stderr)?
  test.not_contains(output.stdout, "\u{2026}")?
  test.contains(output.stdout, "very-long-command-name-that-k")?
  test.contains(output.stdout, "eeps-going")?

  for line in output.stdout.lines() {
    test.ok(line.count_chars() <= 40, line)?
  }
}

proc test_stream_stages_are_trace_observable(ctx: TestContext) [error] {
  let table_trace = test.run_xsht_trace(
    ctx,
    """
let rows = [{name: "b", size: 2}, {name: "a", size: 1}]
(rows) |> sort-by { |row| row.size } |> table.print(columns: ["name", "size"])
""",
    ["--trace", "--raw"],
  )?

  test.ok(table_trace.success, table_trace.stderr)?
  test.contains(table_trace.stdout, "\u{2502} a")?
  test.contains(table_trace.stdout, "\u{2502} b")?
  test.contains(table_trace.stderr, "kind=stream.stage.enter")?
  test.contains(table_trace.stderr, "kind=stream.stage.exit")?
  test.contains(table_trace.stderr, "name=\"sort-by\"")?
  test.contains(table_trace.stderr, "stage=b\"sort-by\"")?
  test.contains(table_trace.stderr, "name=\"table.print\"")?
  test.contains(table_trace.stderr, "stage=b\"table.print\"")?
  test.contains(table_trace.stderr, "item_count=2")?

  let bounded_trace = test.run_xsht_trace(
    ctx,
    r"""
stream numbers() [] -> Stream[Int] {
  yield 1
  yield 2
}
proc main() [io, error] {
  let taken = numbers() |> tee { |n| print f"seen=${n}" } |> where . > 0 |> take(1) |> collect()
  print f"count=${taken.len()}"
}
""",
    ["--trace", "--raw"],
  )?
  test.ok(bounded_trace.success, bounded_trace.stderr)?
  test.eq(bounded_trace.stdout, "seen=1\ncount=1\n")?
  for name in ["tee", "where", "take"] {
    test.contains(bounded_trace.stderr, f"name=\"${name}\"")?
    test.contains(bounded_trace.stderr, f"stage=b\"${name}\"")?
  }

  let adapter_trace = test.run_xsht_trace(
    ctx,
    """"a\\nb\\n" |> text.lines()
""",
    ["--raw"],
  )?

  test.ok(adapter_trace.success, adapter_trace.stderr)?
  test.contains(adapter_trace.stderr, "kind=stream.stage.enter")?
  test.contains(adapter_trace.stderr, "kind=stream.stage.exit")?
  test.contains(adapter_trace.stderr, "name=\"text.lines\"")?
  test.contains(adapter_trace.stderr, "stage=b\"text.lines\"")?

  let batch_trace = test.run_xsht_trace(
    ctx,
    """[1, 2, 3] |> batch --count=2
""",
    ["--raw"],
  )?

  test.ok(batch_trace.success, batch_trace.stderr)?
  test.contains(batch_trace.stderr, "kind=stream.stage.enter")?
  test.contains(batch_trace.stderr, "kind=stream.stage.exit")?
  test.contains(batch_trace.stderr, "name=\"batch\"")?
  test.contains(batch_trace.stderr, "stage=b\"batch\"")?
  test.contains(batch_trace.stderr, "item_count=3")?
}

proc test_stream_errors_include_trace_context(ctx: TestContext) [error] {
  let stream_error = test.run_xsht_trace(
    ctx,
    """
let xs = ["only"]
let values = [1] |> map { |index| xs[index] }
print \${values[0]}
""",
    ["--trace", "--raw"],
  )?

  test.eq(stream_error.status, 3)?
  test.contains(stream_error.stderr, "stream stage `map` item 0 failed")?
  test.contains(stream_error.stderr, "index-out-of-range")?
  test.contains(stream_error.stderr, "kind=stream.item.error")?
  test.contains(stream_error.stderr, "item_index=0")?

  let live_error = test.run_xsht_trace(
    ctx,
    """
stream numbers() [] -> Stream[Int] { yield 1 }
proc main() [error] {
  let values = numbers() |> map { |n| "bad".parse_int()? + n } |> take(1) |> collect()
}
""",
    ["--trace", "--raw"],
  )?
  test.eq(live_error.status, 3)?
  test.contains(live_error.stderr, "kind=stream.stage.enter")?
  test.contains(live_error.stderr, "kind=stream.stage.exit name=\"take\"")?
  test.contains(live_error.stderr, "kind=stream.stage.exit name=\"map\"")?
  test.contains(live_error.stderr, "invalid")?

  let par_error = test.run_xsht_trace(
    ctx,
    """
let xs = ["only"]
let values = [1, 2, 3] |> par-map --jobs=1 { |index| xs[index] }
""",
    ["--trace", "--raw"],
  )?

  test.eq(par_error.status, 3)?
  test.contains(par_error.stderr, "stream stage `par-map` item 0 failed")?
  test.contains(par_error.stderr, "index-out-of-range")?
  test.contains(par_error.stderr, "kind=parallel.job.start")?
  test.contains(par_error.stderr, "kind=parallel.job.end")?
  test.contains(par_error.stderr, "item_index=0")?

  let idle_error = test.run_xsht_trace(
    ctx,
    """
let xs = ["only"]
let values = [1, 2, 3] |> par-map --jobs=8 { |index| xs[index] }
""",
    ["--trace", "--raw"],
  )?

  test.eq(idle_error.status, 3)?
  test.contains(idle_error.stderr, "stream stage `par-map` item 0 failed")?
  test.contains(idle_error.stderr, "index-out-of-range")?
}

proc test_fs_files_lazy_folding_terminals_match_eager_results(ctx: TestContext) [fs, error] {
  # count/sum/min/max/fold drive the live stream by folding one item at a time.
  let root = test.temp_dir(ctx, name: "fs-walk-fold")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/bb.txt".write("bb")?
  fp"${root}/ccc.txt".write("ccc")?
  test.eq(fs.files(root) |> count(), 3)?

  test.eq(
    fs.files(root)
      |> map .size
      |> sum,
    6,
  )?

  test.eq(
    (fs.files(root)
      |> map .size
      |> min)?,
    1,
  )?

  test.eq(
    (fs.files(root)
      |> map .size
      |> max)?,
    3,
  )?

  test.eq(
    fs.files(root)
      |> map .size
      |> fold(0) { |acc|
        acc + .
      },
    6,
  )?
}
