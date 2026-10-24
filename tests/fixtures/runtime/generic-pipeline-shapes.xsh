pure typed_batch(values: List[Int]) -> List[List[Int]] { values |> batch(count: 2) }
pure typed_bytes(values: List[Str]) -> List[List[Str]] { values |> batch(max_bytes: 16) }
pure typed_zip(left: List[Int], right: List[Str]) { left |> zip(right) }
pure typed_repeat(values: List[Int], count: Int) -> List[Int] { values |> repeat(count) }
pure typed_range() -> List[Int] { [0] |> range(2, 5) }
pure typed_shuffle(values: List[Int]) -> List[Int] { values |> shuffle(seed: 1) }

pure batched(value) { [value, value, value] |> batch(count: 2) }
pure no_argv_batch(value) { [value] |> batch(count: 1, max_argv: false) }
pure bounded(value) { [value, value] |> batch(max_bytes: 16) }
pure limited(value) { [value, value] |> batch(count: 1, max_bytes: 16, max_argv: true) }
pure argv_batched(value) { [value, value] |> batch(max_argv: true) }
pure spread_batched(value) { [value, value] |> batch(...{count: 1}) }
pure zipped(left, right) { [left, left] |> zip([right]) }
pure zipped_sources(left, right) { left |> zip(other: right) }
pure repeated(left, right, count: Int) { [left, right] |> repeat(count) }
pure ranged(value) { [value] |> range(end: 5, start: 2) }
pure shuffled(left, right) { [left, right] |> shuffle(seed: 1) }
pure unbatched(value) { [value, value] |> batch(count: 1) |> flat-map { |items| items } }
pure zipped_left(value) { [value] |> zip([1]) |> map .left }
pure ranged_payload(value) { [value] |> range(0, 1) |> map { |item| value } }

var shape_pulls = 0
var shape_closed = 0
var shape_configuration = ""
stream shape_values(value) { defer { shape_closed += 1 }; shape_pulls += 1; yield value; shape_pulls += 1; yield value }
proc setting(label: Str, value: Int) -> Int { shape_configuration += label; value }
proc stream_batch(value) { shape_values(value) |> batch(count: setting("B", 1)) |> take(1) }
proc stream_repeat_zero(value) { shape_values(value) |> repeat(setting("N", 0)) }
proc stream_range(value) { shape_values(value) |> range(end: setting("E", 5), start: setting("S", 2)) }
proc stream_shuffle(value) { shape_values(value) |> shuffle(seed: setting("H", 1)) }
proc right_values(value) { shape_configuration += "Z"; [value] }
proc stream_zip(value) { shape_values(value) |> zip(right_values(value)?) }
error ShapeFailure = Late(message: Str)
proc shape_failure() -> Result[Unit, ShapeFailure] { Err(ShapeFailure.Late(message: "source failed")) }
stream failing_shape_values(value) [error] { defer { shape_closed += 1 }; shape_pulls += 1; yield value; shape_failure()? }
pure consume_shape(values) { values |> shuffle(seed: 1) }
proc consume_right(value) { [value] |> zip(failing_shape_values(value)) }

proc generic_shapes() -> Result[Unit] {
  assert batched(7) == typed_batch([7, 7, 7])
  assert batched("kept") == [["kept", "kept"], ["kept"]]
  let retained: Result[Int, Str] = Err("data")
  assert batched(retained) == [[retained, retained], [retained]]
  assert no_argv_batch(retained) == [[retained]]
  assert bounded("kept") == typed_bytes(["kept", "kept"])
  assert bounded(7) == [[7, 7]]
  assert limited("kept") == [["kept"], ["kept"]]
  assert limited(7) == [[7], [7]]
  assert argv_batched("kept") == [["kept", "kept"]]
  assert argv_batched(7) == [[7, 7]]
  assert spread_batched(retained) == [[retained], [retained]]
  assert zipped(7, "right") == typed_zip([7, 7], ["right"])
  assert zipped("left", 9) == [{left: "left", right: 9}]
  assert zipped(retained, true) == [{left: retained, right: true}]
  assert zipped_sources([7], ["right"]) == [{left: 7, right: "right"}]
  assert zipped_sources({2, 1}, {"b", "a"}) == [{left: 1, right: "a"}, {left: 2, right: "b"}]
  let wrapped: Result[List[Str], Str] = Ok(["right"])
  assert zipped_sources([7], wrapped) == [{left: 7, right: "right"}]
  assert repeated(7, 8, 2) == typed_repeat([7, 8], 2)
  assert repeated("a", "b", 2) == ["a", "b", "a", "b"]
  assert repeated(retained, retained, 2) == [retained, retained, retained, retained]
  assert ranged("ignored") == typed_range()
  assert ranged(retained) == [2, 3, 4]
  assert shuffled(7, 8) == typed_shuffle([7, 8])
  assert shuffled("a", "b") == ["b", "a"]
  assert shuffled(retained, retained) == [retained, retained]
  assert unbatched(retained) == [retained, retained]
  assert zipped_left("kept") == ["kept"]
  assert ranged_payload(retained) == [retained]
  assert stream_batch(7)? == [[7]]
  assert shape_pulls == 2
  assert shape_closed == 1
  assert stream_repeat_zero("kept")? == []
  assert shape_pulls == 2
  assert shape_closed == 1
  assert stream_range(retained)? == [2, 3, 4]
  assert shape_pulls == 2
  assert shape_closed == 1
  assert stream_shuffle("kept")? == ["kept", "kept"]
  assert shape_pulls == 4
  assert shape_closed == 2
  assert stream_zip(retained)? == [{left: retained, right: retained}]
  assert shape_pulls == 5
  assert shape_closed == 3
  assert shape_configuration == "BNESHZ"
  match try { consume_shape(failing_shape_values("kept")) } {
    Err(error) => assert error.message == "source failed"
    Ok(_) => assert false
  }
  assert shape_pulls == 6
  assert shape_closed == 4
  match try { consume_right("kept")? } {
    Err(error) => assert error.message == "source failed"
    Ok(_) => assert false
  }
  assert shape_pulls == 7
  assert shape_closed == 5
  assert typed_repeat([7, 8], -1) == []
  assert repeated(7, 8, -1) == typed_repeat([7, 8], -1)
}
