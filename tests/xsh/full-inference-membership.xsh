test test_full_inference_membership_preserves_container_domains { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure has(item, container) { item in container }
pure lacks(item, container) { item not in container }
pure forward(item, container) { has(item, container) }
enum MembershipFirst: Str { MembershipBuild = "build", MembershipRun = "run" }
enum MembershipSecond: Str { MembershipTask = "task" }
assert has(MembershipBuild, [MembershipBuild]) and lacks(MembershipRun, [MembershipBuild])
assert forward(MembershipTask, [MembershipTask])
let payload: Result[Int, Str] = Err("data")
assert has(payload, [payload])
let unsigned: Map[UInt, Int] = {[1]: 7}
let signed: Map[Int, Str] = {[-1]: "negative"}
let list_expected = 7 in [7, 8]
assert has(7, [7, 8]) == list_expected
assert has("word", ["word"]) and lacks("absent", ["word"])
assert !lacks(7, [7, 8])
let set_expected = 2 in set.from([1, 2])
assert has(2, set.from([1, 2])) == set_expected
assert has("key", {["key"]: null})
assert has(-1, signed)
assert has(1, unsigned) and lacks(0, unsigned)
assert has("key", {key: null})
assert has("key", "a key") and lacks("missing", "a key")
assert has(b"bc", b"abcd") and lacks(b"ce", b"abcd")
assert has("bin", p"/xsh/bin") and has(p"/xsh", p"/xsh/bin")
assert forward(7, [7]) and forward("word", "word")
env ({PATH: "/xsh/bin:/xsh/other"}) {
  assert has(p"/xsh/bin", env.PATH)
  assert lacks(p"/xsh/missing", env.PATH)
}?
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == ""
  assert output.stderr == ""
}

test test_full_inference_membership_validates_unsigned_keys_before_the_container { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure computed(container) { try { 1 - 2 in container } }
pure mutated(container) { var key = 1; key -= 2; try { key in container } }
pure relay(container) { mutated(container) }
pure caught_relay(container) { try { relay(container)? } }
proc selected(container, key: Int) { key in container }
pure ordered(container) {
  var visits = 0
  let result = try {
    let key = { visits += 1; -1 }
    key in { visits += 10; container }
  }
  {visits, result}
}
let signed: Map[Int, Int] = {[0]: 3}
let unsigned: Map[UInt, Int] = {[0]: 7}
assert selected(signed, 0) == Ok(true)
assert selected(unsigned, 0) == Ok(true)
match selected(unsigned, -1) {
  Ok(_) => { assert false },
  Err(error) => { assert error.message == "schema check failed: expected UInt, found Int" },
}
let caught_unsigned = ordered(unsigned)
assert caught_unsigned.visits == 1
match caught_unsigned.result {
  Ok(_) => { assert false },
  Err(error) => { assert error.message == "schema check failed: expected UInt, found Int" },
}
let caught_signed = ordered(signed)
assert caught_signed.visits == 11
assert caught_signed.result == Ok(false)
match computed(unsigned) {
  Ok(_) => { assert false },
  Err(error) => {
    test.error_kind(Err(error), "schema")?
    assert error.message == "schema check failed: expected UInt, found Int"
  },
}
match caught_relay(unsigned) {
  Ok(_) => { assert false },
  Err(error) => {
    test.error_kind(Err(error), "schema")?
    assert error.message == "schema check failed: expected UInt, found Int"
  },
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == ""
  assert output.stderr == ""
}

test test_full_inference_membership_keeps_plain_pure_key_failures_fatal { |ctx|
  for call in ["computed(unsigned)", "relay(unsigned)"] {
    let source = r"""pure computed(container) { 1 - 2 in container }
pure mutated(container) { var key = 1; key -= 2; key in container }
pure relay(container) { mutated(container) }
let unsigned: Map[UInt, Int] = {[0]: 7}
""" + "let captured = try { " + call + " }\n"
    let output = test.expect(ctx, source, status: 3, stderr: ["schema check failed: expected UInt, found Int"])?
    assert output.stdout == "", output.stdout
  }
}

test test_full_inference_membership_rejects_incompatible_domains { |ctx|
  test.expect(
    ctx,
    r"""pure has(item, container) { item in container }
let invalid = has(true, [1])
""",
    status: 2,
    stderr: ["[check.operator-type]", "operation requires Int, found Bool", "has(true, [1])"],
  )?
  test.expect(
    ctx,
    r"""pure has(item, container) { item in container }
let invalid = has("key", 7)
""",
    status: 2,
    stderr: ["[check.membership-type]", "membership is not defined for Str in Int", "has(\"key\", 7)"],
  )?
  test.expect(
    ctx,
    r"""enum First: Str { Build = "same" }
enum Second: Str { Run = "same" }
pure has(item, container) { item in container }
let invalid = has(Build, [Run])
""",
    status: 2,
    stderr: ["[check.operator-type]", "operation requires Second, found First", "has(Build, [Run])"],
  )?
}

test test_full_inference_membership_checks_loop_items_against_a_ground_container { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure known_in_list(items, known: List[Str]) {
  var count = 0
  for item in items { if item in known { count += 1 } }
  count
}
pure first_unknown(items, known: List[Str]) {
  for item in items { guard item in known else { return item } }
  "none"
}
let listed = known_in_list(["a", "b", "c"], ["a", "c"])
let missing = first_unknown(["a", "b"], ["a"])
print $listed
print $missing
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "2\nb\n"
}

test test_full_inference_membership_loop_item_rejects_a_mismatched_ground_container { |ctx|
  test.expect(
    ctx,
    r"""pure known_in_list(items, known: List[Str]) {
  var count = 0
  for item in items { if item in known { count += 1 } }
  count
}
let rejected = known_in_list([1, 2], ["a"])
""",
    status: 2,
    stderr: ["[check."],
  )?
}

test test_full_inference_empty_collection_locals_pass_to_generic_callees { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure measure(items) { items.len() }
let names: Set[Str] = set.empty()
let counts: Map[Str, Int] = map.empty()
let kept = measure(names)
let mapped = measure(counts)
print $kept
print $mapped
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "0\n0\n"
}

test test_full_inference_membership_of_a_non_integer_needle_needs_no_key_validation { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure has_nul(data) {
  let head = if data.len() > 4096 { data[0..4096] } else { data }
  b"\0" in head
}
pure has_label(container) { b"label" in container }
assert has_nul(b"a\0b") and !has_nul(b"abc")
assert has_label({[b"label"]: 1}) and !has_label({[b"other"]: 1})
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == ""
  assert output.stderr == ""
}
