type ReductionRow = {key: Str, value: Int}
pure nominal_reduction(values: List[Int]) { values |> reduce-by(sum: true) { |item| ReductionRow(key: "bucket", value: item) } }
pure reduce_rows(row) { [row, row] |> reduce-by(min: true) { |item| item } }
pure minimum_by_key(value) { [value, value] |> reduce-by(min: true) { |item| { key: "bucket", value: item } } }
pure maximum_by_key(value) { [value, value] |> reduce-by(max: true) { |item| { key: "bucket", value: item } } }
pure summed_by_key(value) { [value, value] |> reduce-by(sum: true) { |item| { key: "bucket", value: item } } }
pure converted_keys(value) { [value, value] |> reduce-by(sum: true) { |item| { key: item, value: 1 } } }
pure empty_reduction(value) { [value] |> take(0) |> reduce-by(min: true) { |item| { key: "bucket", value: item } } }
pure typed_integer_keys(values: List[Int]) { values |> reduce-by(sum: true) { |item| { key: item, value: 1 } } }
pure typed_minimum(values: List[Str]) { values |> reduce-by(min: true) { |item| { key: "bucket", value: item } } }

var reduction_pulls = 0
var reduction_closed = 0
var reduction_calls = 0
var reduction_configuration = ""
stream reduction_values(value) {
  defer { reduction_closed += 1 }
  reduction_pulls += 1
  yield value
  reduction_pulls += 1
  yield value
}
proc reduction_mode(label: Str, enabled: Bool) { reduction_configuration += label; enabled }
proc reduction_jobs() { reduction_configuration += "j"; 1 }
proc reduction_tick(value) { reduction_calls += 1; value }
proc reduction_failure() -> Result[Unit, Str] { Err("reduction failed") }
proc reduction_bad_mode() -> Result[Bool, Str] { Err("configuration failed") }
proc observed_reduction(value) {
  reduction_values(value) |> reduce-by(sum: reduction_mode("s", true)?, min: reduction_mode("m", false)?, jobs: reduction_jobs()?) { |item|
    { key: "bucket", value: reduction_tick(item)? }
  }
}
proc failed_reduction(value) {
  reduction_values(value) |> reduce-by(min: true) { |item|
    reduction_failure()?
    { key: "bucket", value: item }
  }
}
proc failed_reduction_configuration(value) {
  reduction_values(value) |> reduce-by(sum: reduction_bad_mode()?) { |item| { key: "bucket", value: item } }
}

pure unsupported_reduction_key(value) { [value] |> reduce-by(min: true) { |item| { key: p"invalid", value: item } } }
pure runtime_reduction_mode(value, enabled: Bool) { [value] |> reduce-by(sum: enabled) { |item| { key: "bucket", value: item } } }
proc invalid_reduce_key() -> Result[Unit] { let _ = unsupported_reduction_key(7) }
proc invalid_reduce_mode() -> Result[Unit] { let _ = runtime_reduction_mode(7, false) }

proc generic_reduce_by() -> Result[Unit] {
  assert (reduce_rows({ key: 7, value: "row", extra: true }).get("7") ?? "") == "row"
  assert (minimum_by_key(7).get("bucket") ?? 0) == 7
  assert (maximum_by_key("kept").get("bucket") ?? "") == "kept"
  assert (summed_by_key(7).get("bucket") ?? 0) == 14
  assert (summed_by_key(1.5).get("bucket") ?? 0.0) == 3.0
  assert (converted_keys(7).get("7") ?? 0) == 2
  assert (converted_keys(true).get("true") ?? 0) == 2
  assert (converted_keys("name").get("name") ?? 0) == 2
  assert empty_reduction("kept").len() == 0
  assert (typed_integer_keys([2, 2]).get("2") ?? 0) == 2
  assert (nominal_reduction([2, 3]).get("bucket") ?? 0) == 5
  assert (typed_minimum(["b", "a"]).get("bucket") ?? "") == "a"
  let retained: Result[Int, Str] = Err("data")
  let opaque = minimum_by_key(retained)
  assert "bucket" in opaque
  assert (opaque.get("bucket") ?? retained) == retained
  assert (observed_reduction(7)?.get("bucket") ?? 0) == 14
  assert reduction_configuration == "smj"
  assert reduction_calls == 2
  assert reduction_pulls == 2
  assert reduction_closed == 1
  match try { failed_reduction_configuration("kept")? } {
    Err(error) => assert error == "configuration failed"
    Ok(_) => assert false
  }
  assert reduction_pulls == 2
  assert reduction_closed == 1
  match try { failed_reduction("kept")? } {
    Err(error) => assert error == "reduction failed"
    Ok(_) => assert false
  }
  assert reduction_pulls == 3
  assert reduction_closed == 2
}
