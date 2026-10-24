const PRODUCER_KINDS = {"file", "dir"}
var set_producer_events = ""
pure producer_allowed(kind: Str, extra: Set[Str]) -> Bool { kind in PRODUCER_KINDS | extra }
proc observed_producer_set(label: Str, values: Set[Str]) [] -> Set[Str] {
  defer { set_producer_events += "d" }
  set_producer_events += label
  values
}
proc checked_producer_set(label: Str, failing: Bool) [error] -> Set[Str] {
  defer { set_producer_events += "d" }
  set_producer_events += label
  if failing { assert false }
  {"kept",}
}
proc verify_typed_set_producers() [error] -> Result[Unit] {
  let empty: Set[Str] = set.empty()
  assert producer_allowed("file", empty)
  assert producer_allowed("link", {"link",})
  assert producer_allowed("other", empty) == false
  let joined = observed_producer_set("l", {"a", "b"}) | observed_producer_set("r", {"b", "c"})
  assert joined == {"a", "b", "c"}
  assert set_producer_events == "ldrd"
  set_producer_events = ""
  let common = observed_producer_set("l", {"a", "b"}) & observed_producer_set("r", {"b", "c"})
  assert common == {"b",}
  assert set_producer_events == "ldrd"
  assert "a" in joined - common
  assert "b" not in joined - common
  let keys_0 = {1, 2} | {2, 3}
  assert keys_0 == {1, 2, 3}
  let keys_1 = {1KiB, 2KiB} & {2KiB, 3KiB}
  assert keys_1 == {2KiB,}
  let keys_2 = {true, false} | {false,}
  assert keys_2 == {false, true}
  let keys_3 = {b"a", b"b"} & {b"b", b"c"}
  assert keys_3 == {b"b",}
  let keys_4 = {p"/a", p"/b"} | {p"/b", p"/c"}
  assert keys_4 == {p"/a", p"/b", p"/c"}
  let keys_5 = {1s, 2s} & {2s, 3s}
  assert keys_5 == {2s,}
  set_producer_events = ""
  let failure = try { checked_producer_set("l", true) | checked_producer_set("r", false) }
  assert failure is Err(_)
  assert set_producer_events == "ld"
}
