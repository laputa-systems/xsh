type Entry = {name: Str, score: Int}

test test_list_comprehension_basic_transform {
  let nums = [1, 2, 3]
  let doubled = [x * 2 for x in nums]
  assert doubled == [2, 4, 6]
}

test test_list_comprehension_with_guard_filters_elements {
  let nums = [1, 2, 3, 4, 5]
  let evens = [x for x in nums if x % 2 == 0]
  assert evens == [2, 4]
}

test test_list_comprehension_guard_can_produce_empty_list {
  let nums = [1, 3, 5]
  let evens = [x for x in nums if x % 2 == 0]
  assert (evens |> count()) == 0
}

test test_list_comprehension_with_record_destructuring {
  let entries: List[Entry] = [{name: "alice", score: 90}, {name: "bob", score: 55}, {name: "carol", score: 80}]
  let passing = [name for {name, score} in entries if score >= 60]
  assert passing == ["alice", "carol"]
}

error FsError = NotFound(file: Path) : NotFound | PermissionDenied(file: Path, op: Str) : PermissionDenied

proc missing(file: Path) [error] -> Result[Str, FsError] {
  Err(FsError.NotFound(file:))
}

test test_nominal_error_payload_and_facet_patterns {
  match missing(p"missing") {
    Ok(text) => test.fail(f"unexpected ok {text}")?
    Err(FsError.NotFound {file: file}) => assert file.display() == "missing"
    Err(is PermissionDenied) => test.fail("unexpected permission facet")?
    Err(error) => test.fail(error.message)?
  }
}

type Stats = {blanks: Int = 0, code: Int = 0, comments: Int = 0}

pure count_lines(lines: List[Str]) -> Stats {
  var stats: Stats = Stats()

  for line in lines {
    if line.trim() == "" {
      stats.blanks += 1
    } else if line.starts_with("#") {
      stats.comments += 1
    } else {
      stats.code += 1
    }
  }

  stats
}

test test_local_accumulator_field_mutation {
  let stats = count_lines(["alpha", "", "# note", "beta"])
  var counts: Map[Int] = {}
  counts["code"] = stats.code
  counts["comments"] = stats.comments
  assert stats.blanks == 1
  assert (counts.get("code") ?? 0) == 2
  assert (counts.get("comments") ?? 0) == 1
}

test test_compact_sugar_forms { |ctx|
  let root = test.temp_dir(ctx, name: "compact-sugar")?

  let output = test.run_script(
    ctx,
    f"""
let root = p"{root}"
defer root.remove(missing_ok: true)?
root.mkdir(parents: true)?
fp"{{root}}/a.txt".write("a")?
fp"{{root}}/b.log".write("b")?
var total = 1
total += 2
let files = g"{root}/*.txt"
let label = if total == 3 {{ "three" }} else {{ "other" }}
let value = match Ok(total) {{ Ok(count) => count, Err(_) => 0 }}
print ${{label}} ${{value}} ${{files |> count()}}
""",
  )?

  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details

  assert output.stdout == """three 3 1
"""
}

test test_ergonomic_sugar_pass_forms { |ctx|
  let root = test.temp_dir(ctx, name: "ergonomic-sugar")?
  fs.remove(root, missing_ok: true)?
  fs.mkdir(fp"{root}/nested/dir")?
  let pkg = {name: "demo", version: "1", path: fp"{root}/nested/dir"}
  let {name, version, ..} = pkg
  var {path: package_path, ..} = pkg
  package_path = fp"{root}/changed"
  var printed_path = ""

  for item in [pkg] {
    printed_path = item.path.display()
  }

  let jobs = env.Str.XSH_ERGONOMIC_SUGAR_MISSING ?? "1"
  let ok = Ok("set") ?? env.Str.XSH_ERGONOMIC_SUGAR_MISSING?
  json.write(fp"{root}/meta.json", {name, version, jobs, ok})?
  let metadata = json.read(fp"{root}/meta.json")?
  fs.remove(fp"{root}/missing", missing_ok: true)?
  assert printed_path == fp"{root}/nested/dir".display()
  assert name == "demo"
  assert version == "1"
  assert jobs == "1"
  assert ok == "set"
  assert metadata.name == "demo"
  assert metadata.jobs == "1"
}

test test_multi_clause_list_comprehension_encounter_order {
  let pairs = [
    outer * 10 + inner
    for outer in [1, 2, 3]
    if outer != 2
    for inner in range(outer)
    if inner != 1
    if outer + inner < 5
  ]
  assert pairs == [10, 30]
  let empty = [
    inner
    for outer in [1]
    if false
    for inner in [outer]
  ]
  assert empty == []
}

test test_multi_clause_comprehension_bindings_are_lexical { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc main() [io] {
  let value = 10
  let values = [value for value in [1, 2] for value in [value + 1]]
  print f"{values[0]},{values[1]},{value}"
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """2,3,10
"""
}

test test_multi_clause_map_comprehension_later_entries_win {
  let entries = [{key: "a", values: [1, 2]}, {key: "b", values: [3]}, {key: "a", values: [4]}]
  let by_key = {
    entry.key: number
    for entry in entries
    for number in entry.values
    if number != 2
  }
  assert (by_key.get("a") ?? 0) == 4
  assert (by_key.get("b") ?? 0) == 3
}

test test_multi_clause_comprehension_evaluates_only_reached_clauses { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc inner(outer: Int) [io] -> List[Int] {
  print f"iter {outer}"
  return [1, 2]
}
proc project(outer: Int, inner: Int) [io] -> Int {
  print f"value {outer}:{inner}"
  return outer * 10 + inner
}
proc main() [io] {
  let values = [project(outer, item) for outer in [1, 2, 3] if outer != 2 for item in inner(outer) if item == 1]
  print f"{values.len()}"
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """iter 1
value 1:1
iter 3
value 3:1
2
"""
}

test test_multi_clause_comprehension_pulls_streams_lazily_and_closes { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc closed(label: Str) [io] -> Unit { print f"close {label}" }
stream numbers(label: Str) [io] -> Stream[Int] {
  defer closed(label)
  for number in [1, 2] {
    print f"pull {label}:{number}"
    yield number
  }
}
proc project(outer: Int, inner: Int) [io] -> Int {
  print f"value {outer}:{inner}"
  return outer * 10 + inner
}
proc main() [io] {
  let values = [project(outer, inner) for outer in numbers("outer") if outer == 1 for inner in numbers("inner")]
  print f"{values.len()}"
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """pull outer:1
pull inner:1
value 1:1
pull inner:2
value 1:2
close inner
pull outer:2
close outer
2
"""
}

test test_multi_clause_comprehension_failure_closes_nested_streams { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc closed(label: Str) [io] -> Unit { print f"close {label}" }
stream numbers(label: Str) [io] -> Stream[Int] {
  defer closed(label)
  for number in [1, 2] {
    print f"pull {label}:{number}"
    yield number
  }
}
error FixtureError = Failure(message: Str)
proc failed() [error] -> Result[List[Int], FixtureError] {
  return Err(FixtureError.Failure(message: "failure"))
}
proc main() [io, error] {
  let values = [value for outer in numbers("outer") for inner in numbers("inner") for value in failed()]
  print f"{values.len()}"
}
""",
  )?
  assert ! output.success
  assert output.stdout == """pull outer:1
pull inner:1
close inner
close outer
"""
  assert "failure" in output.stderr
}

test test_multi_clause_comprehension_rejects_forward_bindings { |ctx|
  let output = test.run_script(
    ctx,
    """let values = [inner for outer in [1] if inner == 1 for inner in [outer]]
""",
  )?
  assert ! output.success
  assert "inner" in output.stderr
}

pure comprehension_values(number: Int) -> Result[List[Int]] {
  [number, number + 1]
}

test test_multi_clause_comprehension_accepts_fallible_iterables {
  let values = [
    inner
    for outer in comprehension_values(1)
    for inner in comprehension_values(outer)
    if inner != 2
  ]
  assert values == [1, 3]
}

test test_multi_clause_comprehension_propagation_retains_result_and_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error FixtureError = Failure(message: Str)
proc closed(label: Str) [io] -> Unit { print f"close {label}" }
stream numbers(label: Str) [io] -> Stream[Int] {
  defer closed(label)
  yield 1
  yield 2
}
proc project() [error] -> Result[Int, FixtureError] {
  return Err(FixtureError.Failure(message: "failure"))
}
proc collect() [io, error] -> Result[List[Int], FixtureError] {
  return [project()? for outer in numbers("outer") for inner in numbers("inner")]
}
proc main() [io, error] {
  match collect() {
    Ok(_) => print "unexpected"
    Err(FixtureError.Failure {message: message}) => print $message
  }
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """close inner
close outer
failure
"""
}

pure list_splice_default(values = [1, @[2, 3]]) -> List[Int] {
  values
}

test test_list_literal_splicing_preserves_types_nesting_and_aliases {
  var middle = [2, 3]
  let source_alias = middle
  var combined = [1, @middle, 4, @[], @[5, 6]]
  let combined_alias = combined
  middle += [9]
  combined += [7]
  assert source_alias == [2, 3]
  assert middle == [2, 3, 9]
  assert combined_alias == [1, 2, 3, 4, 5, 6]
  assert combined == [1, 2, 3, 4, 5, 6, 7]
  let nested = [[1], @[[2], [3]], [4]]
  assert nested == [[1], [2], [3], [4]]
  let empty = [@[], @[]]
  assert empty == []
  assert empty + ["ready"] == ["ready"]
  let inferred = [@[], 8, @[]]
  assert inferred == [8]
  let typed = [
    @[
      1,
      2,
    ],
    3,
  ]
  assert typed == [1, 2, 3]
  let rows: List[Entry] = [{name: "first", score: 1}, @[{name: "second", score: 2}]]
  assert rows[1].name == "second"
  assert list_splice_default() == [1, 2, 3]
  let declared = [p"first", @[p"second"]]
  assert declared == [p"first", p"second"]
}

test test_list_literal_splicing_evaluates_left_to_right_once { |ctx|
  let result = test.run_script(
    ctx,
    r"""proc item(value: Int) [io] -> Int {
  print f"item {value}"
  return value
}
proc items(value: Int) [io] -> List[Int] {
  print f"splice {value}"
  return [value, value + 1]
}
let result = [item(1), @items(2), item(4), @items(5)]
print result.len()
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = result
  assert succeeded, failure_details
  assert result.stdout == """item 1
splice 2
item 4
splice 5
6
"""
}

test test_list_literal_splicing_propagates_before_later_elements { |ctx|
  let result = test.run_script(
    ctx,
    r"""error SpliceFailure = Stopped(message: Str)
proc item(value: Int) [io] -> Int {
  print f"item {value}"
  return value
}
proc flags() [io] -> Result[List[Int], SpliceFailure] {
  print "flags"
  return Err(SpliceFailure.Stopped(message: "stop building"))
}
let values = [item(1), @flags()?, item(9)]
print values.len()
""",
  )?
  let rejected = ! result.success
  let rejection_details = result.stderr
  assert rejected, rejection_details
  assert "stop building" in result.stderr
  assert result.stdout == """item 1
flags
"""
}

test test_list_literal_splicing_rejects_non_lists_and_incompatible_elements { |ctx|
  for source in [
    """let value = [@"text"]
""",
    """let value = [@b"bytes"]
""",
    """let value = [@map.empty()]
""",
    """let value = [@Ok([1])]
""",
    """let items: Any = [1]
let value = [@items]
""",
    """stream rows() -> Stream[Int] { yield 1 }
let value = [@rows()]
""",
    """let value = [1, @["wrong"]]
""",
  ] {
    let result = test.run_script(ctx, source)?
    let rejected = ! result.success
    let rejection_details = result.stderr
    assert rejected, rejection_details
    assert "check." in result.stderr
  }

  let ambiguous = test.run_script(
    ctx,
    """let value = [@[1] for x in [2]]
""",
  )?
  let rejected = ! ambiguous.success
  let rejection_details = ambiguous.stderr
  assert rejected, rejection_details
  assert "parse." in ambiguous.stderr
}

test test_list_literal_splicing_handles_results_explicitly_and_composes_with_argv {
  let loaded = Ok(["-O2", "-g"])
  let argv = ["cc", @loaded?, "-o", "app"]
  let _ = process.command_argv("true", ["true", @argv])
  assert argv == ["cc", "-O2", "-g", "-o", "app"]
}

test test_multi_clause_comprehension_cleanup_precedes_block_and_function_defers { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error FixtureError = Failure(message: Str)
stream numbers(label: Str) [io] -> Stream[Int] {
  defer { print f"close {label}" }
  yield @[1, 2]
}
proc project() [error] -> Result[Int, FixtureError] {
  return Err(FixtureError.Failure(message: "failure"))
}
proc collect() [io, error] -> Result[List[Int], FixtureError] {
  defer { print "function" }
  if true {
    defer { print "block" }
    return [project()? for outer in numbers("outer") for inner in numbers("inner")]
  }
  return []
}
proc main() [io, error] {
  match collect() {
    Ok(_) => print "unexpected"
    Err(FixtureError.Failure {message}) => print $message
  }
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """close inner
close outer
block
function
failure
"""
}
