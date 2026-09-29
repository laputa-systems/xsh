type Entry = {name: Str, score: Int}

proc test_list_comprehension_basic_transform() [error] {
  let nums = [1, 2, 3]
  let doubled = [x * 2 for x in nums]
  test.eq(doubled, [2, 4, 6])?
}

proc test_list_comprehension_with_guard_filters_elements() [error] {
  let nums = [1, 2, 3, 4, 5]
  let evens = [x for x in nums if x % 2 == 0]
  test.eq(evens, [2, 4])?
}

proc test_list_comprehension_guard_can_produce_empty_list() [error] {
  let nums = [1, 3, 5]
  let evens = [x for x in nums if x % 2 == 0]
  test.eq(evens |> count(), 0)?
}

proc test_list_comprehension_with_record_destructuring() [error] {
  let entries: List[Entry] = [{name: "alice", score: 90}, {name: "bob", score: 55}, {name: "carol", score: 80}]
  let passing = [name for {name, score} in entries if score >= 60]
  test.eq(passing, ["alice", "carol"])?
}

error FsError = NotFound(file: Path) : NotFound | PermissionDenied(file: Path, op: Str) : PermissionDenied

proc missing(file: Path) [error] -> Result[Str, FsError] {
  return Err(FsError.NotFound(file:))
}

proc test_nominal_error_payload_and_facet_patterns() [error] {
  match missing(p"missing") {
    Ok(text) => test.fail(f"unexpected ok ${text}")?
    Err(FsError.NotFound {file: file}) => test.eq(file.display(), "missing")?
    Err(is PermissionDenied) => test.fail("unexpected permission facet")?
    Err(error) => test.fail(error.message)?
  }
}

type Stats = {blanks: Int, code: Int, comments: Int}

pure count_lines(lines: List[Str]) -> Stats {
  var stats: Stats = {blanks: 0, code: 0, comments: 0}

  for line in lines {
    if line.trim() == "" {
      stats.blanks += 1
    } else if line.starts_with("#") {
      stats.comments += 1
    } else {
      stats.code += 1
    }
  }

  return stats
}

proc test_local_accumulator_field_mutation() [error] {
  let stats = count_lines(["alpha", "", "# note", "beta"])
  var counts: Map[Int] = {}
  counts["code"] = stats.code
  counts["comments"] = stats.comments
  test.eq(stats.blanks, 1)?
  test.eq(counts.get("code", 0), 2)?
  test.eq(counts.get("comments", 0), 1)?
}

proc test_compact_sugar_forms(ctx: TestContext) [error] {
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

  test.ok(output.success, output.stderr)?

  test.eq(
    output.stdout,
    """three 3 1
""",
  )?
}

proc test_ergonomic_sugar_pass_forms(ctx: TestContext) [fs, error] {
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
  test.eq(printed_path, fp"${root}/nested/dir".display())?
  test.eq(name, "demo")?
  test.eq(version, "1")?
  test.eq(jobs, "1")?
  test.eq(ok, "set")?
  test.eq(metadata.name, "demo")?
  test.eq(metadata.jobs, "1")?
}

proc test_multi_clause_list_comprehension_encounter_order() [error] {
  let pairs = [
    outer * 10 + inner
    for outer in [1, 2, 3]
    if outer != 2
    for inner in range(outer)
    if inner != 1
    if outer + inner < 5
  ]
  test.eq(pairs, [10, 30])?
  let empty = [
    inner
    for outer in [1]
    if false
    for inner in [outer]
  ]
  test.eq(empty, [])?
}

proc test_multi_clause_comprehension_bindings_are_lexical(ctx: TestContext) [error] {
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "2,3,10\n")?
}

proc test_multi_clause_map_comprehension_later_entries_win() [error] {
  let entries = [{key: "a", values: [1, 2]}, {key: "b", values: [3]}, {key: "a", values: [4]}]
  let by_key = {
    entry.key: number
    for entry in entries
    for number in entry.values
    if number != 2
  }
  test.eq(by_key.get("a", 0), 4)?
  test.eq(by_key.get("b", 0), 3)?
}

proc test_multi_clause_comprehension_evaluates_only_reached_clauses(ctx: TestContext) [error] {
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "iter 1\nvalue 1:1\niter 3\nvalue 3:1\n2\n")?
}

proc test_multi_clause_comprehension_pulls_streams_lazily_and_closes(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
proc closed(label: Str) [io] { print f"close ${label}" }
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull outer:1\npull inner:1\nvalue 1:1\npull inner:2\nvalue 1:2\nclose inner\npull outer:2\nclose outer\n2\n")?
}

proc test_multi_clause_comprehension_failure_closes_nested_streams(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
proc closed(label: Str) [io] { print f"close ${label}" }
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
  test.eq(output.success, false)?
  test.eq(output.stdout, "pull outer:1\npull inner:1\nclose inner\nclose outer\n")?
  test.contains(output.stderr, "failure")?
}

proc test_multi_clause_comprehension_rejects_forward_bindings(ctx: TestContext) [error] {
  let output = test.run_script(ctx, "let values = [inner for outer in [1] if inner == 1 for inner in [outer]]\n")?
  test.eq(output.success, false)?
  test.contains(output.stderr, "inner")?
}

pure comprehension_values(number: Int) -> Result[List[Int]] {
  return Ok([number, number + 1])
}

proc test_multi_clause_comprehension_accepts_fallible_iterables() [error] {
  let values = [
    inner
    for outer in comprehension_values(1)
    for inner in comprehension_values(outer)
    if inner != 2
  ]
  test.eq(values, [1, 3])?
}

proc test_multi_clause_comprehension_propagation_retains_result_and_cleanup(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
error FixtureError = Failure(message: Str)
proc closed(label: Str) [io] { print f"close ${label}" }
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "close inner\nclose outer\nfailure\n")?
}
