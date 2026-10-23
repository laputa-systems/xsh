pure sorted(left, right) { [left, right] |> sort }
pure sorted_by(left, right) { [{key: left}, {key: right}] |> sort-by .key }
pure nested_sort(left, right) { [left, right] |> sort-by { |item| {nested: item} } }
pure open_sorted(left, right) { let _ = left.name; [left, right] |> sort }
pure key_identity(value) { value }
pure named_sorted(left, right) { [left, right] |> sort-by(key_identity, desc: true) }
pure counted_key(value) { [value, value] |> count { |item| item } }
pure both_domains(value) { let counted = [value] |> count { |item| item }; let ordered = [value] |> sort; {counted, ordered} }
pure typed_bool(values: List[Bool]) -> List[Bool] { values |> sort }
pure typed_path(values: List[Path]) -> List[Path] { values |> sort }

var domain_configurations = 0
var domain_keys = 0
var domain_closed = 0
stream domain_values(value) { defer { domain_closed += 1 }; yield value; yield value }
proc domain_descending() -> Bool { domain_configurations += 1; true }
proc domain_key(value) { domain_keys += 1; value }
proc observed_sort(value) { domain_values(value) |> sort-by(desc: domain_descending()) { |item| domain_key(item)? } }
proc empty_sort(value) { domain_values(value) |> take(0) |> sort(desc: domain_descending()) }
proc domain_failure() -> Result[Unit, Str] { Err("sort failed") }
proc failed_sort(value) { domain_values(value) |> sort-by { |item| let _ = domain_failure()?; item } }
proc bad_direction() -> Result[Bool, Str] { Err("direction failed") }
proc failed_direction(value) { domain_values(value) |> sort(desc: bad_direction()?) }

test generic_key_domains {
  assert sorted(2, 1) == [1, 2]
  assert sorted("b", "a") == ["a", "b"]
  assert sorted(true, false) == [false, true]
  assert sorted(p"b", p"a") == [p"a", p"b"]
  assert sorted({name: "b", nested: {flag: true}}, {name: "a", nested: {flag: false}}) == [{name: "a", nested: {flag: false}}, {name: "b", nested: {flag: true}}]
  assert sorted_by(2, 1) == [{key: 1}, {key: 2}]
  assert sorted_by("b", "a") == [{key: "a"}, {key: "b"}]
  assert nested_sort(true, false) == [false, true]
  assert open_sorted({name: "b", extra: true}, {name: "a", extra: false}) == [{name: "a", extra: false}, {name: "b", extra: true}]
  assert named_sorted(1, 2) == [2, 1]
  assert (counted_key(7).get("7") ?? 0) == 2
  assert (counted_key("key").get("key") ?? 0) == 2
  assert (counted_key(true).get("true") ?? 0) == 2
  let unsigned: UInt = 7
  assert (counted_key(unsigned).get("7") ?? 0) == 2
  assert both_domains("key").ordered == ["key"]
  assert typed_bool([true, false]) == [false, true]
  assert typed_path([p"b", p"a"]) == [p"a", p"b"]
  assert observed_sort(7)? == [7, 7]
  assert domain_configurations == 1
  assert domain_keys == 2
  assert domain_closed == 1
  assert empty_sort("kept")? == []
  assert domain_configurations == 2
  assert domain_keys == 2
  assert domain_closed == 1
  match try { failed_direction(7)? } {
    Err(error) => assert error == "direction failed"
    Ok(_) => assert false
  }
  assert domain_closed == 1
  match try { failed_sort(7)? } {
    Err(error) => assert error == "sort failed"
    Ok(_) => assert false
  }
  assert domain_closed == 2
}
