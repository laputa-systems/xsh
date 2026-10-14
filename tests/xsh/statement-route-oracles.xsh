test test_stmt_let { |ctx|
  test.expect(
    ctx,
    r"""proc show() [io] {
  let values = [2, 3]
  print values.len()
}
show()
""",
    status: 0,
    stdout: ["2"],
  )?
}

test test_stmt_guard { |ctx|
  test.expect(
    ctx,
    r"""pure choose(candidate: Int?) -> Int {
  guard let value = candidate else { return 0 }
  return value
}
print ${choose(7)} ${choose(null)}
""",
    status: 0,
    stdout: ["7 0"],
  )?
}

test test_stmt_with { |ctx|
  test.expect(
    ctx,
    r"""proc load() -> Result[Int] { return Ok(7) }
proc show() [io] {
  with value = load() {
    print $value
  } else { |failure|
    print $failure.message
  }
}
show()
""",
    status: 0,
    stdout: ["7"],
  )?
}

test test_stmt_let_int { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value: Int) -> Int {
  let result: Int = value + 0
  return result
}
print ${identity(42)}
""",
    status: 0,
    stdout: ["42"],
  )?
}

test test_stmt_let_bool { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value: Bool) -> Bool {
  let result: Bool = value and true
  return result
}
print ${identity(true)}
""",
    status: 0,
    stdout: ["true"],
  )?
}

test test_stmt_assign { |ctx|
  test.expect(
    ctx,
    r"""proc replace() [io] {
  var values: List[Int] = [1]
  values = [2, 3]
  print values.len() ${values[0]}
}
replace()
""",
    status: 0,
    stdout: ["2 2"],
  )?
}

test test_stmt_assign_field { |ctx|
  test.expect(
    ctx,
    r"""var record = {name: "before"}
record.name = "after"
print $record.name
""",
    status: 0,
    stdout: ["after"],
  )?
}

test test_stmt_assign_field_int { |ctx|
  test.expect(
    ctx,
    r"""var record = {count: 1}
record.count = 2
print $record.count
""",
    status: 0,
    stdout: ["2"],
  )?
}

test test_stmt_assign_path { |ctx|
  test.expect(
    ctx,
    r"""var values = [1, 2]
values[1] = 3
print values[1]
""",
    status: 0,
    stdout: ["3"],
  )?
}

test test_stmt_assign_int { |ctx|
  test.expect(
    ctx,
    r"""proc increment() [io] {
  var count: Int = 1
  count = count + 2
  print $count
}
increment()
""",
    status: 0,
    stdout: ["3"],
  )?
}

test test_stmt_assign_bool { |ctx|
  test.expect(
    ctx,
    r"""proc enable() [io] {
  var ready: Bool = false
  ready = true
  print $ready
}
enable()
""",
    status: 0,
    stdout: ["true"],
  )?
}

test test_stmt_value { |ctx|
  test.expect(
    ctx,
    r"""let value = retry [] { 4 }?
print $value
""",
    status: 0,
    stdout: ["4"],
  )?
}

test test_stmt_assert { |ctx|
  test.expect(
    ctx,
    r"""assert 2 + 2 == 4
print "checked"
""",
    status: 0,
    stdout: ["checked"],
  )?
}

test test_stmt_expr { |ctx|
  test.expect(
    ctx,
    r"""proc mark() [io] { print "called" }
proc continue_after_mark() [io, error] {
  mark()
  print "continued"
}
continue_after_mark()
""",
    status: 0,
    stdout: ["called", "continued"],
  )?
}

test test_stmt_if { |ctx|
  test.expect(
    ctx,
    r"""pure ready(value: Int) -> Bool { value > 0 }
if ready(1) {
  print "yes"
} else {
  print "no"
}
""",
    status: 0,
    stdout: ["yes"],
  )?
}

test test_stmt_if_bool { |ctx|
  test.expect(
    ctx,
    r"""proc show(ready: Bool) [io] {
  if ready {
    print "yes"
  } else {
    print "no"
  }
}
show(true)
""",
    status: 0,
    stdout: ["yes"],
  )?
}

test test_stmt_while { |ctx|
  test.expect(
    ctx,
    r"""pure has_next(index: Int) -> Bool { index < 2 }
var index = 0
while has_next(index) {
  index += 1
}
print $index
""",
    status: 0,
    stdout: ["2"],
  )?
}

test test_stmt_while_bool { |ctx|
  test.expect(
    ctx,
    r"""proc finish() [io] {
  var ready: Bool = true
  while ready {
    ready = false
  }
  print $ready
}
finish()
""",
    status: 0,
    stdout: ["false"],
  )?
}

test test_stmt_pattern_if { |ctx|
  test.expect(
    ctx,
    r"""let outcome: Result[Int] = Ok(7)
if let Ok(value) = outcome {
  print $value
} else {
  print "missing"
}
""",
    status: 0,
    stdout: ["7"],
  )?
}

test test_stmt_pattern_while { |ctx|
  test.expect(
    ctx,
    r"""pure next(index: Int) -> Int? {
  if index < 3 { index } else { null }
}
var index = 0
var total = 0
while let value = next(index) {
  total += value
  index += 1
}
print $total
""",
    status: 0,
    stdout: ["3"],
  )?
}

test test_stmt_match { |ctx|
  test.expect(
    ctx,
    r"""let outcome: Result[Int] = Ok(7)
match outcome {
  Ok(value) => print $value
  Err(_) => print "failed"
}
""",
    status: 0,
    stdout: ["7"],
  )?
}

test test_stmt_str_match { |ctx|
  test.expect(
    ctx,
    r"""match "ready" {
  "ready" => print "matched"
  else => print "other"
}
""",
    status: 0,
    stdout: ["matched"],
  )?
}

test test_stmt_tag_match { |ctx|
  test.expect(
    ctx,
    r"""enum State { Ready, Paused }
let state = Ready
match state {
  Ready => print "ready"
  Paused => print "paused"
}
""",
    status: 0,
    stdout: ["ready"],
  )?
}

test test_stmt_for { |ctx|
  test.expect(
    ctx,
    r"""for value in [1, 2] {
  print $value
}
""",
    status: 0,
    stdout: ["1", "2"],
  )?
}

test test_stmt_let_record { |ctx|
  test.expect(
    ctx,
    r"""proc show() [io] {
  let row = {name: "alpha", count: 2}
  let {name, count} = row
  print $name $count
}
show()
""",
    status: 0,
    stdout: ["alpha 2"],
  )?
}

test test_stmt_for_record { |ctx|
  test.expect(
    ctx,
    r"""let rows = [{name: "alpha", count: 2}, {name: "beta", count: 3}]
for {name, count} in rows {
  print $name $count
}
""",
    status: 0,
    stdout: ["alpha 2", "beta 3"],
  )?
}

test test_stmt_for_str_lines { |ctx|
  test.expect(
    ctx,
    r"""let text = "first\nsecond"
for line in text.lines() {
  print $line
}
""",
    status: 0,
    stdout: ["first", "second"],
  )?
}

test test_stmt_scan_lines { |ctx|
  test.expect(
    ctx,
    r"""pure count_text_lines(text: Str) -> Int {
  var blanks = 0
  var comments = 0
  for line in text.lines() {
    let trimmed = line.trim()
    if trimmed == "" {
      blanks += 1
    } else if trimmed.starts_with("//") {
      comments += 1
    }
  }
  text.count_lines() - blanks - comments
}
print ${count_text_lines("// note\n\ncode")}
""",
    status: 0,
    stdout: ["1"],
  )?
}

test test_stmt_scan_bytes { |ctx|
  test.expect(
    ctx,
    r"""pure scan_slash_text(text: Bytes) -> Bool {
  var block_depth = 0
  var any_code = false
  var any_comment = false
  for line in text.lines() {
    let line_len = line.len()
    var index = 0
    var code_seen = false
    var comment_seen = false
    var in_string = false
    var string_delim = -1
    var escaped = false
    while index < line_len {
      let ch = line.byte_at(index) ?? -1
      let next = line.byte_at(index + 1) ?? -1
      if block_depth > 0 {
        comment_seen = true
        if ch == 47 and next == 42 {
          block_depth += 1
          index += 2
        } else if ch == 42 and next == 47 {
          block_depth -= 1
          index += 2
        } else {
          index += 1
        }
      } else if in_string {
        code_seen = true
        if escaped {
          escaped = false
        } else if ch == 92 {
          escaped = true
        } else if ch == string_delim {
          in_string = false
        }
        index += 1
      } else if ch == 34 or ch == 39 or ch == 96 {
        code_seen = true
        in_string = true
        string_delim = ch
        index += 1
      } else if ch == 47 and next == 47 {
        comment_seen = true
        index = line_len
      } else if ch == 47 and next == 42 {
        comment_seen = true
        block_depth = 1
        index += 2
      } else {
        if ch != 32 and ch != 9 {
          code_seen = true
        }
        index += 1
      }
    }
    any_code = any_code or code_seen
    any_comment = any_comment or comment_seen
  }
  any_code and any_comment
}
assert scan_slash_text(b"/* open\n*/\n\"//\" // comment")
print "scanned"
""",
    status: 0,
    stdout: ["scanned"],
  )?
}

test test_stmt_print { |ctx|
  test.expect(
    ctx,
    r"""print "statement output"
""",
    status: 0,
    stdout: ["statement output"],
  )?
}

test test_stmt_cd { |ctx|
  test.expect(
    ctx,
    r"""let root = fs.cwd()?
cd $root {
  print "inside"
}
""",
    status: 0,
    stdout: ["inside"],
  )?
}

test test_stmt_env { |ctx|
  test.expect(
    ctx,
    r"""env XSH_STMT_ROUTE_ORACLE=active {
  print e"XSH_STMT_ROUTE_ORACLE"?
}
""",
    status: 0,
    stdout: ["active"],
  )?
}

test test_stmt_run { |ctx|
  test.expect(
    ctx,
    r"""run true
print "ran"
""",
    status: 0,
    stdout: ["ran"],
  )?
}

test test_stmt_loop { |ctx|
  test.expect(
    ctx,
    r"""var visits = 0
loop {
  visits += 1
  break
}
print $visits
""",
    status: 0,
    stdout: ["1"],
  )?
}

test test_stmt_return { |ctx|
  test.expect(
    ctx,
    r"""pure answer() -> Int {
  return 7
}
print ${answer()}
""",
    status: 0,
    stdout: ["7"],
  )?
}

test test_stmt_yield { |ctx|
  test.expect(
    ctx,
    r"""stream values() [] -> Stream[Int] {
  yield 7
}
for value in values() {
  print $value
}
""",
    status: 0,
    stdout: ["7"],
  )?
}

test test_stmt_yield_delegate { |ctx|
  test.expect(
    ctx,
    r"""stream child() [] -> Stream[Int] {
  yield 1
  yield 2
}
stream parent() [] -> Stream[Int] {
  yield @child()
}
for value in parent() {
  print $value
}
""",
    status: 0,
    stdout: ["1", "2"],
  )?
}

test test_stmt_break { |ctx|
  test.expect(
    ctx,
    r"""var visits = 0
while visits < 3 {
  visits += 1
  if visits == 2 {
    break
  }
}
print $visits
""",
    status: 0,
    stdout: ["2"],
  )?
}

test test_stmt_break_value { |ctx|
  test.expect(
    ctx,
    r"""let answer = loop { break 9 }
print $answer
""",
    status: 0,
    stdout: ["9"],
  )?
}

test test_stmt_continue { |ctx|
  test.expect(
    ctx,
    r"""var total = 0
for value in [1, 2, 3] {
  if value == 2 {
    continue
  }
  total += value
}
print $total
""",
    status: 0,
    stdout: ["4"],
  )?
}

test test_stmt_defer { |ctx|
  test.expect(
    ctx,
    r"""proc cleanup() [io] { print "cleanup" }
proc work() [io, error] {
  defer cleanup()
  print "body"
}
work()
""",
    status: 0,
    stdout: ["body", "cleanup"],
  )?
}

test test_stmt_default_parameter { |ctx|
  test.expect(
    ctx,
    r"""pure default_count() -> Int { 5 }
pure answer(value: Int = default_count()) -> Int { value }
print ${answer()}
""",
    status: 0,
    stdout: ["5"],
  )?
}
