# `for index, item in source` binds each item's position beside the item. It
# is sugar for a `for` over `source |> enumerate()` with a record target.

type Entry = {name: Str, size: Int}

const entries: List[Entry] = [{name: "a", size: 1}, {name: "b", size: 2}]

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure weighted(values: List[Int]) -> Int {
  var sum = 0
  for i, value in values {
    sum += i * value
  }

  sum
}

test test_for_index_counts_items_from_zero {
  assert weighted([5, 6, 7]) == 20
  assert weighted([]) == 0

  let seen = collect {
    for i, {name, size} in entries {
      yield f"{i}:{name}:{size}"
    }
  }

  assert seen == ["0:a:1", "1:b:2"]
}

test test_for_index_numbers_the_items_a_stream_yields {
  let seen = collect {
    for index, n in range(9) |> where . % 2 == 0 |> where . > 0 {
      continue when index == 0
      break when index == 3
      yield f"{index}:{n}"
    }
  }

  assert seen == ["1:4", "2:6"]
}

test test_for_index_nests_and_keeps_each_loop_its_own_index {
  let cells = collect {
    for i, row in [[1, 2], [3]] {
      for j, cell in row {
        yield f"{i}.{j}={cell}"
      }
    }
  }

  assert cells == ["0.0=1", "0.1=2", "1.0=3"]

  let positions = collect {
    for i, _ in ["x", "y"] {
      yield i
    }
  }

  assert positions == [0, 1]
}

# The source is evaluated once and the loop sees a snapshot of it.
test test_for_index_iterates_a_snapshot_of_its_source {
  var items = ["a", "b"]
  var seen = []
  for i, item in items {
    items += [item + "!"]
    seen += [f"{i}{item}"]
  }

  assert seen == ["0a", "1b"]
  assert items == ["a", "b", "a!", "b!"]
}

test test_for_index_bindings_are_immutable_and_end_with_the_loop { |ctx|
  let stderr = check_errors(
    ctx,
    """for i, item in ["a"] {
  i += 1
  item = "b"
}
print \$i
""",
  )?
  assert count(stderr, "err[") == 3, stderr
  assert ":2:" in stderr, stderr
  assert ":3:" in stderr, stderr
  assert ":5:" in stderr, stderr
}

# The source is the input of a pipeline, so only a list or a stream has an
# index form, and the report is on the source the user wrote.
test test_for_index_requires_a_list_or_stream_source { |ctx|
  let stderr = check_errors(
    ctx,
    """for i, char in "abc" {
  print \$i \$char
}
for i, entry in {a: 1} {
  print \$i \${entry.key}
}
""",
  )?
  assert count(stderr, "err[check.stream-input]") == 2, stderr
  assert count(stderr, "err[") == 2, stderr
  assert ":1:16" in stderr, stderr
  assert ":4:17" in stderr, stderr
}

test test_for_index_takes_a_name_as_its_index { |ctx|
  let stderr = check_errors(
    ctx,
    """for {index}, item in ["a"] {
  print \$index \$item
}
""",
  )?
  assert "err[parse.for-index]" in stderr, stderr
  assert "only the item may be destructured" in stderr, stderr
  assert ":1:5" in stderr, stderr
}

test test_for_index_formats_and_desugars_as_specified { |ctx|
  let source = """type Entry = {name: Str, size: Int}
const entries: List[Entry] = [{name: "a", size: 1}]
for   i ,  {name,size}   in entries {
  print \$i \$name \$size
}
"""
  let candidate = test.temp_file(ctx, name: "for-index.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let text = candidate.read_text()?
  assert "\nfor i, {name, size} in entries {\n" in text, text
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr

  let desugared = run.capture --text "xsht" desugar $candidate
  assert desugared.status.exited_with(0), desugared.stderr
  assert "\nfor {index: i, value: {name, size}} in entries |> enumerate() {\n" in desugared.stdout, desugared.stdout

  let sugar = test.run_script(ctx, text)?
  let core = test.run_script(ctx, desugared.stdout)?
  assert sugar.success, sugar.stderr
  assert core.success, core.stderr
  assert sugar.stdout == "0 a 1\n"
  assert core.stdout == sugar.stdout
}

# The migration lint rewrites a counter that only walks a list, and leaves a
# loop alone when a `for` would not be the same program.
test test_lint_rewrites_counter_loops_that_only_walk_a_list { |ctx|
  let source = """proc show(names: List[Str]) [io] {
  var i = 0
  while i < names.len() {
    let name = names[i]
    print f"{i}: {name}"
    i += 1
  }

  var at = 1

  var shown = 0

  while at < names.len() {
    let name = names[at]

    shown += 1
    print \$name \$shown

    at += 1
  }

  var seen = 0
  while seen < names.len() {
    let name = names[seen]
    print \$name
    seen += 1
  }

  print \$seen
}

show(["a", "b"])
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "counter-loops.xsh", contents: bytes.from_text(source))?

  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-for-index $candidate
  let report = linted.stdout + linted.stderr
  assert count(report, "warn[lint.prefer-for-index]") == 2, report
  assert "write `for i, name in names { ... }`" in report, report
  assert "write `for name in names[1..] { ... }`" in report, report

  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-for-index $candidate
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert "  for i, name in names {\n    print f\"{i}: {name}\"\n  }\n" in rewritten, rewritten
  assert "  }\n\n  var shown = 0\n\n  for name in names[1..] {\n    shown += 1\n    print $name $shown\n  }\n" in rewritten, rewritten
  assert "  while seen < names.len() {\n" in rewritten, rewritten
  assert "var i = 0" not in rewritten, rewritten
  assert "var at = 1" not in rewritten, rewritten

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
  assert after.stdout == "0: a\n1: b\nb 1\na\nb\n2\n"
}

# `xsht grep` takes a loop head as a pattern. A head matches the loops written
# with the same number of bindings, and never the expansion of another form.
test test_grep_finds_loops_by_their_head { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "for-grep.xsh",
    contents: bytes.from_text("""type Entry = {name: Str, size: Int}
const entries: List[Entry] = [{name: "a", size: 1}]
for entry in entries {
  print \$entry.name
}
for i, {name, size} in entries {
  print \$i \$name \$size
}
for row, n in range(3) {
  print \$row \$n
}
repeat 2 times {
  print "tick"
}
"""),
  )?
  let plain = run.capture --text "xsht" grep "for NAME in ITER" $candidate
  assert plain.stdout == f"{candidate}:3:for entry in entries {{\n1 match\n", plain.stdout

  let indexed = run.capture --text "xsht" grep "for INDEX, NAME in ITER" $candidate
  assert ":6:for i, {name, size} in entries {" in indexed.stdout, indexed.stdout
  assert ":9:for row, n in range(3) {" in indexed.stdout, indexed.stdout
  assert indexed.stdout.ends_with("2 matches\n"), indexed.stdout

  # A lowercase name matches only itself, and the source is an expression
  # pattern like any other.
  let named = run.capture --text "xsht" grep "for row, NAME in range(COUNT)" $candidate
  assert named.stdout == f"{candidate}:9:for row, n in range(3) {{\n1 match\n", named.stdout

  let rejected = run.capture --text --accept=[2] "xsht" grep "for {name}, ITEM in ITER" $candidate
  assert "failed to parse pattern" in rejected.stderr, rejected.stderr
}
