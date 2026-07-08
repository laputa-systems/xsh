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
  return Err(FsError.NotFound(file: file))
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
  var {path, ..} = pkg
  path = fp"${root}/changed"
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
