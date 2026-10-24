pure parallel(value, jobs: Int) { [value, value] |> par-map(jobs: jobs) { |item| item } }
pure parallel_capture(value) { [1, 2] |> par-map(jobs: 1) { |item| value } }
pure parallel_nested(value) { [value] |> par-map(jobs: 1) { |item| [item] |> map { |item| item } } }

var parallel_closed = 0
var parallel_configurations = 0
stream parallel_values(value) { defer { parallel_closed += 1 }; yield value; yield value }
proc parallel_jobs() -> Int { parallel_configurations += 1; 1 }
proc parallel_stream(value) { parallel_values(value) |> par-map(jobs: parallel_jobs()) { |item| item } |> take(1) }
proc parallel_fail() -> Result[Unit, Str] { Err("parallel failed") }
proc parallel_failure(value) { parallel_values(value) |> par-map(jobs: 1) { |item| let _ = parallel_fail()?; item } }

proc generic_parallel() -> Result[Unit] {
  assert parallel(7, 2) == [7, 7]
  assert parallel("kept", 1) == ["kept", "kept"]
  let retained: Result[Int, Str] = Err("data")
  assert parallel(retained, 1) == [retained, retained]
  assert parallel_capture("kept") == ["kept", "kept"]
  assert parallel_nested(7) == [[7]]
  assert parallel_stream(7)? == [7]
  assert parallel_configurations == 1
  assert parallel_closed == 1
  match try { parallel_failure("kept")? } {
    Err(error) => assert error == "parallel failed"
    Ok(_) => assert false
  }
  assert parallel_closed == 2
}
