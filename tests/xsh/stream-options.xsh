test test_stream_options_preserve_stage_entry_and_pull_timing { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [] -> Stream[Int] {
  print "pull:1"
  yield 1
  print "pull:2"
  yield 2
  print "close"
}
proc source() [] -> Stream[Int] {
  print "create"
  return numbers()
}
proc jobs() [] -> Int { print "jobs"; return 2 }
proc mapped(value: Int) [] -> Int { print f"map:{value}"; return value }
proc main() [] {
  let values = source() |> map { |item| mapped(item) } |> par-map(jobs: jobs()) { |item| item }
  print values.len()
  let empty = numbers() |> take(0) |> par-map(jobs: jobs()) { |item| item }
  print empty.len()
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """create
pull:1
map:1
pull:2
map:2
close
jobs
2
jobs
0
"""
}

test test_stream_options_preserve_positional_order_and_failure_before_pull { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream numbers() [] -> Stream[Int] { print "pull"; yield 1 }
proc argument(label: Str, value: Int) [] -> Int { print $label; return value }
proc descending() [error] -> Bool {
  print "desc"
  let _ = "invalid direction".parse_int()?
  return true
}
proc main() [error] {
  let values = [] |> range(argument("start", 1), argument("end", 3))
  print values.len()
  let _ = numbers() |> sort-by(desc: descending()) { |item| item }
}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """start
end
2
desc
"""
}

test test_stream_named_options_cover_defaults_modes_and_combined_limits { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc flag(label: Str, value: Bool) [] -> Bool { print $label; return value }
proc number(label: Str, value: Int) [] -> Int { print $label; return value }
proc main() [error] {
  let jobs = 1
  print ${[1, 2] |> par-map(jobs:) { |item| item + 1 }[0]}
  print ${[1] |> par-map { |item| item }[0]}
  print ${[1, 3, 2] |> sort(desc: true)[0]}
  print ${[3, 1, 2] |> sort[0]}
  print ${([{size: 1}, {size: 2}] |> sort-by(desc: true) .size)[0].size}
  print ${([{size: 2}, {size: 1}] |> sort-by .size)[0].size}
  let options = {max_bytes: 5, count: 2, max_argv: false}
  print ${(["aa", "bb", "cc"] |> batch(...options)).len()}
  print ${([1, 2, 3] |> batch(count: 2)).len()}
  print ${(["aa", "bb"] |> batch(max_argv: true)).len()}
  let summed = [1, 2] |> reduce-by(sum: true) { |item| {key: "all", value: item} }
  print (summed.get("all") ?? 0)
  let minimum = [2, 1] |> reduce-by(min: true, jobs: 1) { |item| {key: "all", value: item} }
  print (minimum.get("all") ?? 0)
  let maximum = [1, 2] |> reduce-by(max: true) { |item| {key: "all", value: item} }
  print (maximum.get("all") ?? 0)
  let dynamic = [1, 2] |> reduce-by(jobs: number("jobs", 1), max: flag("max", false), sum: flag("sum", true)) { |item| {key: "all", value: item} }
  print (dynamic.get("all") ?? 0)
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """2
1
3
1
2
1
2
2
1
3
1
2
jobs
max
sum
3
"""
}

test test_stream_named_positionals_preserve_source_order_and_spread_once { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc number(label: Str, value: Int) [] -> Int { print $label; return value }
type Bounds = {start: Int, end: Int}
proc bounds() [] -> Bounds { print "bounds"; return {start: 1, end: 3} }
proc main() [error] {
  print ${[] |> range(end: number("end", 3), start: number("start", 1)).len()}
  print ${[] |> range(...bounds()).len()}
  print ${[1, 2] |> take(count: 1)[0]}
  print ${[1, 2] |> drop(count: 1)[0]}
  print ${[1, 2] |> repeat(count: 2).len()}
  print ${b"abcd" |> bytes.chunks(size: 2).len()}
  print ${[1, 2] |> zip(other: [3, 4])[0].right}
  print ${[1, 2] |> fold(init: 0) { |sum, item| sum + item }}
  print ${[1, 2] |> reduce(init: 0) { |sum, item| sum + item }}
  print ${[1, 2] |> shuffle(seed: 1).len()}
  [{name: "row"}] |> table.print(columns: ["name"])
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert """end
start
2
bounds
2
1
2
4
2
3
3
3
2
""" in output.stdout
  assert "row" in output.stdout
}

test test_stream_named_options_reject_duplicate_unknown_type_mode_and_spread { |ctx|
  for script in [
    "proc main() [] { let _ = [1] |> par-map(jobs: 1, jobs: 2) { |item| item } }",
    "proc main() [] { let options = {jobs: 1}; let _ = [1] |> par-map(...options, jobs: 2) { |item| item } }",
    "proc main() [] { let _ = [1] |> sort(other: true) }",
    "proc main() [] { let _ = [1] |> sort(desc: 1) }",
    "proc main() [] { let _ = [1] |> batch(count: 0) }",
    "proc main() [] { let _ = [1] |> batch(max_argv: false) }",
    "proc main() [] { let _ = [1] |> reduce-by(sum: true, min: true) { |item| {key: \"all\", value: item} } }",
    "proc main() [] { let _ = [1] |> reduce-by(sum: false) { |item| {key: \"all\", value: item} } }",
    "proc main() [] { let _ = [1] |> sort(...1) }",
    "proc main() [] { let options = {}; let _ = [1] |> sort(...options) }",
    "proc main() [] { let _ = [1] |> range(0, start: 1) }",
    "proc main() [] { let _ = [1] |> sort(true) }",
    "proc main() [] { let _ = [1] |> zip(other: 1) }",
  ] {
    let output = test.run_script(ctx, script)?
    {
      let assertion_condition = ! output.success
      let assertion_message = script
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = "check." in output.stderr
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = "unsupported-indexed" not in output.stderr
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_stream_named_options_preserve_materialization_and_live_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream rows() [] -> Stream[Row] { defer { print "close" }; print "pull"; yield {name: "row"} }
type Row = {name: Str}
proc direction() [] -> Bool { print "desc"; return true }
proc columns() [] -> List[Str] { print "columns"; return ["name"] }
proc main() [] {
  let _ = [2, 1] |> sort(...{desc: direction()})
  rows() |> table.print(columns: columns())
  let _ = [1, 2] |> batch(count: 1, max_argv: false)
  let _ = [1, 2] |> shuffle
  [{name: "row"}] |> table.print
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert """desc
columns
pull
close
""" in output.stdout
  assert "row" in output.stdout
  let failure = test.run_script(
    ctx,
    r"""
stream values() [] -> Stream[Int] { defer { print "close" }; print "pull"; yield 1; print "later"; yield 2 }
proc direction() [] -> Bool { print "desc"; return true }
proc main() [] { let _ = values() |> sort(desc: direction()) }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = failure
    assert assertion_condition, assertion_message
  }
  assert failure.stdout == """pull
later
close
desc
"""
}

test test_stream_named_spreads_bind_each_configuration_role { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc main() [] {
  let workers = {jobs: 1}
  let descending = {desc: true}
  let modes = {sum: true, min: false, max: false, jobs: 1}
  let limit = {count: 1}
  let byte_limits = {max_bytes: 2}
  let argv = {max_argv: true}
  let _ = [1] |> par-map(...workers) { |item| item }
  let ordered = [1, 2] |> sort-by(...descending) { |item| item }
  print ordered[0]
  let reduced = [1, 2] |> reduce-by(...modes) { |item| {key: "all", value: item} }
  print (reduced.get("all") ?? 0)
  let _ = [1, 2] |> take(...limit) |> drop(...{count: 0}) |> repeat(...{count: 2})
  let _ = ["a", "b", "c"] |> batch(...byte_limits)
  let _ = ["a", "b"] |> batch(...argv)
  let _ = b"abcd" |> bytes.chunks(...{size: 2})
  let _ = [1] |> zip(...{other: [2]})
  let sum = [1, 2] |> fold(...{init: 0}) { |total, item| total + item }
  print $sum
  let _ = [1] |> reduce(...{init: 0}) { |total, item| total + item }
  let _ = [1, 2] |> shuffle(...{seed: 1})
  [{name: "row"}] |> table.print(...{columns: ["name"]})
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert """2
3
3
""" in output.stdout
  assert "row" in output.stdout
}

test test_stream_dynamic_modes_and_combined_batch_failure_preserve_cleanup { |ctx|
  let modes = test.run_script(
    ctx,
    r"""
stream numbers() [] -> Stream[Int] { print "pull"; yield 1 }
proc disabled() [] -> Bool { print "mode"; return false }
proc main() [] { let _ = numbers() |> reduce-by(sum: disabled()) { |item| {key: "all", value: item} } }
""",
  )?
  {
    let assertion_condition = ! modes.success
    let assertion_message = modes.stderr
    assert assertion_condition, assertion_message
  }
  assert modes.stdout == """mode
"""
  assert "stream-reduce-mode" in modes.stderr
  let batches = test.run_script(
    ctx,
    r"""
stream words() [] -> Stream[Str] {
  defer { print "close" }
  print "pull"
  yield "oversized"
  print "later"
  yield "a"
}
proc main() [] { let _ = words() |> batch(count: 2, max_bytes: 2, max_argv: true) }
""",
  )?
  {
    let assertion_condition = ! batches.success
    let assertion_message = batches.stderr
    assert assertion_condition, assertion_message
  }
  assert batches.stdout == """pull
close
"""
  assert "argv-limit" in batches.stderr
}

test test_stream_option_migration_is_fatal_and_tooling_fix_is_narrow { |ctx|
  let source = """# café
let values = [1, 2] |> par-map --jobs=2 { |item| item + 1 } # keep
print values.len()
"""
  let rejected = test.run_script(ctx, source)?
  {
    let assertion_condition = ! rejected.success
    let assertion_message = rejected.stderr
    assert assertion_condition, assertion_message
  }
  assert rejected.stdout == ""
  assert "parse.stream-option-migration" in rejected.stderr
  let candidate = test.temp_file(ctx, name: "stage-option-migration.xsh", contents: bytes.from_text(source))?
  let diagnosed = run.capture --text "xsht" lint $candidate ?
  {
    let assertion_condition = "lint.stream-options" in diagnosed.stderr
    let assertion_message = diagnosed.stderr
    assert assertion_condition, assertion_message
  }
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  {
    let assertion_condition = applied.status.exited_with(0)
    let assertion_message = applied.stderr
    assert assertion_condition, assertion_message
  }
  let fixed = candidate.read_text()?
  assert fixed == source.replace("--jobs=2", with: "(jobs: 2)")
  let output = test.run_script(ctx, fixed)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """2
"""
  let _ = run.capture --text "xsht" lint --fix $candidate ?
  assert candidate.read_text()? == fixed
  let external = test.run_script(
    ctx,
    "proc main() [process, error] { run printf \"%s\\n\" --jobs --desc --max-bytes ? }",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = external
    assert assertion_condition, assertion_message
  }
  assert external.stdout == """--jobs
--desc
--max-bytes
"""
}

test test_stream_option_migration_refuses_comments_and_unrelated_errors { |ctx|
  for source in [
    """let values = [1] |> reduce-by --sum # preserve
 --jobs=1 { |item| {key: "all", value: item} }
print values.len()
""",
    """let values = [1] |> par-map --jobs=2 { |item| missing + item }
print values.len()
""",
    """let values = [1] |> par-map --jobs=2 { |item| item }
let broken = (
""",
    """let values = [1] |> sort-by --desc (.size)
""",
  ] {
    let candidate = test.temp_file(ctx, name: "stage-option-no-fix.xsh", contents: bytes.from_text(source))?
    let refused = run.capture --text "xsht" lint --fix $candidate ?
    {
      let assertion_condition = ! refused.status.exited_with(0)
      let assertion_message = refused.stderr
      assert assertion_condition, assertion_message
    }
    assert candidate.read_text()? == source
    assert refused.stderr != "", "diagnostics must remain visible"
  }
}
