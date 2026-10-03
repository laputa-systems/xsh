type Entry = {name: Str, score: Int}

test test_list_comprehension_basic_transform {
  let nums = [1, 2, 3]
  let doubled = [x * 2 for x in nums]
  doubled == [2, 4, 6]
}

test test_list_comprehension_with_guard_filters_elements {
  let nums = [1, 2, 3, 4, 5]
  let evens = [x for x in nums if x % 2 == 0]
  evens == [2, 4]
}

test test_list_comprehension_guard_can_produce_empty_list {
  let nums = [1, 3, 5]
  let evens = [x for x in nums if x % 2 == 0]
  evens |> count() == 0
}

test test_list_comprehension_with_record_destructuring {
  let entries: List[Entry] = [{name: "alice", score: 90}, {name: "bob", score: 55}, {name: "carol", score: 80}]
  let passing = [name for {name, score} in entries if score >= 60]
  passing == ["alice", "carol"]
}

error FsError = NotFound(file: Path) : NotFound | PermissionDenied(file: Path, op: Str) : PermissionDenied

proc missing(file: Path) [error] -> Result[Str, FsError] {
  Err(FsError.NotFound(file:))
}

test test_nominal_error_payload_and_facet_patterns {
  match missing(p"missing") {
    Ok(text) => test.fail(f"unexpected ok ${text}")?
    Err(FsError.NotFound {file: file}) => file.display() == "missing"
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
  stats.blanks == 1
  (counts.get("code") ?? 0) == 2
  (counts.get("comments") ?? 0) == 1
}

test test_compact_sugar_forms { |ctx|
  let root = test.temp_dir(ctx, name: "compact-sugar")?

  let output = test.run_script(
    ctx,
    f"""
let root = p"${root.display()}"
defer root.remove(missing_ok: true)?
root.mkdir(parents: true)?
fp"\${root}/a.txt".write("a")?
fp"\${root}/b.log".write("b")?
var total = 1
total += 2
let files = g"${root.display()}/*.txt"
let label = if total == 3 { "three" } else { "other" }
let value = match Ok(total) { Ok(count) => count, Err(_) => 0 }
print \${label} \${value} \${files |> count()}
""",
  )?

  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details

  output.stdout == """three 3 1
"""
}

test test_ergonomic_sugar_pass_forms { |ctx|
  let root = test.temp_dir(ctx, name: "ergonomic-sugar")?
  fs.remove(root, missing_ok: true)?
  fs.mkdir(fp"${root}/nested/dir")?
  let pkg = {name: "demo", version: "1", path: fp"${root}/nested/dir"}
  let {name, version, ..} = pkg
  var {path: package_path, ..} = pkg
  package_path = fp"${root}/changed"
  var printed_path = ""

  for item in [pkg] {
    printed_path = item.path.display
  }

  let jobs = env.Str.XSH_ERGONOMIC_SUGAR_MISSING ?? "1"
  let ok = Ok("set") ?? env.Str.XSH_ERGONOMIC_SUGAR_MISSING?
  json.write(fp"${root}/meta.json", {name, version, jobs, ok})?
  let metadata = json.read(fp"${root}/meta.json")?
  fs.remove(fp"${root}/missing", missing_ok: true)?
  printed_path == fp"${root}/nested/dir".display()
  name == "demo"
  version == "1"
  jobs == "1"
  ok == "set"
  metadata.name == "demo"
  metadata.jobs == "1"
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
  pairs == [10, 30]
  let empty = [
    inner
    for outer in [1]
    if false
    for inner in [outer]
  ]
  empty == []
}

test test_multi_clause_comprehension_bindings_are_lexical { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc main() [io] {
  let value = 10
  let values = [value for value in [1, 2] for value in [value + 1]]
  print f"${values[0]},${values[1]},${value}"
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  output.stdout == "2,3,10\n"
}

test test_multi_clause_map_comprehension_later_entries_win {
  let entries = [{key: "a", values: [1, 2]}, {key: "b", values: [3]}, {key: "a", values: [4]}]
  let by_key = {
    entry.key: number
    for entry in entries
    for number in entry.values
    if number != 2
  }
  (by_key.get("a") ?? 0) == 4
  (by_key.get("b") ?? 0) == 3
}

test test_multi_clause_comprehension_evaluates_only_reached_clauses { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc inner(outer: Int) [io] -> List[Int] {
  print f"iter ${outer}"
  return [1, 2]
}
proc project(outer: Int, inner: Int) [io] -> Int {
  print f"value ${outer}:${inner}"
  return outer * 10 + inner
}
proc main() [io] {
  let values = [project(outer, item) for outer in [1, 2, 3] if outer != 2 for item in inner(outer) if item == 1]
  print f"${values.len()}"
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  output.stdout == "iter 1\nvalue 1:1\niter 3\nvalue 3:1\n2\n"
}

test test_multi_clause_comprehension_pulls_streams_lazily_and_closes { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc closed(label: Str) [io] -> Unit { print f"close ${label}" }
stream numbers(label: Str) [io] -> Stream[Int] {
  defer closed(label)
  for number in [1, 2] {
    print f"pull ${label}:${number}"
    yield number
  }
}
proc project(outer: Int, inner: Int) [io] -> Int {
  print f"value ${outer}:${inner}"
  return outer * 10 + inner
}
proc main() [io] {
  let values = [project(outer, inner) for outer in numbers("outer") if outer == 1 for inner in numbers("inner")]
  print f"${values.len()}"
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  output.stdout == "pull outer:1\npull inner:1\nvalue 1:1\npull inner:2\nvalue 1:2\nclose inner\npull outer:2\nclose outer\n2\n"
}

test test_multi_clause_comprehension_failure_closes_nested_streams { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc closed(label: Str) [io] -> Unit { print f"close ${label}" }
stream numbers(label: Str) [io] -> Stream[Int] {
  defer closed(label)
  for number in [1, 2] {
    print f"pull ${label}:${number}"
    yield number
  }
}
error FixtureError = Failure(message: Str)
proc failed() [error] -> Result[List[Int], FixtureError] {
  return Err(FixtureError.Failure(message: "failure"))
}
proc main() [io, error] {
  let values = [value for outer in numbers("outer") for inner in numbers("inner") for value in failed()]
  print f"${values.len()}"
}
""",
  )?
  ! output.success
  output.stdout == "pull outer:1\npull inner:1\nclose inner\nclose outer\n"
  "failure" in output.stderr
}

test test_multi_clause_comprehension_rejects_forward_bindings { |ctx|
  let output = test.run_script(ctx, "let values = [inner for outer in [1] if inner == 1 for inner in [outer]]\n")?
  ! output.success
  "inner" in output.stderr
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
  values == [1, 3]
}

test test_multi_clause_comprehension_propagation_retains_result_and_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error FixtureError = Failure(message: Str)
proc closed(label: Str) [io] -> Unit { print f"close ${label}" }
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
  output.stdout == "close inner\nclose outer\nfailure\n"
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
  source_alias == [2, 3]
  middle == [2, 3, 9]
  combined_alias == [1, 2, 3, 4, 5, 6]
  combined == [1, 2, 3, 4, 5, 6, 7]
  let nested = [[1], @[[2], [3]], [4]]
  nested == [[1], [2], [3], [4]]
  let empty = [@[], @[]]
  empty == []
  empty + ["ready"] == ["ready"]
  let inferred = [@[], 8, @[]]
  inferred == [8]
  let typed = [
    @(
      [1, 2]
    ),
    3,
  ]
  typed == [1, 2, 3]
  let rows: List[Entry] = [{name: "first", score: 1}, @[{name: "second", score: 2}]]
  rows[1].name == "second"
  list_splice_default() == [1, 2, 3]
  let declared = [p"first", @[p"second"]]
  declared == [p"first", p"second"]
}

test test_list_literal_splicing_evaluates_left_to_right_once { |ctx|
  let result = test.run_script(
    ctx,
    r"""proc item(value: Int) [io] -> Int {
  print f"item $value"
  return value
}
proc items(value: Int) [io] -> List[Int] {
  print f"splice $value"
  return [value, value + 1]
}
let result = [item(1), @items(2), item(4), @items(5)]
print result.len()
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = result
  assert succeeded, failure_details
  result.stdout == "item 1\nsplice 2\nitem 4\nsplice 5\n6\n"
}

test test_list_literal_splicing_propagates_before_later_elements { |ctx|
  let result = test.run_script(
    ctx,
    r"""error SpliceFailure = Stopped(message: Str)
proc item(value: Int) [io] -> Int {
  print f"item $value"
  return value
}
proc flags() [io] -> Result[List[Int], SpliceFailure] {
  print "flags"
  return Err(SpliceFailure.Stopped(message: "stop building"))
}
let values = [item(1), @(flags()?), item(9)]
print values.len()
""",
  )?
  let rejected = ! result.success
  let rejection_details = result.stderr
  assert rejected, rejection_details
  "stop building" in result.stderr
  result.stdout == "item 1\nflags\n"
}

test test_list_literal_splicing_rejects_non_lists_and_incompatible_elements { |ctx|
  for source in [
    "let value = [@\"text\"]\n",
    "let value = [@b\"bytes\"]\n",
    "let value = [@map.empty()]\n",
    "let value = [@Ok([1])]\n",
    "let items: Any = [1]\nlet value = [@items]\n",
    "stream rows() -> Stream[Int] { yield 1 }\nlet value = [@rows()]\n",
    "let value = [1, @[\"wrong\"]]\n",
  ] {
    let result = test.run_script(ctx, source)?
    let rejected = ! result.success
    let rejection_details = result.stderr
    assert rejected, rejection_details
    "check." in result.stderr
  }
  let ambiguous = test.run_script(ctx, "let value = [@[1] for x in [2]]\n")?
  let rejected = ! ambiguous.success
  let rejection_details = ambiguous.stderr
  assert rejected, rejection_details
  "parse." in ambiguous.stderr
}

test test_list_literal_splicing_handles_results_explicitly_and_composes_with_argv {
  let loaded = Ok(["-O2", "-g"])
  let argv = ["cc", @(loaded?), "-o", "app"]
  let _ = process.command_argv("true", ["true", @argv])
  argv == ["cc", "-O2", "-g", "-o", "app"]
}

test test_multi_clause_comprehension_cleanup_precedes_block_and_function_defers { |ctx|
  let output = test.run_script(ctx, r"""
error FixtureError = Failure(message: Str)
stream numbers(label: Str) [io] -> Stream[Int] {
  defer { print f"close ${label}" }
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
""")?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  output.stdout == "close inner\nclose outer\nblock\nfunction\nfailure\n"
}
