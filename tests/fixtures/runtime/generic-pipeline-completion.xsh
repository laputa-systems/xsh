pure completion_identity(value) {
  let ignored = [value] |> map { |item| return value }
  value
}
pure completion_sorted(value) {
  let ignored = [value] |> map { |item| return value } |> sort |> take(0)
  value
}
pure completion_fixed(value) -> Int {
  let ignored = [value] |> map { |item| return 11 } |> sort |> take(0)
  0
}
pure completion_spliced(value) {
  let ignored = [@[value]] |> map { |item| return value } |> sort |> take(0)
  value
}
pure completion_order(value) -> Int {
  var trace = 0
  let ignored = [value] |> map { |item| return trace } |> sort(desc: { trace += 1; false }) |> take(0)
  trace
}
pure typed_completion_order(value: Int) -> Int {
  var trace = 0
  let ignored = [value] |> map { |item| return trace } |> sort(desc: { trace += 1; false }) |> take(0)
  trace
}
proc completion_producing_order(value) -> Result[Int] {
  var trace = 0
  let ignored = [value] |> map { |item| trace += 10; item } |> sort(desc: { trace += 1; false })
  trace
}
pure completion_authored(value) {
  let retained: Any = [value] |> map { |item| return item }
  let _ = retained
  value
}
proc completion_statement(value) -> Result[Int] {
  [value] |> map { |item| return 19 }
  0
}
pure completion_count(marker, items: List[Int]) -> Int {
  let ignored = items |> map { |item| return 23 } |> count
  let _ = marker
  0
}
var completion_closed = 0
proc completion_cleanup(value) -> Result[Int] {
  defer { completion_closed += 1 }
  let ignored = [value] |> map { |item| return 13 } |> sort |> take(0)
  0
}
pure completion_named(value) {
  let rows = [value]
  let ignored = rows |> map { |item| return item } |> sort |> take(0)
  value
}
pure completion_named_alias(value) {
  let rows = [value]
  let alias = rows
  let ignored = alias |> map { |item| return item } |> sort |> take(0)
  value
}
var completion_initializer_runs = 0
proc completion_named_initialization(marker) -> Result[Int] {
  let rows = [{ completion_initializer_runs += 1; 31 }]
  let ignored = rows |> map { |item| return item } |> sort |> take(0)
  let _ = marker
  0
}
proc generic_pipeline_completion() -> Result[Unit] {
  assert completion_identity(7) == 7
  assert completion_identity("retained") == "retained"
  assert completion_sorted(9) == 9
  assert completion_sorted("sorted") == "sorted"
  assert completion_sorted(1.5) == 1.5
  let opaque: Result[Int, Str] = Err("data")
  assert completion_sorted(opaque) == opaque
  assert completion_named(7) == 7
  assert completion_named("named") == "named"
  assert completion_named(opaque) == opaque
  assert completion_named_alias(9) == 9
  assert completion_named_alias("alias") == "alias"
  assert completion_named_alias(opaque) == opaque
  assert completion_named_initialization(1)? == 31
  assert completion_named_initialization("marker")? == 31
  assert completion_initializer_runs == 2
  assert completion_spliced(17) == 17
  assert completion_spliced("spliced") == "spliced"
  assert completion_fixed("marker") == 11
  assert completion_order(1) == typed_completion_order(1)
  assert completion_order("marker") == 0
  assert completion_producing_order(1)? == 11
  assert completion_producing_order("marker")? == 11
  assert completion_cleanup(1)? == 13
  assert completion_cleanup("marker")? == 13
  assert completion_closed == 2
  assert completion_count("empty", []) == 0
  assert completion_count(7, [1]) == 23
  assert completion_authored(7) == 7
  assert completion_authored("retained") == "retained"
  assert completion_statement(1)? == 19
  assert completion_statement("marker")? == 19
}
