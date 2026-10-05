# An index written as a negative integer literal counts from the end of a
# list. A computed index that turns out negative is still out of range.

type Table = {rows: List[List[Int]]}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

pure last(items: List[Str]) -> Str {
  items[-1]
}

pure depth_last(items: List[Int], depth: Int) -> Int {
  if depth == 0 { items[-1] } else { depth_last(items, depth - 1) + items[-2] }
}

test test_negative_literal_index_counts_from_the_end {
  let items = [1, 2, 3]
  assert items[-1] == 3
  assert items[-2] == 2
  assert items[-3] == 1
  assert items[-0] == 1
  assert last(["a", "b"]) == "b"
  assert "a,b,c".split(",")[-1] == "c"
  assert depth_last(items, 4) == 11

  let table = Table([[1], [2, 3]])
  assert table.rows[-1][-2] == 2
  assert items[1..][-1] == 3
}

pure fetch_rows(ready: Bool) -> Result[List[Int]] {
  fail "not ready" unless ready
  Ok([7, 8, 9])
}

pure branch_tail(items: List[Int], first: Bool) -> Int {
  if first { items[-2] } else { items[-1] }
}

pure arm_tail(items: List[Int]) -> Int {
  match items.len() {
    0 => 0
    else => items[-1]
  }
}

pure fetched_tail(ready: Bool) -> Result[Int] {
  (fetch_rows(ready)?)[-1]
}

# The value of a function body, of an `if` or `match` branch, and of a
# comprehension element is computed on the evaluator's explicit frame stack,
# where a call argument or a top-level binding is computed by the recursive
# one. Both read an index from the end; these are the frame-stack positions,
# with the base that fails and the index that is out of range.
test test_negative_literal_index_in_tail_branch_and_comprehension_positions { |ctx|
  assert last(["a", "b"]) == "b"
  assert branch_tail([1, 2, 3], true) == 2
  assert branch_tail([1, 2, 3], false) == 3
  assert arm_tail([4, 5]) == 5
  assert [row[-1] for row in [[1, 2], [3]]] == [2, 3]
  assert [row[-1] for row in [[1, 2], [3, 4]] if row[-2] > 1] == [4]
  assert fetched_tail(true) is Ok(9)
  assert fetched_tail(false) is Err(_)

  let output = test.run_script(
    ctx,
    """pure tail(items: List[Int]) -> Int {
  items[-3]
}

print \${tail([1, 2, 3])}
print \${tail([1, 2])}
print "unreachable"
""",
  )?
  assert output.status != 0
  assert output.stdout == "1\n", output.stdout
  assert "index-out-of-range" in output.stderr, output.stderr
  assert ":2:" in output.stderr, output.stderr
}

test test_negative_literal_index_follows_a_null_safe_hop {
  let absent: List[Int]? = null
  let present: List[Int]? = [4, 5]
  assert absent?[-1] == null
  assert present?[-1] == 5
}

test test_negative_literal_index_reads_inside_a_stream_stage {
  let rows = [[1, 2], [3]]
  let tails = rows |> map .[-1] |> collect()
  assert tails == [2, 3]
}

test test_negative_literal_index_past_the_start_fails_at_run_time { |ctx|
  let output = test.run_script(
    ctx,
    """let items = [1, 2].push(3)
print \${items[-3]}
print \${items[-4]}
print "unreachable"
""",
  )?
  assert output.status != 0
  assert output.stdout == "1\n", output.stdout
  assert "index-out-of-range" in output.stderr, output.stderr
  assert ":3:" in output.stderr, output.stderr
}

# Only the literal counts from the end, so an off-by-one that computes a
# negative index never reads the last item silently.
test test_computed_negative_index_is_still_out_of_range { |ctx|
  for index in ["offset", "0 - 1", "items.len() - 4", "- offset - 2"] {
    let output = test.run_script(
      ctx,
      f"""let items = [1, 2, 3]
let offset = 0 - 1
print "before"
print \${{items[{index}]}}
print "unreachable"
""",
    )?
    assert output.status != 0, index
    assert output.stdout == "before\n", index
    assert "index-out-of-range" in output.stderr, output.stderr
  }
}

test test_negative_literal_index_past_a_list_literal_is_a_check_error { |ctx|
  let output = test.expect(
    ctx,
    """let fits = [1, 2][-2]
let missing = [1, 2][-3]
let spliced = [@[1, 2], 3][-4]
print \$fits \$missing \$spliced
""",
    status: 2,
  )?
  assert output.stdout == ""
  assert count(output.stderr, "err[check.index-out-of-range]") == 1, output.stderr
  assert "index -3 is out of range for a list of 2 item(s)" in output.stderr, output.stderr
  assert ":2:" in output.stderr, output.stderr
}

# An assignment target does not count from the end.
test test_negative_literal_assignment_target_is_out_of_range { |ctx|
  let output = test.run_script(
    ctx,
    """var items = [1, 2]
items[-1] = 9
print "unreachable"
""",
  )?
  assert output.status != 0
  assert output.stdout == ""
  assert "index-out-of-range" in output.stderr, output.stderr
}

test test_lint_rewrites_a_length_minus_a_literal { |ctx|
  let source = """type Table = {rows: List[List[Int]]}

proc show(items: List[Int], table: Table) [io] {
  let last = items[items.len() - 1]
  let cell = table.rows[table.rows.len() - 2][0]
  var copy = items
  copy[copy.len() - 1] = 0
  let zeroed = copy[copy.len() - 1]
  print \$last \$cell \$zeroed
}

show([1, 2, 3], {rows: [[4], [5]]})
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "end-index.xsh", contents: bytes.from_text(source))?

  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-negative-index $candidate
  let report = linted.stdout + linted.stderr
  assert count(report, "warn[lint.prefer-negative-index]") == 3, report

  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-negative-index $candidate
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert "  let last = items[-1]\n" in rewritten, rewritten
  assert "  let cell = table.rows[-2][0]\n" in rewritten, rewritten
  assert "  copy[copy.len() - 1] = 0\n" in rewritten, rewritten
  assert "  let zeroed = copy[-1]\n" in rewritten, rewritten

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
  assert after.stdout == "3 4 0\n"
}
