type Row = {name: Str, size: Int}

pure sample_rows() -> List[Row] {
  [{name: "a", size: 1}, {name: "b", size: 0}, {name: "c", size: 2}]
}

test test_collect_gathers_yields_in_the_order_they_run {
  let squares = collect {
    yield 0
    for n in [1, 2, 3, 4] {
      yield n * n when n % 2 == 0
    }

    yield @[7, 8]
    if squares_enabled() {
      yield 100
    } else {
      yield 200
    }
  }
  assert squares == [0, 4, 16, 7, 8, 100]

  # Through `match`, `unless`, `while`, and a bare block.
  var index = 0
  let rows = sample_rows()
  let names = collect {
    while index < rows.len() {
      let row = rows[index]
      index += 1
      {
        yield "row" unless index > 1
      }
      match row.size {
        0 => continue
        else => yield row.name
      }
    }
  }
  assert names == ["row", "a", "c"]
}

pure squares_enabled() -> Bool {
  true
}

test test_collect_infers_its_item_type {
  # From the yields, as a list literal does from its elements.
  let pairs = collect {
    for row in sample_rows() {
      yield {label: row.name, big: row.size > 1}
    }
  }
  assert pairs[2].label == "c"
  assert pairs[2].big

  # From the expected type, which an empty block needs.
  let none: List[Int] = collect {}
  assert none.is_empty()
  let typed: List[Row] = collect {
    yield {name: "typed", size: 3}
  }
  assert typed[0].size == 3

  # A `Result` item stays a value.
  let results = collect {
    yield Ok(1)
    yield Err(error.failure("no"))
  }
  assert results.len() == 2
  assert results[0] is Ok(1)
  assert results[1] is Err(_)
}

pure enabled_names(rows: List[Row]) -> List[Str] {
  collect {
    for row in rows {
      continue unless row.size > 0
      yield row.name
    }
  }
}

test test_collect_is_a_value_in_tail_and_argument_position {
  assert enabled_names(sample_rows()) == ["a", "c"]
  assert enabled_names([]) == []
  assert collect {
    yield 1
    yield 2
  }.len() == 2
  let total = collect {
    for row in sample_rows() {
      yield row.size
    }
  } |> fold(0) { |sum, size| sum + size }
  assert total == 3
}

proc first_sizes(rows: List[Row], limit: Int) -> List[Int] {
  let sizes = collect {
    for row in rows {
      return [-1] when limit < 0
      break when row.size >= limit
      yield row.size
    }
  }
  sizes
}

proc checked_sizes(rows: List[Row]) -> Result[List[Int]] {
  let sizes = collect {
    for row in rows {
      fail f"{row.name} is empty" when row.size == 0
      yield row.size
    }
  }
  Ok(sizes)
}

# The block is no boundary: control flow and failures pass through it.
test test_collect_lets_control_flow_and_failures_through {
  assert first_sizes(sample_rows(), 2) == [1, 0]
  assert first_sizes(sample_rows(), 0) == []
  assert first_sizes(sample_rows(), -1) == [-1]

  match checked_sizes(sample_rows()) {
    Ok(_) => assert false
    Err(failure) => assert failure.message == "b is empty"
  }

  assert checked_sizes([{name: "x", size: 4}]) == Ok([4])

  # A `break` leaves the loop around the expression, and no list is bound.
  var bound = 0
  for round in [1, 2, 3] {
    let got = collect {
      yield round
      break when round == 2
    }
    bound += got.len()
  }

  assert bound == 1
}

test test_collect_nests_and_each_block_has_its_own_list {
  let table = collect {
    for n in [1, 2] {
      yield collect {
        yield n
        yield n + 10
      }
    }
  }
  assert table == [[1, 11], [2, 12]]

  # A called function's block does not share the caller's.
  let outer = collect {
    yield "before"
    yield @enabled_names(sample_rows())
    yield "after"
  }
  assert outer == ["before", "a", "c", "after"]

  # Each evaluation starts from an empty list.
  var lengths = []
  for limit in [1, 2] {
    let got = collect {
      for n in range(limit) {
        yield n
      }
    }
    lengths += [got.len()]
  }

  assert lengths == [1, 2]
}

# In a producer, the yields of a `collect` block append and emit nothing.
test test_collect_inside_a_producer_keeps_its_yields { |ctx|
  let output = test.expect(
    ctx,
    """stream pairs_of(items: List[Int]) -> Stream[List[Int]] {
  for item in items {
    yield collect {
      yield item
      yield item * 2
    }
  }
}

stream counted(items: List[Int]) -> Stream[Int] {
  let doubled = collect {
    for item in items {
      yield item * 2
    }
  }
  yield doubled.len()
  yield @doubled
}

assert pairs_of([1, 2]).collect() == [[1, 2], [2, 4]]
assert counted([3, 4]).collect() == [2, 6, 8]

# Pulling a stream inside the block, one item at a time.
var pulls = 0
let pulled = collect {
  for pair in pairs_of([5, 6]) {
    pulls += 1
    yield @pair
  }
}
assert pulled == [5, 10, 6, 12]
print f"pulls {pulls}"
""",
    status: 0,
  )?
  assert output.stdout == "pulls 2\n"
}

test test_collect_runs_cleanup_and_scopes_inside_the_block {
  var log = []
  let got = collect {
    defer { log += ["closed"] }
    yield 1
    log += ["yielded"]
  }
  assert got == [1]
  assert log == ["yielded", "closed"]

  # A `yield` in a `within` body in the block is the block's.
  let timed = collect {
    let waited = within 30s {
      yield 1
      2
    }
    yield waited ?? 0
  }
  assert timed == [1, 2]

  # Each attempt of a `retry` around the block builds a list of its own.
  var attempts = 0
  let retried = retry [0ms, 0ms] {
    attempts += 1
    let items = collect {
      yield attempts
      assert attempts >= 2
      yield attempts * 10
    }
    items
  }
  assert retried == Ok([2, 20])
}

test test_collect_is_contextual_and_checked { |ctx|
  let rows = [3, 1, 2]
  assert (rows |> sort |> collect()) == [1, 2, 3]
  assert rows.collect() == rows
  let collect = 2
  assert collect + 1 == 3

  let checks = [
    {
      source: "let nothing = collect { print \"x\" }\n",
      wants: "check.collect-item",
    },
    {
      source: "let mixed = collect {\n  yield 1\n  yield \"two\"\n}\n",
      wants: "check.type-mismatch",
    },
    {
      source: "let typed: List[Int] = collect {\n  yield \"one\"\n}\n",
      wants: "check.type-mismatch",
    },
    {
      source: "let sizes: List[Int] = collect {\n  let seen = [1, 2] |> map {\n    yield .\n    . + 1\n  }\n  yield seen.len()\n}\n",
      wants: "`yield` is valid only in stream producers",
    },
    {
      source: "let again = collect {\n  let r = retry [] {\n    yield 1\n    2\n  }\n}\n",
      wants: "`yield` is not allowed inside a retry attempt",
    },
    {
      source: "stream items() -> Stream[Int] {\n  yield 1\n}\nlet pulled = collect {\n  yield @items()\n}\n",
      wants: "collect the stream first with `.collect()`",
    },
    {
      source: "stream items() -> Stream[Int] {\n  yield 1\n}\nlet held: List[Int] = collect {\n  yield items()\n}\n",
      wants: "check.yield-stream",
    },
    {
      source: "let flat = collect {\n  yield @3\n}\n",
      wants: "check.yield-delegation",
    },
    {
      source: "let late = collect {\n  defer { yield 1 }\n  yield 2\n}\n",
      wants: "`yield` is not allowed in a deferred cleanup block",
    },
    {
      source: "let collect = true\nif collect { print \"x\" }\n",
      wants: "err[parse.",
    },
  ]
  for check in checks {
    let output = test.run_script(ctx, check.source)?
    assert output.status != 0, check.source
    assert check.wants in output.stderr, f"{check.source}: {output.stderr}"
    assert output.stdout == "", check.source
  }
}

test test_collect_formats_stably_and_is_rewritten_by_its_lint { |ctx|
  let source = """let squares = collect{
  for n in [1, 2] {
        yield n * n
  }
}
let count = collect   { yield 1 }.len()
"""
  let candidate = test.temp_file(ctx, name: "collect.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == """let squares = collect {
  for n in [1, 2] {
    yield n * n
  }
}
let count = collect { yield 1 }.len()
"""
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr

  let appended = r"""proc loud(sizes: List[Int]) -> List[Int] {
  var kept = []

  for size in sizes {
    print $size
    kept = kept.push(size * 2)
  }

  kept
}

print f"{loud([1, 2]).len()}"
"""
  let built = test.temp_file(ctx, name: "built.xsh", contents: bytes.from_text(appended))?
  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-collect $built
  assert "warn[lint.prefer-collect]" in linted.stdout + linted.stderr, linted.stderr
  let fixed = run.capture --text --accept=[0, 1] "xsht" lint --fix --only lint.prefer-collect $built
  assert fixed.status.exited_with(0), fixed.stdout + fixed.stderr
  assert built.read_text()? == r"""proc loud(sizes: List[Int]) -> List[Int] {
  let kept = collect {
    for size in sizes {
      print $size
      yield size * 2
    }
  }

  kept
}

print f"{loud([1, 2]).len()}"
"""
  let ran = run.capture --text "xsh" $built
  assert ran.stdout == "1\n2\n2\n", ran.stderr
}
