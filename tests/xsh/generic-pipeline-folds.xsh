pure folded(value) { [value, value] |> fold(value) { |acc, item| item } }
pure reduced(value) { [value, value] |> reduce(value) { |acc, item| acc } }
pure counted(value) { [value, value] |> fold(0) { |acc, item| acc + 1 } }
pure empty_fold(value) { [value] |> take(0) |> fold(value) { |acc, item| item } }
pure nested_fold(value) { [value] |> fold(value) { |acc, item| [acc, item] |> fold(acc) { |inner, current| current } } }
pure named_fold(value) { [value] |> fold(init: value) { |acc, item| item } }
pure spread_fold(value) { [value] |> reduce(...{init: value}) { |acc, item| acc } }

var fold_calls = 0
var fold_closed = 0
proc fold_tick(value) { fold_calls += 1; value }
stream fold_values(value) { defer { fold_closed += 1 }; yield value; yield value }
proc effect_fold(value) { fold_values(value) |> fold(fold_tick(value)?) { |acc, item| fold_tick(item)? } }
proc empty_effect_fold(value) { fold_values(value) |> take(0) |> fold(fold_tick(value)?) { |acc, item| fold_tick(item)? } }
proc failed_fold(value) { fold_values(value) |> fold(value) { |acc, item| let _ = fold_failure()?; item } }
proc fold_failure() -> Result[Unit, Str] { Err("fold failed") }
proc bad_initial() -> Result[Int, Str] { Err("initial failed") }
proc failed_initial(value) { fold_values(value) |> fold(bad_initial()?) { |acc, item| acc } }

test generic_folds {
  assert folded(7) == 7
  assert folded("kept") == "kept"
  assert reduced(7) == 7
  assert reduced("kept") == "kept"
  assert counted("kept") == 2
  assert empty_fold("kept") == "kept"
  assert nested_fold(7) == 7
  assert named_fold("kept") == "kept"
  assert spread_fold(7) == 7
  let retained: Result[Int, Str] = Err("data")
  assert folded(retained) == retained
  assert effect_fold(7)? == 7
  assert fold_calls == 3
  assert fold_closed == 1
  assert empty_effect_fold("kept")? == "kept"
  assert fold_calls == 4
  assert fold_closed == 1
  match try { failed_initial(7)? } {
    Err(error) => assert error == "initial failed"
    Ok(_) => assert false
  }
  assert fold_calls == 4
  assert fold_closed == 1
  match try { failed_fold(7)? } {
    Err(error) => assert error == "fold failed"
    Ok(_) => assert false
  }
  assert fold_closed == 2
}
