test test_existing_lvalue_observes_current_root_after_rhs {
  var row = {count: 1, untouched: 2}
  row.count += if true {
    row = {count: 10, untouched: 20}
    3
  } else { 0 }
  assert row == {count: 13, untouched: 20}
  let empty: Map[Int] = {}
  var values = empty.set("selected", 1).set("untouched", 2)
  values["selected"] += if true {
    values = values.set("selected", 30).set("untouched", 40)
    3
  } else { 0 }
  assert values.get("selected")? == 33
  assert values.get("untouched")? == 40
}

test test_list_element_assignment_and_aliases {
  var values = [1, 2, 3]
  let alias = values
  values[1] = 8
  values[2] += 4
  assert values == [1, 8, 7]
  assert alias == [1, 2, 3]
}

test test_list_assignment_selector_and_rhs_observe_current_root {
  var values = [1, 2]
  values[if true {
    values = [10, 20]
    0
  } else { 1 }] += if true {
    values = [30, 40]
    3
  } else { 0 }
  assert values == [33, 40]
  var entries: Map[Int] = {}
  entries[if true {
    entries = entries.set("selected", 10).set("untouched", 20)
    "selected"
  } else { "never" }] += if true {
    entries = entries.set("selected", 30).set("untouched", 40)
    3
  } else { 0 }
  assert entries.get("selected")? == 33
  assert entries.get("untouched")? == 40
}

type ListAssignmentRow = {count: Int, children: List[Int]}

test test_list_assignment_traverses_record_map_and_list_paths {
  let empty: Map[List[ListAssignmentRow]] = {}
  var root = {groups: empty.set("first", [{count: 1, children: [2, 3]}])}
  let alias = root
  root.groups["first"][0].count += 4
  root.groups["first"][0].children[1] *= 3
  root.groups["first"][0].children += [7]
  assert root.groups.get("first")?[0].count == 5
  assert root.groups.get("first")?[0].children == [2, 9, 7]
  assert alias.groups.get("first")?[0].count == 1
  assert alias.groups.get("first")?[0].children == [2, 3]
  var matrix = [[1, 2], [3, 4]]
  let earlier = matrix
  matrix[1][0] -= 2
  matrix[0] += matrix[0]
  assert matrix == [[1, 2, 1, 2], [1, 4]]
  assert earlier == [[1, 2], [3, 4]]
}

test test_list_assignment_preserves_contextual_element_schema {
  var rows: List[ListAssignmentRow] = [{count: 1, children: [2]}]
  rows[0] = {count: 4, children: []}
  rows[0].children = []
  rows[0].children += [3]
  assert rows[0].count == 4
  assert rows[0].children == [3]
}

test test_list_assignment_rejects_indexing_bounds_after_rhs { |ctx|
  for index in [-1, 2, 9223372036854775807] {
    let output = test.run_script(
      ctx,
      f"""
var values = [1, 2]
defer { print (json.encode(values)?) }
values[${index}] = if true {
  print "rhs"
  9
} else { 0 }
""",
    )?
    assert ! output.success
    assert "index-out-of-range" in output.stderr
    assert output.stdout == """rhs
[1,2]
"""
  }
}

test test_list_assignment_failed_arithmetic_keeps_ancestors { |ctx|
  let output = test.run_script(
    ctx,
    r"""
var rows = [{count: 7, untouched: [3]}]
let alias = rows
defer { print (json.encode(rows)?) (json.encode(alias)?) }
rows[0].count /= if true {
  rows[0].untouched = [9]
  print "rhs"
  0
} else { 1 }
""",
  )?
  assert ! output.success
  assert "division" in output.stderr
  assert output.stdout == """rhs
[{"count":7,"untouched":[9]}] [{"count":7,"untouched":[3]}]
"""
}

test test_list_assignment_rejects_immutable_temporary_and_non_list_roots { |ctx|
  for source in [
    """let values = [1]
values[0] = 2
""",
    """var values = [1]
values["first"] = 2
""",
    """var values = [1]
values[0] = "two"
""",
    """var text = "abc"
text[0] = "z"
""",
    """var data = b"abc"
data[0] = 0
""",
    """[1, 2][0] = 3
""",
    """var values = [1]
values[0..1] = [2]
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success
    assert "err[" in output.stderr
  }
}

test test_existing_lvalue_failure_retains_rhs_root_effects { |ctx|
  let output = test.run_script(
    ctx,
    r"""
var row = {count: 7, untouched: 3}
defer { print $row.count $row.untouched }
row.count /= if true {
  row = {count: 8, untouched: 9}
  print "rhs"
  0
} else { 1 }
""",
  )?
  assert ! output.success
  assert "division" in output.stderr
  assert output.stdout == """rhs
8 9
"""
}

test test_list_assignment_evaluates_each_selector_and_rhs_once {
  let empty: Map[List[Int]] = {}
  var root = [empty.set("selected", [1, 2])]
  var seen = []
  root[if true {
    seen += ["outer"]
    0
  } else { 1 }][if true {
    seen += ["key"]
    "selected"
  } else { "never" }][if true {
    seen += ["inner"]
    0
  } else { 1 }] += if true {
    seen += ["rhs"]
    root[0]["selected"][1] = 40
    3
  } else { 0 }
  assert seen == ["outer", "key", "inner", "rhs"]
  assert root[0].get("selected")? == [4, 40]
}

test test_list_assignment_propagates_original_result_before_commit { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error AssignmentError = Failed(code: Int)
pure failed() -> Result[Int, AssignmentError] {
  return Err(AssignmentError.Failed(code: 7))
}
proc update() [io, error] -> Result[Int, AssignmentError] {
  var rows = [{count: 1}]
  defer { print ${rows[0].count} }
  rows[-1].count = if true {
    rows[0].count = 9
    failed()?
  } else { 0 }
  return 0
}
proc main() [io, error] {
  match update() {
    Ok(_) => print "unexpected"
    Err(AssignmentError.Failed {code}) => print $code
  }
}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """9
7
"""
}

type ListAssignmentStats = {blanks: Int = 0, code: Int = 0, comments: Int = 0}

test test_list_assignment_retains_specialized_record_storage {
  var rows = [ListAssignmentStats()]
  let alias = rows
  rows[0].code += 3
  rows[0].comments = 1
  assert rows[0].code == 3
  assert rows[0].comments == 1
  assert alias[0].code == 0
}

pure list_assignment_unit() -> Unit {
  var values = [1]
  values[0] = 2
}

pure list_assignment_empty_unit() -> Unit {}

test test_list_assignment_is_unit {
  assert list_assignment_unit() == list_assignment_empty_unit()
}
