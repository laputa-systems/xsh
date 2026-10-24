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
let set_expected = 2 in {1, 2}
assert has(2, {1, 2}) == set_expected
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
    r"""pure computed(container) { 1 - 2 in container }
pure mutated(container) { var key = 1; key -= 2; key in container }
pure relay(container) { mutated(container) }
pure caught_relay(container) { try { relay(container) } }
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
match try { computed(unsigned) } {
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

test test_full_inference_membership_rejects_incompatible_domains { |ctx|
  for source in [
    r"""pure has(item, container) { item in container }
let invalid = has(true, [1])
""",
    r"""pure has(item, container) { item in container }
let invalid = has("key", 7)
""",
    r"""enum First: Str { Build = "same" }
enum Second: Str { Run = "same" }
pure has(item, container) { item in container }
let invalid = has(Build, [Run])
""",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert !output.success
    assert "[check.type-mismatch]" in output.stderr or "[check.membership-type]" in output.stderr, output.stderr
  }
}
