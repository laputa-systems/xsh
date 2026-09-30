test test_yield_delegation_lists_and_parent_continuation [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc close(message: Str) [io] { print $message }
stream rows() [] -> Stream[Int] {
  yield 0
  yield @[]
  yield @[1, 2]
  yield @([3, 4])
  yield 5
}
stream nested() [] -> Stream[List[Int]] {
  yield [1, 2]
  yield @[[3], [4, 5]]
}
proc main() [io, error] {
  let values = rows() |> collect()
  for n in values { print f"${n}" }
  let lists = nested() |> collect()
  print f"${lists.len()} ${lists[0].len()} ${lists[2].len()}"
}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "0\n1\n2\n3\n4\n5\n3 2 2\n")?
}

test test_yield_delegation_pulls_lazily_and_closes_child_first [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc close(message: Str) [io] { print $message }
stream child() [io, error] -> Stream[Int] {
  defer close("child-close")
  for n in range(4) {
    print f"pull ${n}"
    yield n
  }
}
stream parent() [io, error] -> Stream[Int] {
  defer close("parent-close")
  print "parent-start"
  yield @child()
  print "parent-after"
  yield 99
}
proc main() [io, error] {
  let source = parent()
  print "created"
  for n in source {
    print f"row ${n}"
    if n == 1 { break }
  }
  print "consumer-after"
}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "created\nparent-start\npull 0\nrow 0\npull 1\nrow 1\nchild-close\nparent-close\nconsumer-after\n")?
}

test test_yield_delegation_evaluates_source_once_and_resumes [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc close(message: Str) [io] { print $message }
proc source() [io] -> List[Int] {
  print "source"
  return [2, 3]
}
stream child() [io, error] -> Stream[Int] {
  defer close("child-close")
  yield 1
}
stream parent() [io, error] -> Stream[Int] {
  defer close("parent-close")
  yield @child()
  print "between"
  yield @source()
  print "after"
}
proc main() [io, error] {
  for n in parent() { print f"row ${n}" }
}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "row 1\nchild-close\nbetween\nsource\nrow 2\nrow 3\nafter\nparent-close\n")?
}

test test_yield_delegation_result_handling_and_late_failure [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc close(message: Str) [io] { print $message }
error RowsError = Late(row: Int)
proc source() [error] -> Result[List[Int], RowsError] { return [1, 2] }
proc fail() [] -> Result[Unit, RowsError] { return Err(RowsError.Late(row: 4)) }
stream child() [io, error] -> Stream[Int] {
  defer close("child-close")
  yield 3
  fail()?
}
stream parent() [io, error] -> Stream[Int] {
  defer close("parent-close")
  yield @(source()?)
  yield @child()
  print "unreachable"
}
proc main() [io, error] {
  for row in parent() { print f"row ${row}" }
  print "consumer-after"
}
""")?
  test.eq(output.success, false)?
  test.eq(output.stdout, "row 1\nrow 2\nrow 3\nchild-close\nparent-close\n")?
  test.ok("RowsError.Late" in output.stderr)?
}

test test_yield_delegation_aliases_share_one_cursor [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc close(message: Str) [io] { print $message }
stream child() [] -> Stream[Int] { yield @[1, 2, 3] }
stream parent(source: Stream[Int]) [] -> Stream[Int] { yield @source }
proc main() [io, error] {
  let source = child()
  let wrapped = parent(source)
  for n in wrapped { print f"${n}"; break }
  let remaining = source |> collect()
  print f"remaining ${remaining.len()}"
}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "1\nremaining 0\n")?
}

test test_yield_delegation_requires_explicit_list_or_stream [error] { |ctx|
  for value in ["1", "\"text\"", "b\"bytes\"", "{name: 1}", "Ok([1])"] {
    let output = test.run_script(ctx, f"stream bad() -> Stream[Int] { yield @${value} }\n")?
    test.eq(output.success, false)?
    test.ok("check.yield-delegation" in output.stderr)?
  }
  let output = test.run_script(ctx, "proc bad() { yield @[1] }\n")?
  test.eq(output.success, false)?
  test.ok("check.yield" in output.stderr)?
}

test test_yield_delegation_guard_and_zero_take_do_not_evaluate_source [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc source() [io] -> List[Int] { print "source"; return [1, 2] }
stream rows() [io] -> Stream[Int] {
  yield @source() when false
  yield @source() unless true
  yield @source()
}
proc main() [io] {
  let empty = rows() |> take(0) |> collect()
  print f"empty ${empty.len()}"
  for row in rows() { print f"row ${row}" }
}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "empty 0\nsource\nrow 1\nrow 2\n")?
}

test test_yield_delegation_live_source_and_list_snapshot [fs, error] { |ctx|
  let file_path = test.temp_path(ctx, name: "delegated-lines")
  file_path.write("first\nsecond\n")?
  let output = test.run_script(ctx, f"""
stream lines(file: Path) [fs, error] -> Stream[Str] {
  yield @(file.lines()?)
  yield "last"
}
stream rows(values: List[Int]) [] -> Stream[Int] { yield @values }
proc main() [io, fs, error] {
  for line in lines(Path("${file_path.display()}")) { print f"\${line}" }
  var values = [1, 2, 3]
  let source = rows(values)
  for number in source {
    print f"\${number}"
    values = [99]
  }
  print f"original \${values[0]}"
}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "first\nsecond\nlast\n1\n2\n3\noriginal 99\n")?
}

test test_yield_delegation_cleanup_failure_still_closes_parent [error] { |ctx|
  let output = test.run_script(ctx, r"""
error CleanupError = Child(message: Str) | Parent(message: Str)
proc child_close() [io, error] -> Result[Unit, CleanupError] {
  print "child-close"
  return Err(CleanupError.Child(message: "child cleanup failed"))
}
proc parent_close() [io, error] -> Result[Unit, CleanupError] {
  print "parent-close"
  return Err(CleanupError.Parent(message: "parent cleanup failed"))
}
stream child() [io, error] -> Stream[Int] {
  defer child_close()?
  yield @[1, 2]
}
stream parent() [io, error] -> Stream[Int] {
  defer parent_close()?
  yield @child()
}
proc main() [io, error] {
  for row in parent() { print f"row ${row}"; break }
}
""")?
  test.eq(output.success, false)?
  test.eq(output.stdout, "row 1\nchild-close\nparent-close\n")?
  test.ok("child cleanup failed" in output.stderr)?
}

test test_yield_delegation_rejects_item_and_effect_mismatches [error] { |ctx|
  for source in [
    "stream bad() [] -> Stream[Int] { yield @[\"wrong\"] }\n",
    "stream child() [io] -> Stream[Int] { print \"effect\"; yield 1 }\nstream bad() [] -> Stream[Int] { yield @child() }\n",
    "stream child() [] -> Stream[Int] { yield 1 }\nstream bad() [] -> Stream[Int] { yield child() }\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.success, false)?
    test.ok("check." in output.stderr)?
  }
}
