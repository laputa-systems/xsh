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

pure list_splice_default(values: List[Int] = [1, @[2, 3]]) -> List[Int] {
  return values
}

proc test_list_literal_splicing_preserves_types_nesting_and_aliases() [error] {
  var middle = [2, 3]
  let source_alias = middle
  var combined = [1, @middle, 4, @[], @[5, 6]]
  let combined_alias = combined
  middle += [9]
  combined += [7]
  test.eq(source_alias, [2, 3])?
  test.eq(middle, [2, 3, 9])?
  test.eq(combined_alias, [1, 2, 3, 4, 5, 6])?
  test.eq(combined, [1, 2, 3, 4, 5, 6, 7])?
  let nested = [[1], @[[2], [3]], [4]]
  test.eq(nested, [[1], [2], [3], [4]])?
  let empty: List[Str] = [@[], @[]]
  test.eq(empty, [])?
  let inferred = [@[], 8, @[]]
  test.eq(inferred, [8])?
  let typed: List[Int] = [
    @(
      [1] + [2]
    ),
    3,
  ]
  test.eq(typed, [1, 2, 3])?
  let rows: List[Entry] = [{name: "first", score: 1}, @[{name: "second", score: 2}]]
  test.eq(rows[1].name, "second")?
  test.eq(list_splice_default(), [1, 2, 3])?
  let declared: List[Path] = [p"first", @[p"second"]]
  test.eq(declared, [p"first", p"second"])?
}

proc test_list_literal_splicing_evaluates_left_to_right_once(ctx: TestContext) [error] {
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
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "item 1\nsplice 2\nitem 4\nsplice 5\n6\n")?
}

proc test_list_literal_splicing_propagates_before_later_elements(ctx: TestContext) [error] {
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
  test.ok(! result.success, result.stderr)?
  test.contains(result.stderr, "stop building")?
  test.eq(result.stdout, "item 1\nflags\n")?
}

proc test_list_literal_splicing_rejects_non_lists_and_incompatible_elements(ctx: TestContext) [error] {
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
    test.ok(! result.success, result.stderr)?
    test.contains(result.stderr, "check.")?
  }
  let ambiguous = test.run_script(ctx, "let value = [@[1] for x in [2]]\n")?
  test.ok(! ambiguous.success, ambiguous.stderr)?
  test.contains(ambiguous.stderr, "parse.")?
}

proc test_list_literal_splicing_handles_results_explicitly_and_composes_with_argv() [error] {
  let loaded: Result[List[Str]] = Ok(["-O2", "-g"])
  let argv = ["cc", @(loaded?), "-o", "app"]
  let _ = process.command_argv("true", ["true", @argv])
  test.eq(argv, ["cc", "-O2", "-g", "-o", "app"])?
}
