pure typed_total(values: List[Int]) -> Int { values |> sum() }
pure typed_minimum(values: List[Int]) -> Result[Int] { values |> min() }
pure typed_maximum(values: List[Int]) -> Result[Int] { values |> max() }
pure typed_minimum_result(values: List[Result[Int, Str]]) -> Result[Result[Int, Str]] { values |> min() }
pure typed_maximum_list(values: List[List[Int]]) -> Result[List[Int]] { values |> max() }
pure typed_minimum_float(values: List[Float]) -> Result[Float] { values |> min() }

pure list_identity(value) { value }
pure total(values) { values |> sum() }
pure total_with_marker(marker, values: List[Int]) -> Int { let _ = marker; values |> sum() }
pure minimum(values) { values |> min() }
pure maximum(values) { values |> max() }
pure mapped_minimum(value) { [value] |> map { |item| item } |> min() }

var aggregate_pulls = 0
var aggregate_closed = 0
stream aggregate_values(first, second) {
  defer { aggregate_closed += 1 }
  aggregate_pulls += 1
  yield first
  aggregate_pulls += 1
  yield second
}
stream empty_aggregate(value) {
  defer { aggregate_closed += 1 }
  for index in range(0) { let _ = index; yield value }
}
error AggregateFailure = Late(message: Str)
proc aggregate_failure() -> Result[Unit, AggregateFailure] { Err(AggregateFailure.Late(message: "upstream failed")) }
stream failing_aggregate(value) [error] {
  defer { aggregate_closed += 1 }
  aggregate_pulls += 1
  yield value
  aggregate_failure()?
}

proc generic_aggregates() -> Result[Unit] {
  assert total([1, 2, 3]) == typed_total([1, 2, 3])
  assert total({1, 2, 3}) == 6
  let wrapped: Result[List[Int], Str] = Ok([2, 3])
  assert total(wrapped) == 5
  let opaque: Result[Int, Str] = Err("data")
  let other: Result[Int, Str] = Ok(7)
  assert total_with_marker(opaque, [1, 2]) == 3
  assert minimum([7, 2]) == typed_minimum([7, 2])
  assert maximum([7, 2]) == typed_maximum([7, 2])
  assert minimum(["z", "a"])? == "a"
  assert maximum([false, true])? == true
  assert minimum([p"z", p"a"])? == p"a"
  assert minimum([{value: 3}, {value: 1}])? == {value: 1}
  assert minimum([opaque, other]) == typed_minimum_result([opaque, other])
  assert minimum([opaque])? == opaque
  assert maximum([[2], [1]]) == typed_maximum_list([[2], [1]])
  assert minimum([9.0, 1.0]) == typed_minimum_float([9.0, 1.0])
  let unchecked: Any = 7
  let nested: List[Any] = [unchecked]
  assert list_identity([nested]) == [nested]
  assert minimum([nested])? == nested
  assert maximum([{value: unchecked}])? == {value: unchecked}
  assert mapped_minimum(opaque)? == opaque
  assert total(aggregate_values(4, 6)) == 10
  assert aggregate_pulls == 2 and aggregate_closed == 1
  assert minimum(aggregate_values(7, 2))? == 2
  assert aggregate_pulls == 4 and aggregate_closed == 2
  assert maximum(aggregate_values("a", "z"))? == "z"
  assert aggregate_pulls == 6 and aggregate_closed == 3
  assert total(empty_aggregate(7)) == 0
  assert aggregate_pulls == 6 and aggregate_closed == 4
  match minimum(empty_aggregate(7)) {
    Err(error) => assert error.message == "stream was empty"
    Ok(_) => assert false
  }
  assert aggregate_pulls == 6 and aggregate_closed == 5
  match maximum(empty_aggregate("kept")) {
    Err(error) => assert error.message == "stream was empty"
    Ok(_) => assert false
  }
  assert aggregate_pulls == 6 and aggregate_closed == 6
  match try { total(failing_aggregate(7)) } {
    Err(error) => assert error.message == "upstream failed"
    Ok(_) => assert false
  }
  assert aggregate_pulls == 7 and aggregate_closed == 7
  match try { minimum(failing_aggregate(7))? } {
    Err(error) => assert error.message == "upstream failed"
    Ok(_) => assert false
  }
  assert aggregate_pulls == 8 and aggregate_closed == 8
  match try { maximum(failing_aggregate(opaque))? } {
    Err(error) => assert error.message == "upstream failed"
    Ok(_) => assert false
  }
  assert aggregate_pulls == 9 and aggregate_closed == 9
  assert minimum(aggregate_values(7, 2) |> take(1))? == 7
  assert aggregate_pulls == 10 and aggregate_closed == 10
}
