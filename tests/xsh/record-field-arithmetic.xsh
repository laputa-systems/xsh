type Span = {start: Int, size: Int, label: Str}

pure covered(spans: List[Span]) -> Int {
  var total = 0
  for span in spans {
    total = total + span.size * 2 - span.start
  }
  total
}

pure stepped(limit: Span) -> Int {
  var at = 0
  var steps = 0
  while at < limit.size {
    at = at + limit.start
    steps += 1
  }
  steps
}

test test_record_int_fields_read_in_arithmetic_and_conditions {
  let spans: List[Span] = [{start: 1, size: 4, label: "a"}, {start: 3, size: 10, label: "b"}]
  assert covered(spans) == 24
  assert stepped({start: 3, size: 10, label: "c"}) == 4
}

test test_record_int_field_reads_see_the_current_record_value {
  var span: Span = {start: 2, size: 5, label: "x"}
  let original = span
  var sums: List[Int] = []
  for _ in range(3) {
    sums += [span.size + span.start]
    span.size += 10
  }
  assert sums == [7, 17, 27]
  assert original.size == 5
  span = {start: 100, size: 1, label: "y"}
  let after = span.size + span.start
  assert after == 101
  assert original.size + original.start == 7
}

test test_record_int_field_overflow_reports_the_expression { |ctx|
  let overflow = test.expect(
    ctx,
    r"""type Box = {value: Int}
let box: Box = {value: 9223372036854775807}
var total = 1
total = total + box.value
print $total
""",
    status: 3,
    stderr: ["integer overflow", "total + box.value"],
  )?
  assert overflow.stdout == "", overflow.stdout
}
