test test_range_checks_integer_bounds_before_execution { |ctx|
  for case in [
    {source: "range(missing.end)", code: "check.unresolved-name"},
    {source: "range(missing.end, 3)", code: "check.unresolved-name"},
    {source: "range(0, missing.end)", code: "check.unresolved-name"},
    {source: "range(\"three\")", code: "check.type-mismatch"},
    {source: "range(false, 3)", code: "check.type-mismatch"},
    {source: "range(0, 3.5)", code: "check.type-mismatch"},
    {source: "range(end: 3)", code: "check.named-arg"},
    {source: "range(@[3])", code: "check.call-splice"},
    {source: "range()", code: "check.arity"},
    {source: "range(0, 3, 1)", code: "check.arity"},
  ] {
    let output = test.run_script(
      ctx,
      "print \"started\"\nproc main() { for _ in " + case.source + " {} }\n",
    )?
    assert output.status == 2, output.stderr
    assert case.code in output.stderr, output.stderr
    assert "compact.indexed-build" not in output.stderr, output.stderr
    assert output.stdout == "", output.stdout
  }

  let valid = test.run_script(
    ctx,
    r"""pure count() -> Int { 3 }
pure start() -> Int { 1 }
proc main() [io] {
  for n in range(count()) { print $n }
  for n in range(start(), count()) { print $n }
}
""",
  )?
  assert valid.success, valid.stderr
  assert valid.stdout == "0\n1\n2\n1\n2\n", valid.stdout
}

test test_pipeline_parameters_shadow_outer_bindings_and_restore_their_types { |ctx|
  for stage in ["fold", "reduce"] {
    let source = r"""type Totals = {values: Map[Int], seed: Str, item: Str}
type ImplicitTotals = {values: Map[Int], seed: Str}
pure totals(seed: Str, item: Str) -> Totals {
  let values = [1, 2] |> STAGE({[seed]: 0}) { |seed, item|
    seed.set("sum", (seed.get("sum") ?? 0) + item)
  }
  {values: values, seed: seed, item: item}
}
pure implicit(seed: Str) -> ImplicitTotals {
  let values = [1, 2] |> STAGE({[seed]: 0}) { |seed|
    seed.set("sum", (seed.get("sum") ?? 0) + .)
  }
  {values: values, seed: seed}
}
let explicit = totals("initial", "outer")
let implicit_item = implicit("initial")
print f"{explicit.values["initial"]}:{explicit.values["sum"]}:{explicit.seed}:{explicit.item}"
print f"{implicit_item.values["initial"]}:{implicit_item.values["sum"]}:{implicit_item.seed}"
""".replace("STAGE", stage)
    let output = test.run_script(ctx, source)?
    assert output.success, output.stderr
    assert output.stdout == "0:3:initial:outer\n0:3:initial\n", output.stdout
  }

  let output = test.run_script(
    ctx,
    r"""type Transformed = {values: List[Int], outer: Str, local: Str}
pure transform(value: Str) -> Transformed {
  let local = value
  let values = [1, 2] |> map { |local|
    let doubled = local * 2
    doubled
  } |> where { |value| value > 2 }
  {values: values, outer: value, local: local}
}
let transformed = transform("outer")
print f"{transformed.values[0]}:{transformed.outer}:{transformed.local}"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "4:outer:outer\n", output.stdout

  for stage in ["fold", "reduce"] {
    let duplicate = test.run_script(ctx, "let value = [1] |> " + stage + "(0) { |same, same| same }\n")?
    assert duplicate.status == 2, duplicate.stderr
    assert "check.duplicate-name" in duplicate.stderr, duplicate.stderr
    assert "compact.indexed-build" not in duplicate.stderr, duplicate.stderr
  }
}

test test_stream_adapters_and_transform_stages {
  let lines = """alpha
beta
""" |> text.lines

  assert lines == ["alpha", "beta"]
  let chunks = b"abcdef" |> bytes.chunks(2)
  assert chunks[0] == b"ab"
  assert chunks[2] == b"ef"

  let json_lines = """{"name":"alpha","size":2}
{"name":"beta","size":1}
"""
  |> json.lines
  |> sort-by .size.require(Int)?

  assert json_lines[0].name == "beta"

  let json_stream = """{"ok":true}
{"ok":false}
""" |> json.stream

  assert json_stream[1].ok == false

  assert ([3, 1, 2, 2]
    |> where . > 1
    |> sort
    |> unique-by .
    |> map { |n|
      n * 2
    }) == [4, 6]

  assert ([1, 2, 3, 4] |> take(2)) == [1, 2]
  assert ([1, 2, 3, 4] |> drop(2)) == [3, 4]
  assert ([1, 2] |> repeat(2)) == [1, 2, 1, 2]
  assert ([0] |> range(1, 4)) == [1, 2, 3]
  assert ([0] |> range(4, 1)) == [4, 3, 2]

  assert (["ab", "c"]
    |> flat-map { |word|
      word.split("")
    }) == ["a", "b", "c"]

  assert ([1, 2, 3]
    |> fold(0) { |acc|
      acc + .
    }) == 6

  # Accumulator-plus-item form: the block binds the accumulator (typed by the
  # initial value) before the stream item, and the tail produces the accumulator.
  assert ([1, 2, 3]
    |> fold(0) { |acc, it|
      acc + it
    }) == 6

  assert ([1, 2, 3]
    |> reduce(10) { |acc, it|
      acc + it
    }) == 16

  # A postfix `?` inside a stream-stage closure, followed by a method call on
  # the unwrapped value, must compile and propagate normally instead of
  # tripping the compact indexed-IR `full_ir_function_blocker`. The blocker was
  # caused by the slot-based pipeline inference mis-typing a `first`/`last`/
  # `min`/`max` terminal as its input list, so the `.lower()` on the unwrapped
  # item was mistaken for a method on the list.
  let ext_lower = ["a.TXT", "b.com"]
    |> map { |s|
      s.split(".") |> last()?.lower()
    }
    |> collect()
  assert ext_lower == ["txt", "com"]

  # A bare trailing `?` (no method tail) in a stage block is still accepted and
  # unwraps the terminal result inside the closure.
  let firsts = [["a"], ["b"], ["c"]]
    |> map { |row|
      row.get(0)?
    }
    |> collect()
  assert firsts == ["a", "b", "c"]

  # A bare accumulator-ident tail no longer trips the indexed IR builder; it
  # returns the running accumulator unchanged.
  assert ([1, 2, 3]
    |> fold(0) { |x|
      x
    }) == 0

  # Counting through fold without group-by: the accumulator is a Map and the
  # item a Str, which the two-parameter binding types correctly.
  let fold_counts = ["a", "b", "a", "c"]
    |> fold(map.empty()) { |acc, it|
      acc.set(it, (acc.get(it) ?? 0) + 1)
    }
  assert (fold_counts.get("a") ?? 0) == 2
  assert (fold_counts.get("b") ?? 0) == 1
  assert (fold_counts.get("c") ?? 0) == 1
  assert fold_counts.len() == 3

  assert ([1, 2, 3]
    |> reduce(10) { |acc|
      acc + .
    }) == 16

  assert ([1, 2, 3] |> sum) == 6
  assert [3, 1, 2] |> min? == 1
  assert [3, 1, 2] |> max? == 3
  assert [3, 1, 2] |> first()? == 3
  assert [3, 1, 2] |> last()? == 2
  assert [1, 2, 3] |> any . == 2
  assert [1, 2, 3] |> all . > 0
  let expected_counts: Map[Int] = {["1"]: 2, ["2"]: 1}

  assert (["a", "bb", "c"]
    |> count { |word|
      word.count_chars()
    }) == expected_counts

  assert ([1, 2, 3]
    |> par-map { |value|
      value * 2
    }) == [2, 4, 6]

  assert ([1, 2, 3, 4] |> batch(count: 2)) == [[1, 2], [3, 4]]
  let enumerated = ["x", "y"] |> enumerate()
  assert enumerated[1].index == 1
  assert enumerated[1].value == "y"
  let zipped = ["left", "right"] |> zip([10, 20])
  assert zipped[0].left == "left"
  assert zipped[1].right == 20

  let groups = [{kind: "a", value: 1}, {kind: "b", value: 2}, {kind: "a", value: 3}]
    |> group-by .kind
    |> sort-by .key

  assert groups[0].key == "a"
  assert groups[0].items.len() == 2

  let scalar_groups = [3, 1, 2, 1]
    |> group-by { |value|
      value
    }
    |> sort-by { |bucket|
      bucket.key
    }
  assert scalar_groups[0].key == 1
  assert scalar_groups[1].key == 2
  assert scalar_groups[2].key == 3

  let shuffled = [1, 2, 3, 4] |> shuffle(7)
  assert shuffled.len() == 4
  assert (shuffled |> sort) == [1, 2, 3, 4]

  [1, 2]
    |> each { |value|
      assert value > 0
    }

  assert ([1, 2]
    |> tee { |value|
      assert value > 0
    }
    |> map { |value|
      value + 1
    }) == [2, 3]

  [{name: "small", size: 1}, {name: "large", size: 4}] |> table.print(columns: ["name", "size"])
}

test test_fold_initial_branch_expression_types_the_accumulator {
  # A branch or match initializer used to type the fold result as Unknown,
  # which then failed preparation instead of checking.
  let prefix = true
  let joined = ["a", "b"] |> fold(if prefix { "<" } else { "" }) { |acc, item| acc + item }
  let counted = [1, 2, 3] |> fold(match joined { "<ab" => 10, _ => 0 }) { |acc, item| acc + item }
  let suffixed = joined + ">"
  assert suffixed == "<ab>"
  assert counted == 16
}

test test_fold_block_composes_pipeline_over_accumulator_field {
  let result = [0]
    |> fold({parts: ["first", "last"]}) { |acc, _|
      let popped = acc.parts
        |> take(acc.parts.len() - 1)
        |> collect()
      {parts: popped}
    }
  assert result.parts == ["first"]
}

test test_fold_block_supports_nested_if_statement_with_assignment {
  let result = [1, 2, 3]
    |> fold(0) { |acc, item|
      var next = acc
      if item > 1 {
        next = next + item
      }

      next
    }
  assert result == 5
}

test test_fold_block_supports_nested_if_as_branch_tail {
  let result = [1, 2, 3]
    |> fold(0) { |acc, item|
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
  assert result == 5
}

test test_fold_and_reduce_run_direct_effects_in_item_order { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull {n}"
    yield n
  }
}

proc after_item(n: Int) [io] {
  print f"defer {n}"
}

proc main() [io, process, error] {
  let total = numbers() |> fold(0) { |acc, n|
    defer after_item(n)
    run true ?
    print f"fold {n}"
    acc + n
  }
  print f"total={total}"
  let reduced = [1, 2] |> reduce(0) { |acc, n|
    print f"reduce {n}"
    acc + n
  }
  print f"reduced={reduced}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
fold 0
defer 0
pull 1
fold 1
defer 1
pull 2
fold 2
defer 2
total=3
reduce 1
reduce 2
reduced=3
"""
}

test test_fold_error_stops_and_closes_live_source { |ctx|
  let pulled = test.temp_path(ctx, name: "fold-pulled")
  let closed = test.temp_path(ctx, name: "fold-closed")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(pulled: Path, closed: Path) [fs, error] -> Stream[Int] {{
  defer closed.write("closed")?
  for n in range(5) {{
    pulled.write(f"pull {{n}}")?
    yield n
  }}
}}

proc main() [fs, error] {{
  let total = numbers(Path("{pulled}"), Path("{closed}"))
    |> fold(0) {{ |acc, n| acc + 10 / (1 - n) }}
  print ${{total}}
}}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stdout
    assert assertion_condition, assertion_message
  }
  assert pulled.read_text()? == "pull 1"
  assert closed.read_text()? == "closed"
}

test test_sum_rejects_unchecked_stream_before_pulling_source { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Any] {
  defer { print "closed" }
  print "pull 1"
  yield 1
  print "pull bad"
  yield "bad"
  print "pull 3"
  yield 3
}

proc main() [io] {
  let total = numbers() |> sum()
  print f"total={total}"
}
""",
  )?
  assert output.status == 2
  assert "check.dynamic-boundary" in output.stderr
  assert output.stdout == ""
}

test test_keyed_stages_errors_stop_live_source { |ctx|
  for terminal in ["group-by", "count", "unique-by"] {
    let source = f"""\nproc close() [io] {{ print "closed" }}

stream numbers() [io, error] -> Stream[Int] {{
  defer close()
  for n in range(3) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc main() [io, error] {{
  let _result = numbers() |> {terminal} {{ |n| 1 / (1 - n) }}
}}
"""
    let output = test.run_script(ctx, source)?
    {
      let assertion_condition = ! output.success
      let assertion_message = output.stdout
      assert assertion_condition, assertion_message
    }
    assert output.stdout == """pull 0
pull 1
closed
"""
  }
}

test test_keyed_stages_project_before_next_live_pull { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers(label: Str) [io] -> Stream[Int] {
  for n in [2, 1, 2] {
    print f"{label} pull {n}"
    yield n
  }
}

proc key(n: Int) [io] -> Int {
  print f"key {n}"
  return n
}

proc main() [io, error] {
  let groups = numbers("group") |> group-by { |n| key(n) }
  print f"groups={groups[0].key}:{groups[0].items.len()},{groups[1].key}:{groups[1].items.len()}"
  let counts = numbers("count") |> count { |n| key(n) }
  print f"counts={counts.get("1") ?? 0},{counts.get("2") ?? 0}"
  let unique = numbers("unique") |> unique-by { |n| key(n) }
  print f"unique={unique[0]},{unique[1]}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """group pull 2
key 2
group pull 1
key 1
group pull 2
key 2
groups=2:2,1:1
count pull 2
key 2
count pull 1
key 1
count pull 2
key 2
counts=1,2
unique pull 2
key 2
unique pull 1
key 1
unique pull 2
key 2
unique=2,1
"""
}

test test_zip_evaluates_right_before_pulling_and_stops_at_shorter_side { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc close() [io] { print "closed" }

stream numbers() [io, error] -> Stream[Int] {
  defer close()
  for n in range(4) {
    print f"pull {n}"
    yield n
  }
}

proc right() [io] -> List[Int] {
  print "right"
  return [10, 20]
}

proc main() [io, error] {
  let pairs = numbers() |> zip(right())
  print f"pairs={pairs[0].left}:{pairs[0].right},{pairs[1].left}:{pairs[1].right}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """right
pull 0
pull 1
closed
pairs=0:10,1:20
"""
}

test test_zip_right_error_does_not_pull_left { |ctx|
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
  assert ! output.success
  assert output.stdout == """right
"""
  assert "invalid" in output.stderr
  assert "kind=stream.stage.exit name=\"zip\"" in output.stderr
}

test test_zip_collects_right_stream_before_pulling_left { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream left() [io] -> Stream[Int] {
  for n in [1, 2, 3] {
    print f"left {n}"
    yield n
  }
}

stream right() [io] -> Stream[Int] {
  for n in [10, 20] {
    print f"right {n}"
    yield n
  }
}

proc main() [io, error] {
  let pairs = left() |> zip(right())
  print f"pairs={pairs[0].left}:{pairs[0].right},{pairs[1].left}:{pairs[1].right}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """right 10
right 20
left 1
left 2
pairs=1:10,2:20
"""
}

test test_zip_result_length_is_available_to_format_interpolation { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [] -> Stream[Int] { yield 1 }

proc main() [io, error] {
  let pairs = numbers() |> zip([10])
  print f"pairs={pairs.len()}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pairs=1
"""
}

test test_last_min_max_live_terminals_finish_and_close_producers { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc close(name: Str) [io] { print f"closed {name}" }

stream numbers(name: Str) [io, error] -> Stream[Int] {
  defer close(name)
  for n in [3, 1, 2] {
    print f"{name} {n}"
    yield n
  }
}

proc main() [io, error] {
  let last = numbers("last") |> last()?
  print f"last={last}"
  let min = numbers("min") |> min()?
  print f"min={min}"
  let max = numbers("max") |> max()?
  print f"max={max}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """last 3
last 1
last 2
closed last
last=2
min 3
min 1
min 2
closed min
min=1
max 3
max 1
max 2
closed max
max=3
"""
}

test test_terminal_each_as_final_proc_statement_returns_unit { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """one
two
three
"""
  assert output.stderr == ""
}

test test_each_live_source_runs_body_before_next_pull { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull {n}"
    yield n
  }
}

proc main() [io] {
  numbers() |> each { |n| print f"each {n}" }
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
each 0
pull 1
each 1
pull 2
each 2
"""
}

test test_each_error_stops_and_closes_live_source { |ctx|
  let pulled = test.temp_path(ctx, name: "each-pulled")
  let closed = test.temp_path(ctx, name: "each-closed")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(pulled: Path, closed: Path) [fs, error] -> Stream[Int] {{
  defer closed.write("closed")?
  for n in range(5) {{
    pulled.write(f"pull {{n}}")?
    yield n
  }}
}}

proc main() [fs, error] {{
  numbers(Path("{pulled}"), Path("{closed}"))
    |> each {{ |n| let _ = 10 / (1 - n) }}
}}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stdout
    assert assertion_condition, assertion_message
  }
  assert pulled.read_text()? == "pull 1"
  assert closed.read_text()? == "closed"
}

test test_if_else_is_a_stream_stage_tail_value {
  let mapped = [1, 2, 3]
    |> map { |n|
      if n % 2 == 0 {
        "even"
      } else {
        "odd"
      }
    }
  assert mapped == ["odd", "even", "odd"]

  let filtered = [1, 2, 3]
    |> where { |n|
      if n > 1 {
        true
      } else {
        false
      }
    }
  assert filtered == [2, 3]

  let _ = [1, 2]
    |> each { |n|
      if n > 1 {
        let _ = n
      } else {
        let _ = n
      }
    }
}

test test_predicate_stage_blocks_bind_local_lets {
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
  assert filtered_block == filtered_expr
  assert filtered_block == [2, 4, 6]

  let any_block = nums
    |> any { |n|
      let twice = n * 2
      twice > 8
    }
  let any_expr = nums
    |> any { |n|
      n * 2 > 8
    }
  assert any_block == any_expr
  assert any_block

  let all_block = nums
    |> all { |n|
      let rem = n % 2
      rem == 0
    }
  let all_expr = nums
    |> all { |n|
      n % 2 == 0
    }
  assert all_block == all_expr
  assert ! all_block
  assert [2, 4, 6]
    |> all { |n|
      let rem = n % 2
      rem == 0
    }
}

test test_implicit_standard_read_helpers_and_pipe_shorthand { |ctx|
  let file = test.temp_file(ctx, name: "pipe-shorthand-input", contents: b"ok\nwarn one\nwarn two\n")?
  let file_text = fs.read_text(file)?
  let piped = file.read_bytes()?.utf8()?

  let warnings = piped
    |> text.lines
    |> where "warn" in .

  let names = [{path: "b"}, {path: "a"}]
    |> map .path
    |> sort

  assert file_text == piped
  assert warnings[0] == "warn one"
  assert warnings[1] == "warn two"
  assert names == ["a", "b"]
  assert cpu.count() > 0
}

test test_core_commands_and_byte_pipeline { |ctx|
  let root = test.temp_dir(ctx, name: "core-byte-pipeline")?
  let output = fp"{root}/out.txt"

  cd root {
    fs.write(p"inside.txt", "cwd")?
  }

  assert fp"{root}/inside.txt".read_text()? == "cwd"
  eprint "covered stderr"
  run printf "%s" "abc" | run tr a-z A-Z > output ?
  assert output.read_text()? == "ABC"
}

test test_reduce_by_stream_aggregates {
  # `reduce-by` keeps one accumulator per key without group-by materialization.
  let nums = [1, 2, 3, 4, 5, 6]

  let agg = nums
    |> reduce-by(sum: true) { |n|
      {key: if n % 2 == 0 { "even" } else { "odd" }, value: {count: 1, total: n}}
    }

  let lo = nums
    |> reduce-by(min: true) { |n|
      {key: "all", value: n}
    }

  let hi = nums
    |> reduce-by(max: true) { |n|
      {key: "all", value: n}
    }

  assert (agg.get("even") ?? {count: 0, total: 0}) == {count: 3, total: 12}
  assert (agg.get("odd") ?? {count: 0, total: 0}) == {count: 3, total: 9}
  assert (lo.get("all") ?? 0) == 1
  assert (hi.get("all") ?? 0) == 6
}

test test_reduce_by_live_source_folds_each_item_before_next_pull { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull {n}"
    yield n
  }
}

proc observed(n: Int) [io] -> Int {
  print f"reduce {n}"
  return n
}

proc main() [io, error] {
  let groups = numbers()
    |> reduce-by(sum: true, jobs: 1) { |n|
      {key: "all", value: observed(n)}
    }
  print f"total={groups.get("all") ?? 0}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
reduce 0
pull 1
reduce 1
pull 2
reduce 2
total=3
"""
}

test test_reduce_by_error_stops_and_closes_live_source { |ctx|
  let pulled = test.temp_path(ctx, name: "reduce-by-pulled")
  let closed = test.temp_path(ctx, name: "reduce-by-closed")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(pulled: Path, closed: Path) [fs, error] -> Stream[Int] {{
  defer closed.write("closed")?
  for n in range(5) {{
    pulled.write(f"pull {{n}}")?
    yield n
  }}
}}

proc main() [fs, error] {{
  let groups = numbers(Path("{pulled}"), Path("{closed}"))
    |> reduce-by(sum: true) {{ |n|
      {{key: "all", value: 10 / (1 - n)}}
    }}
  print ${{groups.get("all") ?? 0}}
}}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stdout
    assert assertion_condition, assertion_message
  }
  assert pulled.read_text()? == "pull 1"
  assert closed.read_text()? == "closed"
}

test test_reduce_by_jobs_hint_preserves_results {
  # The accepted `--jobs` hint currently uses the same serial reducer.
  let nums = [0] |> range(0, 50000)

  let serial = nums
    |> reduce-by(sum: true) { |n|
      {key: if n % 3 == 0 { "a" } else if n % 3 == 1 { "b" } else { "c" }, value: {count: 1, total: n}}
    }

  let par = nums
    |> reduce-by(sum: true, jobs: 8) { |n|
      {key: if n % 3 == 0 { "a" } else if n % 3 == 1 { "b" } else { "c" }, value: {count: 1, total: n}}
    }

  for k in serial.keys() {
    assert (par.get(k) ?? {count: 0, total: 0}) == (serial.get(k) ?? {count: 0, total: 0})
  }

  assert par.keys().len() == 3

  assert (nums
    |> reduce-by(min: true, jobs: 8) { |n|
      {key: "all", value: n}
    }.get("all") ?? -1) == 0

  assert (nums
    |> reduce-by(max: true, jobs: 8) { |n|
      {key: "all", value: n}
    }.get("all") ?? -1) == 49999
}

test test_jobs_options_evaluate_once_before_stage { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc jobs(label: Str) [io] -> Int {
  print $label
  return 2
}

proc main() [io, error] {
  let reduced = [1, 2]
    |> reduce-by(sum: true, jobs: jobs("reduce")) { |n| {key: "all", value: n} }
  print f"total={reduced.get("all") ?? 0}"
  let from_workers = [1, 2]
    |> par-map(jobs: 2) { |n| n }
    |> reduce-by(sum: true, jobs: jobs("after-map")) { |n| {key: "all", value: n} }
  print f"worker-total={from_workers.get("all") ?? 0}"
  print "done"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """reduce
total=3
after-map
worker-total=3
done
"""
}

test test_serial_stages_reject_jobs_option { |ctx|
  for script in [
    "proc main() [io] { [1] |> each(jobs: 2) { |n| print $n } }",
    "proc main() [] { let _ = [1] |> group-by(jobs: 2) { |n| n } }",
    "proc main() [] { let _ = [1] |> count(jobs: 2) { |n| n } }",
    "proc main() [] { let _ = [1] |> count(jobs: 2) }",
  ] {
    let output = test.run_script(ctx, script)?
    {
      let assertion_condition = ! output.success
      let assertion_message = script
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = "check.arity" in output.stderr
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_reduce_by_jobs_rejects_dynamic_zero_before_pulling_source { |ctx|
  let evaluated = test.temp_path(ctx, name: "jobs-evaluated")
  let pulled = test.temp_path(ctx, name: "jobs-source-pulled")
  let output = test.run_script(
    ctx,
    f"""
proc zero_jobs(evaluated: Path) [fs, error] -> Int {{
  evaluated.write("evaluated")?
  return 0
}}

stream numbers(pulled: Path) [fs, error] -> Stream[Int] {{
  pulled.write("pulled")?
  yield 1
}}

proc main() [fs, error] {{
  let totals = numbers(Path("{pulled}"))
    |> reduce-by(sum: true, jobs: zero_jobs(Path("{evaluated}"))) {{ |n| {{key: "all", value: n}} }}
  print ${{totals.get("all") ?? 0}}
}}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "stream worker count must be positive" in output.stderr
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  assert evaluated.read_text()? == "evaluated"
  assert ! pulled.exists()?
}

test test_par_map_jobs_rejects_dynamic_zero { |ctx|
  let output = test.run_script(
    ctx,
    """
proc zero_jobs() [] -> Int { return 0 }

proc main() [error] {
  let values = [1, 2] |> par-map(jobs: zero_jobs()) { |n| n }
  print \${values.len()}
}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "stream worker count must be positive" in output.stderr
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
}

test test_reduce_by_jobs_rejects_static_zero_in_checker { |ctx|
  let output = test.run_script(
    ctx,
    """
proc main() [error] {
  let grouped = [1] |> reduce-by(sum: true, jobs: 0) { |n| {key: "all", value: n} }
  print \${(grouped.get("all") ?? 0)}
}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "check.stream-jobs" in output.stderr
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
}

test test_par_map_reduce_by_fuses_to_worker_aggregation {
  let nums = [0] |> range(0, 50000)

  let fused = nums
    |> par-map(jobs: 8) { |n|
      {
        bucket: if n % 4 == 0 { "a" } else if n % 4 == 1 { "b" } else if n % 4 == 2 { "c" } else { "d" },
        doubled: n * 2,
        count: 1,
      }
    }
    |> reduce-by(sum: true) { |row|
      {key: row.bucket, value: {count: row.count, total: row.doubled}}
    }

  let unfused = nums
    |> par-map(jobs: 8) { |n|
      {
        bucket: if n % 4 == 0 { "a" } else if n % 4 == 1 { "b" } else if n % 4 == 2 { "c" } else { "d" },
        doubled: n * 2,
        count: 1,
      }
    }
    |> reduce-by(sum: true, jobs: 1) { |row|
      {key: row.bucket, value: {count: row.count, total: row.doubled}}
    }

  for k in unfused.keys() {
    assert (fused.get(k) ?? {count: 0, total: 0}) == (unfused.get(k) ?? {count: 0, total: 0})
  }

  assert fused.keys().len() == 4
  assert (fused.get("a") ?? {count: 0, total: 0}) == {count: 12500, total: 624950000}
}

test test_flat_map_identity_reduce_by_matches_direct_rows {
  let nums = [0] |> range(0, 1000)

  let nested = nums
    |> par-map { |n|
      [{key: if n % 2 == 0 { "even" } else { "odd" }, count: 1, total: n}]
    }
    |> flat-map { |rows|
      rows
    }
    |> reduce-by(sum: true) { |row|
      {key: row.key, value: {count: row.count, total: row.total}}
    }

  let direct = nums
    |> par-map { |n|
      {key: if n % 2 == 0 { "even" } else { "odd" }, count: 1, total: n}
    }
    |> reduce-by(sum: true) { |row|
      {key: row.key, value: {count: row.count, total: row.total}}
    }

  assert (nested.get("even") ?? {count: 0, total: 0}) == (direct.get("even") ?? {count: 0, total: 0})
  assert (nested.get("odd") ?? {count: 0, total: 0}) == {count: 500, total: 250000}
}

test test_live_files_flat_map_reduce_by_matches_collected_rows { |ctx|
  let root = test.temp_dir(ctx, name: "live-stream-flat-map-reduce")?
  fp"{root}/nested".mkdir()?
  fp"{root}/a.txt".write("abc")?
  fp"{root}/nested/b.txt".write("de")?
  fp"{root}/nested/c.md".write("fghi")?

  let streamed = fs.files(root)
    |> par-map(jobs: 4) { |entry|
      [{ext: entry.ext, count: 1, size: entry.size}]
    }
    |> flat-map { |rows|
      rows
    }
    |> reduce-by(sum: true) { |row|
      {key: row.ext, value: {count: row.count, size: row.size}}
    }
  let collected_rows = fs.files(root) |> collect()
  let collected = collected_rows
    |> par-map(jobs: 4) { |entry|
      {ext: entry.ext, count: 1, size: entry.size}
    }
    |> reduce-by(sum: true) { |row|
      {key: row.ext, value: {count: row.count, size: row.size}}
    }
  assert streamed == collected
  assert (streamed.get("txt") ?? {count: 0, size: 0}) == {count: 2, size: 5}
  assert (streamed.get("md") ?? {count: 0, size: 0}) == {count: 1, size: 4}
}

test test_live_files_par_map_for_matches_collected_rows { |ctx|
  let root = test.temp_dir(ctx, name: "live-stream-par-map-for")?
  fp"{root}/nested".mkdir()?
  fp"{root}/a.txt".write("abc")?
  fp"{root}/nested/b.txt".write("de")?
  fp"{root}/nested/c.md".write("fghi")?

  var streamed_txt_count = 0
  var streamed_txt_size = 0
  var streamed_md_count = 0
  var streamed_md_size = 0
  for row in fs.files(root)
    |> par-map(jobs: 4) { |entry|
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

  let collected_rows = fs.files(root) |> collect()
  let collected = collected_rows
    |> par-map(jobs: 4) { |entry|
      {ext: entry.ext, count: 1, size: entry.size}
    }
    |> reduce-by(sum: true) { |row|
      {key: row.ext, value: {count: row.count, size: row.size}}
    }
  assert {count: streamed_txt_count, size: streamed_txt_size} == (collected.get("txt") ?? {count: 0, size: 0})
  assert {count: streamed_md_count, size: streamed_md_size} == (collected.get("md") ?? {count: 0, size: 0})
  assert {count: streamed_txt_count, size: streamed_txt_size} == {count: 2, size: 5}
  assert {count: streamed_md_count, size: streamed_md_size} == {count: 1, size: 4}
}

test test_par_map_filesystem_reads_preserve_all_results { |ctx|
  let root = test.temp_dir(ctx, name: "par-map-filesystem-reads")?
  for index in range(32) {
    fp"{root}/entry-{index}.txt".write(f"""{index}
""")?
  }

  let entries = fs.files(root, stat: false)? |> collect()
  let lengths = entries
    |> par-map(jobs: 8) { |entry|
      entry.path.read_text()?.count_chars()
    }
  assert lengths.len() == 32
  assert (lengths |> sum) == 86
}

test test_projected_reduce_by_sums_output_fields { |ctx|
  let output = test.run_script(
    ctx,
    """
let rows = [
  {key: "g", a: 1, b: 10},
  {key: "g", a: 2, b: 20},
  {key: "g", a: 3, b: 30},
]
let reduced = rows
  |> reduce-by(sum: true) { |row|
    {key: row.key, value: {x: row.a, y: row.b}}
  }
let g = reduced.get("g") ?? {x: 0, y: 0}
print f"x={g.x}"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """x=6
"""
}

test test_stream_producers_are_lazy_and_run_defers_on_stop { |ctx|
  let marker = test.temp_path(ctx, name: "stream-marker")
  let rows = test.temp_path(ctx, name: "stream-rows")
  let output = test.run_script(
    ctx,
    f"""
stream nums(marker: Path, rows: Path) [fs, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(5) {{
    rows.write(f"row {{n}}")?
    yield n
  }}
}}

let numbers = nums(Path("{marker}"), Path("{rows}"))
# The call did not run the body, so no row has been written yet, and the first
# pull stops at the first row: the defer runs and the later rows never do.
print ${{Path("{rows}").exists() ?}}
let first = numbers |> first()?
print ${{first}}
print ${{Path("{rows}").read_text() ?}}
print ${{Path("{marker}").read_text() ?}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """false
0
row 0
closed
"""
}

test test_any_and_all_stop_live_producer_after_decisive_item { |ctx|
  let marker = test.temp_path(ctx, name: "any-stop-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(5) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc main() [fs, io, error] {{
  print f"any={{numbers(Path("{marker}")) |> any . == 0}}"
  print Path("{marker}").read_text()?
  print f"all={{numbers(Path("{marker}")) |> all . < 0}}"
  print Path("{marker}").read_text()?
  let any_block = numbers(Path("{marker}")) |> any {{ |n|
    let matched = n == 0
    matched
  }}
  print f"any_block={{any_block}}"
  print Path("{marker}").read_text()?
  let all_block = numbers(Path("{marker}")) |> all {{ |n|
    let matched = n < 0
    matched
  }}
  print f"all_block={{all_block}}"
  print Path("{marker}").read_text()?
  print f"none={{numbers(Path("{marker}")) |> any . == 99}}"
  print f"all_true={{numbers(Path("{marker}")) |> all . < 5}}"
}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
any=true
closed
pull 0
all=false
closed
pull 0
any_block=true
closed
pull 0
all_block=false
closed
pull 0
pull 1
pull 2
pull 3
pull 4
none=false
pull 0
pull 1
pull 2
pull 3
pull 4
all_true=true
"""
}

test test_live_tee_where_take_stops_upstream_and_closes_producer { |ctx|
  let marker = test.temp_path(ctx, name: "tee-take-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(5) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc amount() [io] -> Int {{
  print "count"
  return 2
}}

proc main() [fs, io, error] {{
  let taken = numbers(Path("{marker}"))
    |> tee {{ |n| print f"tee {{n}}" }}
    |> where . % 2 == 0
    |> take(amount())
    |> collect()
  print f"rows={{taken.len()}} {{taken[0]}} {{taken[1]}}"
  print Path("{marker}").read_text()?
}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """count
pull 0
tee 0
pull 1
tee 1
pull 2
tee 2
rows=2 0 2
closed
"""
}

test test_live_serial_collect_runs_stages_in_item_order { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull {n}"
    yield n
  }
}

proc main() [io] {
  let rows = numbers()
    |> tee { |n| print f"first {n}" }
    |> tee { |n| print f"second {n}" }
    |> collect()
  print f"rows={rows.len()}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
first 0
second 0
pull 1
first 1
second 1
pull 2
first 2
second 2
rows=3
"""
}

test test_live_serial_expression_boundary_runs_stages_in_item_order { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(2) {
    print f"pull {n}"
    yield n
  }
}

proc main() [io] {
  let rows = numbers()
    |> tee { |n| print f"first {n}" }
    |> tee { |n| print f"second {n}" }
  for n in rows { print f"row {n}" }
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
first 0
second 0
pull 1
first 1
second 1
row 0
row 1
"""
}

test test_live_serial_for_interleaves_body_and_stops_producer { |ctx|
  let marker = test.temp_path(ctx, name: "serial-for-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(5) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc main() [fs, io, error] {{
  for n in numbers(Path("{marker}"))
    |> tee {{ |value| print f"tee {{value}}" }}
    |> where . % 2 == 0 {{
    print f"row {{n}}"
    if n == 2 {{ break }}
  }}
  print Path("{marker}").read_text()?
}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
tee 0
row 0
pull 1
tee 1
pull 2
tee 2
row 2
closed
"""
}

test test_live_serial_for_keeps_source_and_stage_state { |ctx|
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
    print f"row {row.index}:{row.value}"
    if row.index == 1 { continue }
    print f"kept {row.value}"
  }
}
""",
  )?
  assert output.stdout == """source
row 0:1
kept 1
row 1:11
row 2:2
kept 2
"""
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
}

test test_live_serial_for_take_closes_producer { |ctx|
  let marker = test.temp_path(ctx, name: "serial-for-take-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(4) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc main() [fs, io, error] {{
  for n in numbers(Path("{marker}")) |> take(2) {{
    print f"row {{n}}"
  }}
  print Path("{marker}").read_text()?
}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
row 0
pull 1
row 1
closed
"""
}

test test_raw_stream_for_continue_reaches_next_item { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull {n}"
    yield n
  }
}

proc main() [io] {
  for n in numbers() {
    if n == 1 { continue }
    print f"row {n}"
  }
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
row 0
pull 1
pull 2
row 2
"""
}

test test_returned_stream_delegates_rows_and_cleanup { |ctx|
  let marker = test.temp_path(ctx, name: "returned-stream-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(4) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc source(marker: Path) [fs, io, error] -> Stream[Int] {{
  print "source"
  return numbers(marker)
}}

proc main() [fs, io, error] {{
  for n in source(Path("{marker}")) {{
    print f"row {{n}}"
    if n == 1 {{ break }}
  }}
  print Path("{marker}").read_text()?
}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """source
pull 0
row 0
pull 1
row 1
closed
"""
}

test test_live_serial_for_error_closes_trace_and_producer { |ctx|
  let marker = test.temp_path(ctx, name: "serial-for-error-marker")
  let output = test.run_xsht_trace(
    ctx,
    f"""
stream words(marker: Path) [fs, error] -> Stream[Str] {{
  defer marker.write("closed")?
  yield "1"
  yield "bad"
}}

proc main() [fs, io, error] {{
  for n in words(Path("{marker}")) |> map .parse_int_decimal()? {{
    print f"row {{n}}"
  }}
}}
""",
    ["--trace", "--raw"],
  )?
  assert output.status == 3
  assert "parse-int: invalid integer `bad`" in output.stderr
  assert output.stderr.split("kind=stream.stage.enter", -1).len() == 2
  assert output.stderr.split("kind=stream.stage.exit", -1).len() == 2
  assert marker.read_text()? == "closed"
}

test test_live_flat_map_take_stops_within_expanded_row { |ctx|
  let marker = test.temp_path(ctx, name: "flat-map-take-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(4) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc main() [fs, io, error] {{
  let rows = numbers(Path("{marker}"))
    |> flat-map {{ |n|
      print f"expand {{n}}"
      [n, n + 10]
    }}
    |> tee {{ |n| print f"expanded {{n}}" }}
    |> take(3)
    |> collect()
  print f"rows={{rows[0]}} {{rows[1]}} {{rows[2]}}"
  print Path("{marker}").read_text()?
}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
expand 0
expanded 0
expanded 10
pull 1
expand 1
expanded 1
rows=0 10 1
closed
"""
}

test test_live_map_drop_and_where_block_keep_take_bounded { |ctx|
  let marker = test.temp_path(ctx, name: "map-take-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(5) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc main() [fs, io, error] {{
  let mapped = numbers(Path("{marker}"))
    |> map {{ |n| n + 1 }}
    |> drop(1)
    |> take(2)
    |> collect()
  print f"mapped={{mapped[0]}} {{mapped[1]}}"
  let filtered = numbers(Path("{marker}"))
    |> where {{ |n|
      let even = n % 2 == 0
      even
    }}
    |> take(2)
    |> collect()
  print f"filtered={{filtered[0]}} {{filtered[1]}}"
  let enumerated = numbers(Path("{marker}"))
    |> map {{ |n|
      let next = n + 1
      next
    }}
    |> enumerate
    |> take(2)
    |> collect()
  print f"enumerated={{enumerated[0].index}}:{{enumerated[0].value}} {{enumerated[1].index}}:{{enumerated[1].value}}"
  print Path("{marker}").read_text()?
}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
pull 1
pull 2
mapped=2 3
pull 0
pull 1
pull 2
filtered=0 2
pull 0
pull 1
enumerated=0:1 1:2
closed
"""
}

test test_live_bounded_map_error_closes_producer { |ctx|
  let marker = test.temp_path(ctx, name: "bounded-map-error-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, error] -> Stream[Int] {{
  defer marker.write("closed")?
  yield 1
  yield 2
}}

proc main() [io, fs, error] {{
  let taken = numbers(Path("{marker}"))
    |> map {{ |n| "bad".parse_int()? + n }}
    |> take(1)
    |> collect()
  print f"rows={{taken.len()}}"
}}
""",
  )?
  assert ! output.success
  assert "invalid" in output.stderr
  assert marker.read_text()? == "closed"
}

test test_live_tee_any_and_where_first_stop_upstream { |ctx|
  let marker = test.temp_path(ctx, name: "bounded-terminal-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, io, error] -> Stream[Int] {{
  defer marker.write("closed")?
  for n in range(5) {{
    print f"pull {{n}}"
    yield n
  }}
}}

proc main() [io, fs, error] {{
  let found = numbers(Path("{marker}"))
    |> tee {{ |n| print f"tee {{n}}" }}
    |> any . == 0
  print f"found={{found}}"
  print Path("{marker}").read_text()?
  let block_found = numbers(Path("{marker}"))
    |> tee {{ |n| print f"tee {{n}}" }}
    |> any {{ |n|
      let matched = n == 0
      matched
    }}
  print f"block_found={{block_found}}"
  print Path("{marker}").read_text()?
  let first = numbers(Path("{marker}"))
    |> where . % 2 == 1
    |> first()?
  print f"first={{first}}"
  print Path("{marker}").read_text()?
}}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 0
tee 0
found=true
closed
pull 0
tee 0
block_found=true
closed
pull 0
pull 1
first=1
closed
"""
}

test test_live_where_first_empty_preserves_error_and_cleanup { |ctx|
  let marker = test.temp_path(ctx, name: "first-empty-marker")
  let output = test.run_script(
    ctx,
    f"""
stream numbers(marker: Path) [fs, error] -> Stream[Int] {{
  defer marker.write("closed")?
  yield 1
  yield 2
}}
proc main() [fs, io, error] {{
  let first = numbers(Path("{marker}")) |> where . > 10 |> first()?
  print f"first={{first}}"
}}
""",
  )?
  assert ! output.success
  assert "empty-stream" in output.stderr
  assert marker.read_text()? == "closed"
}

test test_zero_argument_stream_producers_run_from_every_call_position { |ctx|
  # A call with no arguments reaches the frame engine's call decision without
  # walking an argument list, so a producer spelled that way has to be
  # recognized there too: a producer pushed as an ordinary frame runs its body
  # with nowhere for `yield` to report and fails as a function that did not
  # return.
  let output = test.run_script(
    ctx,
    f"""
stream once() [] -> Stream[Int] {{
  yield 1
  yield 2
}}

stream twice() [] -> Stream[Int] {{
  for item in once() {{
    yield item * 2
  }}
}}

proc total() [error] -> Int {{
  var sum = 0
  for item in once() {{
    sum = sum + item
  }}
  return sum
}}

var direct = 0
for item in once() {{
  direct = direct + item
}}
print f"direct={{direct}}"
let bound = once().collect()
print f"bound={{bound.len()}}"
print f"total={{total()}}"
let doubled = twice().collect()
print f"doubled={{doubled.len()}}"
let mapped = once() |> map {{ |item| item + 1 }} |> collect()
print f"mapped={{mapped.len()}}"
let first = once() |> first()
print f"first={{first ?? -1}}"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """direct=3
bound=2
total=3
doubled=2
mapped=2
first=1
"""
}

test test_count_and_group_by_preserve_large_group_counts_and_order {
  # group-by must preserve encounter order within each group.
  let nums = [0] |> range(0, 20000)

  let counts = nums
    |> count {
      if . % 2 == 0 {
        "even"
      } else {
        "odd"
      }
    }

  let groups = nums
    |> group-by { |n|
      n % 3
    }
    |> sort-by { |g|
      g.key
    }
    |> map { |g|
      g.items
    }

  assert (counts.get("even") ?? 0) == 10000
  assert (counts.get("odd") ?? 0) == 10000
  assert groups.len() == 3
  assert groups[0].len() == 6667
  assert groups[0][0] == 0
  assert groups[0][1] == 3
  assert groups[0][6666] == 19998
  assert groups[1].len() == 6667
  assert groups[1][6666] == 19999
  assert groups[2].len() == 6666
  assert groups[2][6665] == 19997
}

test test_stream_adapters_bridge_text_bytes_and_json_lines {
  let captured = run.text printf "%s\n" "a.txt" "b.log" ?

  let paths = captured
    |> text.lines
    |> map { |line|
      fp"{line}"
    }

  let chunks = b"abcde" |> bytes.chunks(2)

  let rows = """{"name":"a","size":1}
{"name":"b","size":2}
"""
  |> json.lines
  |> sort-by .size.require(Int)?

  let streamed = """{"name":"c","size":3}
""" |> json.stream

  let words = "one two".words()
  assert paths[0].ext == "txt"
  assert paths[1].name == "b.log"
  assert chunks[0] == b"ab"
  assert chunks[2] == b"e"
  assert rows[1].name == "b"
  assert rows[0].size == 1
  assert streamed[0].name == "c"
  assert words[1] == "two"
}

test test_line_methods_and_adapters_are_lazy_sources { |ctx|
  let root = test.temp_dir(ctx, name: "stream-lines")?
  let input = fp"{root}/input.txt"

  input.write("""alpha\r
beta
gamma
""")?

  assert input.lines()? |> first()? == "alpha"

  assert """one
two
""".lines()
  |> drop(1)
  |> first()? == "two"

  assert """red
blue
"""
  |> text.lines
  |> take(1)[0] == "red"

  assert """x
y
""".lines()
  .collect()[1] == "y"

  assert b"a\nb\n".lines().collect().len() == 2
  assert input.bytes_lines()?.collect()[1] == b"beta"
}

test test_terminal_newline_does_not_add_empty_line_for_round_trip {
  let lines = """a
b
""".lines()
  .collect()

  assert lines == ["a", "b"]
  assert f"""{lines.join("\n")}
""" == """a
b
"""
}

test test_flat_map_consumes_live_streams_returned_by_blocks { |ctx|
  let root = test.temp_dir(ctx, name: "flat-map-live-stream")?
  let left = fp"{root}/left.txt"
  let right = fp"{root}/right.txt"

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

  assert lines == ["a", "b", "c", "d"]
}

test test_flat_map_drains_one_nested_stream_before_next_outer_pull { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream outer() [io] -> Stream[Int] {
  for n in range(3) {
    print f"outer {n}"
    yield n
  }
}

stream inner(n: Int) [io] -> Stream[Int] {
  for offset in range(3) {
    print f"inner {n}:{offset}"
    yield n * 10 + offset
  }
}

proc main() [io, error] {
  let first = outer() |> flat-map { |n| inner(n) } |> first()?
  print f"first={first}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """outer 0
inner 0:0
inner 0:1
inner 0:2
first=0
"""
}

test test_sort_boundary_materializes_serial_prefix_before_key_projection { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in [2, 1, 3] {
    print f"pull {n}"
    yield n
  }
}

proc key(n: Int) [io] -> Int {
  print f"key {n}"
  return n
}

proc main() [io, error] {
  let rows = numbers()
    |> tee { |n| print f"tee {n}" }
    |> sort-by { |n| key(n) }
    |> take(1)
    |> collect()
  print f"row={rows[0]}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull 2
tee 2
pull 1
tee 1
pull 3
tee 3
key 2
key 1
key 3
row=1
"""
}

test test_sort_by_desc_option_error_precedes_live_source_pull { |ctx|
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
  let sorted = numbers() |> sort-by(desc: descending()) { |n| n }
  print "after"
}
""",
    ["--raw"],
  )?
  assert ! output.success
  assert output.stdout == """desc
"""
  assert "invalid" in output.stderr
  assert "kind=stream.stage.exit name=\"sort-by\"" in output.stderr
}

test test_sort_by_desc_reverses_sort_order {
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
    |> sort-by(desc: true) { |n|
      n
    }

  let words = ["banana", "apple", "cherry"]
    |> sort-by(desc: true) { |word|
      word
    }

  assert asc[0] == 1
  assert asc[7] == 9
  assert desc[0] == 9
  assert desc[7] == 1
  assert words[0] == "cherry"
  assert words[2] == "apple"
}

test test_sort_by_compound_record_keys_and_stability {
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

  assert direct == [{name: "a", count: 1}, {name: "c", count: 1}, {name: "b", count: 2}]

  # --desc reverses the compound comparison.
  let desc = records
    |> sort-by(desc: true) { |r|
      {c: r.count, n: r.name}
    }

  assert desc == [{name: "b", count: 2}, {name: "c", count: 1}, {name: "a", count: 1}]

  # The documented two-pass stable idiom matches the direct compound key.
  let two_pass = records
    |> sort-by { |r|
      r.name
    }
    |> sort-by { |r|
      r.count
    }

  assert two_pass == direct

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
  assert stable == repeats

  # Whole-record sort uses the same record ordering.
  let whole = records |> sort
  assert whole == direct
}

test test_sort_by_rejects_non_orderable_keys_at_runtime { |ctx|
  let failed = test.run_script(
    ctx,
    """
let rows = [{name: "b"}, {name: "a"}]
let values = map.empty().set("key", ["not-orderable"])
let key = (values.get("key") ?? 0)
let out = (rows) |> sort-by { |_| key } |> collect()
for r in out { print \${r.name} }
""",
  )?

  {
    let assertion_condition = ! failed.success
    let assertion_message = failed.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "sort-by" in failed.stderr
    let assertion_message = failed.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "List" in failed.stderr
    let assertion_message = failed.stderr
    assert assertion_condition, assertion_message
  }
}

test test_sort_by_map_accumulator_any_typed_fields {
  # The explicit dynamic value domain survives integer additions, and it
  # sorts only by a validated key.
  let counts: Map[Str, Any] = {}
  let keys = ["b", "a"]
  let acc = counts.set("a", 2).set("b", 1)

  let by_count = keys
    |> map { |k|
      {count: acc.get(k) ?? 0, ext: k}
    }
    |> sort-by .count.require(Int)?
  assert [row.ext for row in by_count] == ["b", "a"]
  assert [row.count.require(Int)? for row in by_count] == [1, 2]

  # The list-comprehension equivalent accepts and sorts identically.
  let by_count_comp = [{count: acc.get(k) ?? 0, ext: k} for k in keys] |> sort-by .count.require(Int)?
  assert by_count_comp == by_count
}

test test_structured_stream_batch_count_and_argv_limits {
  let by_count = [1, 2, 3, 4, 5] |> batch(count: 2)
  let by_size = [p"aaaa", p"bbbb", p"cccc"] |> batch(max_bytes: 10)
  assert by_count == [[1, 2], [3, 4], [5]]
  assert by_size[0] == [p"aaaa", p"bbbb"]
  assert by_size[1] == [p"cccc"]

  [p"one", p"two"]
    |> batch(max_argv: true)
    |> each { |files|
      run true @files ?
    }

  assert ([1]
    |> where false
    |> batch(count: 2)
    |> count()) == 0
}

test test_batch_max_bytes_error_stops_and_closes_live_source { |ctx|
  let output = test.run_xsht_trace(
    ctx,
    r"""
proc close() [io] { print "closed" }

stream paths() [io, error] -> Stream[Path] {
  defer close()
  for value in ["ok", "oversized", "after"] {
    print f"pull {value}"
    yield Path(value)
  }
}

proc main() [io, error] {
  let batches = paths() |> batch(max_bytes: 3)
  print "after"
}
""",
    ["--raw"],
  )?
  assert ! output.success
  assert output.stdout == """pull ok
pull oversized
closed
"""
  assert "batch item exceeds byte budget" in output.stderr
  assert "kind=stream.stage.exit name=\"batch\"" in output.stderr
}

test test_repeat_zero_does_not_pull_live_source { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [io] -> Stream[Int] {
  for n in range(3) {
    print f"pull {n}"
    yield n
  }
}

proc main() [io, error] {
  let rows = numbers() |> repeat(0)
  print f"rows={rows.len()}"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """rows=0
"""
}

test test_count_producer_error_runs_defer_and_closes_trace { |ctx|
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
  print f"counted={counted}"
}
""",
    ["--raw"],
  )?
  assert ! output.success
  assert output.stdout == """closed
"""
  assert "invalid integer `bad`" in output.stderr
  assert "kind=stream.stage.exit name=\"count\"" in output.stderr
}

test test_mapped_live_count_enters_terminal_before_source_error { |ctx|
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
  let counted = numbers() |> map { |n| n + 1 } |> where . > 0 |> count()
  print f"counted={counted}"
}
""",
    ["--raw"],
  )?
  assert ! output.success
  assert output.stdout == """closed
"""
  assert "invalid integer `bad`" in output.stderr
  for name in ["map", "where", "count"] {
    assert f"kind=stream.stage.enter name=\"{name}\"" in output.stderr
    assert f"kind=stream.stage.exit name=\"{name}\"" in output.stderr
  }
}

test test_live_serial_count_after_map_where_and_flat_map {
  let count = range(4)
    |> map { |n|
      n + 1
    }
    |> where . > 2
    |> flat-map { |n|
      [n, n]
    }
    |> count()
  assert count == 4
}

test test_parallel_stream_stages_are_bounded_and_deterministic {
  assert ([1, 2, 3, 4]
    |> par-map { |x|
      x * 2
    }) == [2, 4, 6, 8]

  var seen = []

  ["a", "b"]
    |> each { |x|
      seen += [x]
    }

  assert seen == ["a", "b"]
}

test test_each_trace_reports_serial_execution { |ctx|
  let trace = test.run_xsht_trace(
    ctx,
    r"""
proc main() [io] {
  [1, 2] |> each { |n| print f"item={n}" }
}
""",
    ["--trace", "--raw"],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = trace
    assert assertion_condition, assertion_message
  }
  assert trace.stdout == """item=1
item=2
"""
  assert "kind=stream.stage.enter name=\"each\"" in trace.stderr
  assert "kind=stream.stage.exit name=\"each\"" in trace.stderr
  assert "kind=parallel.job." not in trace.stderr
  assert "kind=parallel.cancel" not in trace.stderr
}

test test_parallel_stream_preserves_filtered_order {
  assert ([0, 1, 2, 3, 4, 5]
    |> where . >= 2
    |> par-map(jobs: 3) { |x|
      x * 10
    }) == [20, 30, 40, 50]
}

test test_structured_streams_walk_filter_map_collect_and_count { |ctx|
  let root = test.temp_dir(ctx, name: "stream-walk")?
  let nested = fp"{root}/nested"
  nested.mkdir()?
  fp"{root}/a.txt".write("a")?
  fp"{nested}/b.txt".write("b")?
  let entries = fs.files(root) |> collect()

  let names = entries
    |> map .name
    |> sort-by .

  let count = fs.files(root) |> count()
  assert names == ["a.txt", "b.txt"]
  assert count == 2
}

test test_direct_collect_of_lazy_module_stream_is_a_list { |ctx|
  let root = test.temp_dir(ctx, name: "stream-direct-collect")?
  fp"{root}/a.txt".write("a")?
  fp"{root}/b.txt".write("b")?
  fp"{root}/c.txt".write("c")?

  # A module-produced lazy stream piped straight into the collect terminal, with
  # no intervening transformation stage, must lower and run as a materialized
  # list (regression: the direct result was mis-typed as a stream, so `len` was
  # rejected and the pipeline failed to compile).
  let all = fs.files(root) |> collect()
  assert all.len() == 3
}

test test_table_print_wraps_cells_to_terminal_width { |ctx|
  let output = test.run_script(
    ctx,
    """
let rows = [{name: "very-long-command-name-that-keeps-going", size: 123}]
rows |> table.print(columns: ["name", "size"])
""",
    [],
    {COLUMNS: "40"},
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert "…" not in output.stdout
  assert "very-long-command-name-that-k" in output.stdout
  assert "eeps-going" in output.stdout

  for line in output.stdout.lines() {
    {
      let assertion_condition = line.count_chars() <= 40
      let assertion_message = line
      assert assertion_condition, assertion_message
    }
  }
}

test test_stream_stages_are_trace_observable { |ctx|
  let table_trace = test.run_xsht_trace(
    ctx,
    """
let rows = [{name: "b", size: 2}, {name: "a", size: 1}]
(rows) |> sort-by { |row| row.size } |> table.print(columns: ["name", "size"])
""",
    ["--trace", "--raw"],
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = table_trace
    assert assertion_condition, assertion_message
  }
  assert "│ a" in table_trace.stdout
  assert "│ b" in table_trace.stdout
  assert "kind=stream.stage.enter" in table_trace.stderr
  assert "kind=stream.stage.exit" in table_trace.stderr
  assert "name=\"sort-by\"" in table_trace.stderr
  assert "stage=b\"sort-by\"" in table_trace.stderr
  assert "name=\"table.print\"" in table_trace.stderr
  assert "stage=b\"table.print\"" in table_trace.stderr
  assert "item_count=2" in table_trace.stderr

  let bounded_trace = test.run_xsht_trace(
    ctx,
    r"""
stream numbers() [] -> Stream[Int] {
  yield 1
  yield 2
}
proc main() [io, error] {
  let taken = numbers() |> tee { |n| print f"seen={n}" } |> where . > 0 |> take(1) |> collect()
  print f"count={taken.len()}"
}
""",
    ["--trace", "--raw"],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = bounded_trace
    assert assertion_condition, assertion_message
  }
  assert bounded_trace.stdout == """seen=1
count=1
"""
  for name in ["tee", "where", "take"] {
    assert f"name=\"{name}\"" in bounded_trace.stderr
    assert f"stage=b\"{name}\"" in bounded_trace.stderr
  }

  let adapter_trace = test.run_xsht_trace(
    ctx,
    """let _ = "a\\nb\\n" |> text.lines()
""",
    ["--raw"],
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = adapter_trace
    assert assertion_condition, assertion_message
  }
  assert "kind=stream.stage.enter" in adapter_trace.stderr
  assert "kind=stream.stage.exit" in adapter_trace.stderr
  assert "name=\"text.lines\"" in adapter_trace.stderr
  assert "stage=b\"text.lines\"" in adapter_trace.stderr

  let batch_trace = test.run_xsht_trace(
    ctx,
    """let _ = [1, 2, 3] |> batch(count: 2)
""",
    ["--raw"],
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = batch_trace
    assert assertion_condition, assertion_message
  }
  assert "kind=stream.stage.enter" in batch_trace.stderr
  assert "kind=stream.stage.exit" in batch_trace.stderr
  assert "name=\"batch\"" in batch_trace.stderr
  assert "stage=b\"batch\"" in batch_trace.stderr
  assert "item_count=3" in batch_trace.stderr
}

test test_stream_errors_include_trace_context { |ctx|
  let stream_error = test.run_xsht_trace(
    ctx,
    """
let xs = ["only"]
let values = [1] |> map { |index| xs[index] }
print \${values[0]}
""",
    ["--trace", "--raw"],
  )?

  assert stream_error.status == 3
  assert "stream stage `map` item 0 failed" in stream_error.stderr
  assert "index-out-of-range" in stream_error.stderr
  assert "kind=stream.item.error" in stream_error.stderr
  assert "item_index=0" in stream_error.stderr

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
  assert live_error.status == 3
  assert "kind=stream.stage.enter" in live_error.stderr
  assert "kind=stream.stage.exit name=\"take\"" in live_error.stderr
  assert "kind=stream.stage.exit name=\"map\"" in live_error.stderr
  assert "invalid" in live_error.stderr

  let par_error = test.run_xsht_trace(
    ctx,
    """
let xs = ["only"]
let values = [1, 2, 3] |> par-map(jobs: 1) { |index| xs[index] }
""",
    ["--trace", "--raw"],
  )?

  assert par_error.status == 3
  assert "stream stage `par-map` item 0 failed" in par_error.stderr
  assert "index-out-of-range" in par_error.stderr
  assert "kind=parallel.job.start" in par_error.stderr
  assert "kind=parallel.job.end" in par_error.stderr
  assert "item_index=0" in par_error.stderr

  let idle_error = test.run_xsht_trace(
    ctx,
    """
let xs = ["only"]
let values = [1, 2, 3] |> par-map(jobs: 8) { |index| xs[index] }
""",
    ["--trace", "--raw"],
  )?

  assert idle_error.status == 3
  assert "stream stage `par-map` item 0 failed" in idle_error.stderr
  assert "index-out-of-range" in idle_error.stderr
}

test test_fs_files_lazy_folding_terminals_match_eager_results { |ctx|
  # count/sum/min/max/fold drive the live stream by folding one item at a time.
  let root = test.temp_dir(ctx, name: "fs-walk-fold")?
  fp"{root}/a.txt".write("a")?
  fp"{root}/bb.txt".write("bb")?
  fp"{root}/ccc.txt".write("ccc")?
  assert (fs.files(root) |> count()) == 3

  assert (fs.files(root)
    |> map .size
    |> sum) == 6

  assert fs.files(root)
    |> map .size
    |> min? == 1

  assert fs.files(root)
    |> map .size
    |> max? == 3

  assert (fs.files(root)
    |> map .size
    |> fold(0) { |acc|
      acc + .
    }) == 6
}

test test_keyed_count_result_retains_map_type_through_later_pipelines { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let stats = ["rs", "md", "rs"] |> count { |ext| ext }
let counts = stats.keys()
  |> map { |ext| {count: stats.get(ext) ?? 0, ext: ext} }
  |> sort-by .count
for row in counts { print f"{row.ext}:{row.count}" }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """md:1
rs:2
"""
}

test test_pipeline_stage_facts_retain_terminal_and_callback_types_in_procedures { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc summarize() [error] {
  let folded = [1, 2] |> fold(0) { |acc, value| acc + value }
  let grouped = ["a", "a"] |> group-by { |value| value }
  let sizes = grouped |> map { |row| row.items.len() }
  let batched = [1, 2] |> batch(count: 2) |> map { |items| items.len() }
  print f"{folded.float()}:{sizes[0]}:{batched[0]}"
}
summarize()
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """3:2:2
"""
}

type StreamLabelRow = {name: Str, unit: Str}

test test_stream_where_string_views_match_ordinary_record_comparison {
  let source = """ MemFree :bytes
VendorCounter:widgets
"""
  var viewed: List[StreamLabelRow] = []
  for line in source.lines() {
    let fields = line.split(":")
    viewed += [{name: fields[0].trim(), unit: fields[1]}]
  }

  let owned: List[StreamLabelRow] = [{name: "MemFree", unit: "bytes"}, {name: "VendorCounter", unit: "widgets"}]
  assert viewed[0].name == "MemFree"
  assert viewed[1].name != "MemFree"
  assert (viewed |> where .name == "MemFree").len() == 1
  assert (viewed |> where "MemFree" == .name).len() == 1
  assert (viewed |> where .name != "MemFree").len() == 1
  assert (viewed |> where "MemFree" != .name).len() == 1
  assert (viewed |> where .name == "MemFree" and .unit == "bytes").len() == 1
  assert (viewed |> where .name == "MemFree" or .unit == "widgets").len() == 2
  assert (viewed |> where .name == "MemFree") == (owned |> where .name == "MemFree")
  assert (viewed |> where .name != "MemFree") == (owned |> where .name != "MemFree")
  assert (viewed |> where .name == "MemFree") == (viewed
    |> where { |row|
      row.name == "MemFree"
    })
}

# Breaking out of the loop stops the producer while its body is suspended
# inside a context-scope block that is the scrutinee of a match; stopping
# runs the defers and restores the scope without finishing the match.
test test_stream_break_inside_context_scope_match_stops_cleanly { |ctx|
  let marker = test.temp_path(ctx, name: "ctx-match-marker")
  let output = test.run_script(
    ctx,
    f"""
enum E1 {{ E1V0, E1V1 }}
stream numbers(marker: Path) [fs, error] -> Stream[Int] {{
  defer marker.write("closed")?
  match ctx "stream" {{
    yield 1
    E1V1
  }} {{
    E1V0 => {{}}
    E1V1 => {{}}
  }}
  yield 2
}}
for value in numbers(Path("{marker}")) {{
  print f"value={{value}}"
  break
}}
print Path("{marker}").read_text()?
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """value=1
closed
"""
}

# An empty producer body is the degenerate empty stream: consuming it yields
# no items and completes without error.
test test_stream_empty_body_yields_nothing { |ctx|
  let output = test.run_script(
    ctx,
    """stream empty() [] -> Stream[Int] { }
for value in empty() {
  print f"item {value}"
}
print "done"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
}
