const PRODUCER_KINDS = set.from(["file", "dir"])
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
  set.from(["kept"])
}
proc verify_typed_set_producers() [error] -> Result[Unit] {
  let empty: Set[Str] = set.empty()
  assert producer_allowed("file", empty)
  assert producer_allowed("link", set.from(["link"]))
  assert producer_allowed("other", empty) == false
  let joined = observed_producer_set("l", set.from(["a", "b"])) | observed_producer_set("r", set.from(["b", "c"]))
  assert joined == set.from(["a", "b", "c"])
  assert set_producer_events == "ldrd"
  set_producer_events = ""
  let common = observed_producer_set("l", set.from(["a", "b"])) & observed_producer_set("r", set.from(["b", "c"]))
  assert common == set.from(["b"])
  assert set_producer_events == "ldrd"
  assert "a" in joined - common
  assert "b" not in joined - common
  let keys_0 = set.from([1, 2]) | set.from([2, 3])
  assert keys_0 == set.from([1, 2, 3])
  let keys_1 = set.from([1KiB, 2KiB]) & set.from([2KiB, 3KiB])
  assert keys_1 == set.from([2KiB])
  let keys_2 = set.from([true, false]) | set.from([false])
  assert keys_2 == set.from([false, true])
  let keys_3 = set.from([b"a", b"b"]) & set.from([b"b", b"c"])
  assert keys_3 == set.from([b"b"])
  let keys_4 = set.from([p"/a", p"/b"]) | set.from([p"/b", p"/c"])
  assert keys_4 == set.from([p"/a", p"/b", p"/c"])
  let keys_5 = set.from([1s, 2s]) & set.from([2s, 3s])
  assert keys_5 == set.from([2s])
  set_producer_events = ""
  let failure = try { checked_producer_set("l", true) | checked_producer_set("r", false) }
  assert failure is Err(_)
  assert set_producer_events == "ld"
}
