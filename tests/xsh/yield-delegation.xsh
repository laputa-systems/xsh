test test_yield_delegation_lists_and_parent_continuation { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """0
1
2
3
4
5
3 2 2
"""
}

test test_yield_delegation_pulls_lazily_and_closes_child_first { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """created
parent-start
pull 0
row 0
pull 1
row 1
child-close
parent-close
consumer-after
"""
}

test test_yield_delegation_evaluates_source_once_and_resumes { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """row 1
child-close
between
source
row 2
row 3
after
parent-close
"""
}

test test_yield_delegation_result_handling_and_late_failure { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  assert output.success == false
  assert output.stdout == """row 1
row 2
row 3
child-close
parent-close
"""
  assert "RowsError.Late" in output.stderr
}

test test_yield_delegation_aliases_share_one_cursor { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """1
remaining 0
"""
}

test test_yield_delegation_requires_explicit_list_or_stream { |ctx|
  for value in ["1", "\"text\"", "b\"bytes\"", "{name: 1}", "Ok([1])"] {
    let source = f"""
      stream bad() -> Stream[Int] { yield @${value} }

      """
    let output = test.run_script(ctx, source)?
    assert output.success == false
    assert "check.yield-delegation" in output.stderr
  }

  let output = test.run_script(
    ctx,
    """proc bad() { yield @[1] }
""",
  )?
  assert output.success == false
  assert "check.yield" in output.stderr
}

test test_yield_delegation_guard_and_zero_take_do_not_evaluate_source { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """empty 0
source
row 1
row 2
"""
}

test test_yield_delegation_live_source_and_list_snapshot { |ctx|
  let file_path = test.temp_path(ctx, name: "delegated-lines")
  file_path.write("""first
second
""")?
  let output = test.run_script(
    ctx,
    f"""
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """first
second
last
1
2
3
original 99
"""
}

test test_yield_delegation_cleanup_failure_still_closes_parent { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  assert output.success == false
  assert output.stdout == """row 1
child-close
parent-close
"""
  assert "child cleanup failed" in output.stderr
}

test test_yield_delegation_rejects_item_and_effect_mismatches { |ctx|
  for source in [
    """stream bad() [] -> Stream[Int] { yield @["wrong"] }
""",
    """stream child() [io] -> Stream[Int] { print "effect"; yield 1 }
stream bad() [] -> Stream[Int] { yield @child() }
""",
    """stream child() [] -> Stream[Int] { yield 1 }
stream bad() [] -> Stream[Int] { yield child() }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success == false
    assert "check." in output.stderr
  }
}
