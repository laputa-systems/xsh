var observer_calls = 0
var observer_pulls = 0
var observer_closed = 0
error ObserverFailure = Rejected(message: Str)
proc observe(value) -> Result[Unit, ObserverFailure] { observer_calls += 1; Ok() }
proc observer_failure() -> Result[Unit, ObserverFailure] { Err(ObserverFailure.Rejected(message: "observer failed")) }
proc reject(value) -> Result[Unit, ObserverFailure] { observer_calls += 1; return observer_failure() }
stream observer_values(value) { defer { observer_closed += 1 }; observer_pulls += 1; yield value; observer_pulls += 1; yield value }

pure observer_identity(value) { value }
pure narrowed_observers(values: List[Int?]) -> List[Int] { values |> map { |item| if item != null { observer_identity(item) } else { 0 } } }
pure ignore(value) -> Unit { let _ = value }
pure each_pure_named(value) { [value] |> each(ignore) }
pure tee_pure_named(value) { [value] |> tee(ignore) }
pure each_unit(value) { [value] |> each { |item| let _ = item } }
pure tee_unit(value) { [value] |> tee { |item| let _ = item } }
proc each_inline(value) { [value, value] |> each { |item| observe(item) } }
proc tee_inline(value) { let captured = value; [value] |> tee { |item| let kept = captured; observe(kept) } }
proc each_named(value) { [value] |> each(observe) }
proc tee_named(value) { [value] |> tee(observe) }
proc each_alias(value) { let callback = observe; [value] |> each(callback) }
proc tee_alias(value) { let callback = observe; [value] |> tee(callback) }
proc typed_each(value: Int) { [value, value] |> each { |item| observe(item) } }
proc typed_tee(value: Str) { [value] |> tee(observe) }
proc limited_observers(value) { observer_values(value) |> tee(observe) |> take(1) }
proc skipped_observers(value) { observer_values(value) |> tee(observe) |> take(0) }
proc failed_each(value) { observer_values(value) |> each(reject) }
proc failed_tee(value) { observer_values(value) |> tee { |item| reject(item) } }

proc generic_observers() -> Result[Unit] {
  assert narrowed_observers([null, 7]) == [0, 7]
  each_pure_named(7)
  assert tee_pure_named("kept") == ["kept"]
  each_unit(7)
  each_unit("kept")
  assert tee_unit(7) == [7]
  assert tee_unit("kept") == ["kept"]
  let retained: Result[Int, Str] = Err("data")
  assert tee_unit(retained) == [retained]
  let retained_unit: Result[Unit, ObserverFailure] = observer_failure()
  assert tee_unit(retained_unit) == [retained_unit]
  each_inline(7)?
  assert observer_calls == 2
  typed_each(7)?
  assert observer_calls == 4
  assert tee_inline("kept")? == ["kept"]
  assert typed_tee("kept")? == ["kept"]
  assert observer_calls == 6
  each_named(retained)?
  assert tee_named(retained)? == [retained]
  each_alias("kept")?
  assert tee_alias(7)? == [7]
  assert observer_calls == 10
  assert limited_observers(retained)? == [retained]
  assert observer_calls == 11
  assert observer_pulls == 1
  assert observer_closed == 1
  assert skipped_observers(7)? == []
  assert observer_calls == 11
  assert observer_pulls == 1
  assert observer_closed == 1
  let each_failure = try { failed_each("kept")? }
  assert each_failure is Err(ObserverFailure.Rejected)
  match each_failure {
    Err(error) => assert error.message == "observer failed"
    Ok(_) => assert false
  }
  assert observer_calls == 12
  assert observer_pulls == 2
  assert observer_closed == 2
  let tee_failure = try { failed_tee(retained)? }
  assert tee_failure is Err(ObserverFailure.Rejected)
  match tee_failure {
    Err(error) => assert error.message == "observer failed"
    Ok(_) => assert false
  }
  assert observer_calls == 13
  assert observer_pulls == 3
  assert observer_closed == 3
}
