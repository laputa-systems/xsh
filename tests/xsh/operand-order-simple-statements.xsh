test test_simple_statement_expression_payloads_evaluate_once { |ctx|
  let output = test.run_script(
    ctx,
    r"""type Pair = {text: Str, count: Int}

proc marked_text(label: Str, value: Str) -> Str {
  print f"eval:{label}"
  return value
}

proc marked_pair() -> Pair {
  print "eval:let-record"
  return Pair(text: "record", count: 7)
}

proc value_tail() -> Str {
  marked_text("value", "tail")
}

proc expression_statement() {
  print "eval:expr"
}

let bound = marked_text("let", "bound")
var assigned = "before"
assigned = marked_text("assign", "after")
var pair = Pair(text: "before", count: 0)
pair.text = marked_text("assign-field", "field")
let {text: record_text, count: record_count} = marked_pair()
let tail = value_tail()
expression_statement()
print f"{bound}|{assigned}|{pair.text}|{pair.count}|{record_text}|{record_count}|{tail}"
""",
  )?

  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == """eval:let
eval:assign
eval:assign-field
eval:let-record
eval:value
eval:expr
bound|after|field|0|record|7|tail
"""
}

test test_path_assignment_evaluates_the_index_before_the_value { |ctx|
  let failed = test.expect(
    ctx,
    r"""var values = [0]
proc marked_value() -> Int {
  print "eval:rhs"
  return 5
}
values[1 / 0] = marked_value()
""",
    status: 3,
    stderr: ["division-by-zero"],
  )?
  assert failed.stdout == ""

  let succeeded = test.expect(
    ctx,
    r"""var nested = [[0]]
proc marked_index_one() -> Int {
  print "eval:index-one"
  return 0
}
proc marked_index_two() -> Int {
  print "eval:index-two"
  return 0
}
proc marked_value() -> Int {
  print "eval:rhs"
  return 5
}
nested[marked_index_one()][marked_index_two()] = marked_value()
print ${nested[0][0]}
""",
    status: 0,
  )?
  assert succeeded.stdout == """eval:index-one
eval:index-two
eval:rhs
5
"""
}

test test_assert_statement_orders_and_selects_its_message { |ctx|
  let definitions = r"""proc condition(value: Bool) -> Bool {
  print "eval:condition"
  return value
}
proc message() -> Str {
  print "eval:message"
  return "assertion context"
}
"""
  let passed = test.expect(ctx, definitions + "assert condition(true), message()\n", status: 0)?
  assert passed.stdout == "eval:condition\n"

  let failed = test.expect(
    ctx,
    definitions + "assert condition(false), message()\n",
    status: 3,
    stderr: ["assertion context"],
  )?
  assert failed.stdout == "eval:condition\neval:message\n"
}

test test_print_statement_evaluates_arguments_left_to_right_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc marked(label: Str, value: Str) -> Str {
  print f"eval:{label}"
  return value
}
print ${marked("first", "one")} ${marked("second", "two")}
""",
    status: 0,
  )?
  assert output.stdout == "eval:first\neval:second\none two\n"
  assert output.stderr == ""
}

test test_cd_statement_evaluates_its_target_before_its_body { |ctx|
  let output = test.expect(
    ctx,
    r"""proc target() -> Path {
  print "eval:target"
  return fs.cwd()?
}
cd ${target()} {
  print "body"
}
""",
    status: 0,
  )?
  assert output.stdout == "eval:target\nbody\n"
  assert output.stderr == ""
}

test test_env_statement_evaluates_values_left_to_right_before_its_body { |ctx|
  let output = test.expect(
    ctx,
    r"""proc marked(label: Str, value: Str) -> Str {
  print f"eval:{label}"
  return value
}
env XSH_ROUTE_ORDER_FIRST=${marked("first", "one")} XSH_ROUTE_ORDER_SECOND=${marked("second", "two")} {
  print e"XSH_ROUTE_ORDER_FIRST"? e"XSH_ROUTE_ORDER_SECOND"?
}
""",
    status: 0,
  )?
  assert output.stdout == "eval:first\neval:second\none two\n"
  assert output.stderr == ""
}

test test_run_statement_evaluates_arguments_left_to_right_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc marked(label: Str, value: Str) -> Str {
  print f"eval:{label}"
  return value
}
run printf "%s:%s\n" ${marked("first", "one")} ${marked("second", "two")}
""",
    status: 0,
  )?
  assert output.stdout == "eval:first\neval:second\none:two\n"
  assert output.stderr == ""
}
