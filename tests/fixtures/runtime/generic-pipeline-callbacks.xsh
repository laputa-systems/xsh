pure identity(value) { value }
pure named(value) { [value] |> map(identity) }

var callback_count = 0
var callback_cleaned = 0

proc tick(value) { callback_count += 1; value }
stream callback_values(value) { defer { callback_cleaned += 1 }; yield value; yield value }
proc stepped(value) { [value, value, value] |> map { |item| tick(item)? } |> take(1) }
proc skipped(value) { [value] |> take(0) |> map { |item| tick(item)? } }
proc mapped_stream(value) { callback_values(value) |> map { |item| item } |> take(1) }
proc bad_callback() -> Result[Int, Str] { Err("callback failure") }
proc callback_failure(value) { callback_values(value) |> map { |item| let _ = bad_callback()?; item } }

pure mapped(value) { [value, value] |> map { |item| item } |> where { |item| true } |> flat-map { |item| [item] } }
pure captured(value) { [1, 2] |> map { |item| value } }
pure any_value(value) { [value] |> any { |item| true } }
pure all_value(value) { [value] |> all { |item| true } }
pure nested(value) { [value] |> map { |item| [item] |> map { |item| item } } }
pure shorthand(value) { [{payload: value}] |> map .payload }

proc generic_callbacks() -> Result[Unit] {
  assert named(7) == [7]
  assert named("kept") == ["kept"]
  let retained: Result[Int, Str] = Err("data")
  assert named(retained) == [retained]
  assert mapped(7) == [7, 7]
  assert mapped("kept") == ["kept", "kept"]
  assert captured(7) == [7, 7]
  assert any_value(7)
  assert all_value("kept")
  assert nested(7) == [[7]]
  assert shorthand("kept") == ["kept"]
  assert stepped(7)? == [7]
  assert callback_count == 1
  assert skipped("kept")? == []
  assert callback_count == 1
  assert mapped_stream(7)? == [7]
  assert callback_cleaned == 1
  match try { callback_failure(7)? } {
    Err(error) => assert error == "callback failure"
    Ok(_) => assert false
  }
  assert callback_cleaned == 2
}
