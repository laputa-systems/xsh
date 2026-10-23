pure grouped(value) { [value, value] |> group-by { |item| item } }
pure group_capture(value) { [1, 2] |> group-by { |item| value } }
pure unique_values(value) { [value, value] |> unique-by { |item| item } }
pure counted_values(value) { [value, value] |> count { |item| "all" } }
pure counted_integer(value) { [value, value] |> count { |item| 1 } }
pure counted_boolean(value) { [value, value] |> count { |item| true } }
pure counted_unsigned(value) { [value, value] |> count { |item| let key: UInt = 1; key } }

var keys_closed = 0
stream keyed_values(value) { defer { keys_closed += 1 }; yield value; yield value }
proc key_failure() -> Result[Unit, Str] { Err("key failed") }
proc failed_group(value) { keyed_values(value) |> group-by { |item| let _ = key_failure()?; item } }

proc generic_keys() -> Result[Unit] {
  assert grouped(7) == [{key: 7, items: [7, 7]}]
  assert grouped("kept") == [{key: "kept", items: ["kept", "kept"]}]
  let retained: Result[Int, Str] = Err("data")
  assert grouped(retained) == [{key: retained, items: [retained, retained]}]
  assert group_capture("kept") == [{key: "kept", items: [1, 2]}]
  assert unique_values(7) == [7]
  assert unique_values("kept") == ["kept"]
  assert unique_values(retained) == [retained]
  assert (counted_values("kept").get("all") ?? 0) == 2
  assert (counted_integer("kept").get("1") ?? 0) == 2
  assert (counted_boolean("kept").get("true") ?? 0) == 2
  assert (counted_unsigned("kept").get("1") ?? 0) == 2
  match try { failed_group(7)? } {
    Err(error) => assert error == "key failed"
    Ok(_) => assert false
  }
  assert keys_closed == 1
}
