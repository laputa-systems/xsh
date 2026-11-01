test test_repeated_optional_guards_keep_their_payload_views { |ctx|
  let output = test.run_xsh(ctx, r"""pure consume(value: Path) -> Str { value.display() }
pure selected(value) -> Str {
  let checked: Path? = if let item = value { item } else { null }
  if let item = value { consume(item) } else { "none" }
}
pure forwarded(value) -> Str { selected(value) }
pure guarded(value) -> Str {
  guard let item = value else { return "none" }
  consume(item)
}
let possible: Path? = p"/"
print selected(p"/")
print selected(null)
print forwarded(possible)
print forwarded(null)
print guarded(possible)
print guarded(null)
""")?
  assert output.success, output.stderr
  assert output.stdout == "/\nnone\n/\nnone\n/\nnone\n", output.stdout
}

test test_optional_destinations_do_not_widen_required_inputs { |ctx|
  let output = test.run_xsh(ctx, r"""pure consume(value: Str) -> Int { value.byte_len() }
pure optional_first(value, choice: Bool) -> Int {
  if choice { let output: Str? = value; 0 } else { consume(value) }
}
pure required_first(value, choice: Bool) -> Int {
  if choice { consume(value) } else { let output: Str? = value; 0 }
}
pure forwarded(value, choice: Bool) -> Int { optional_first(value, choice) }
print optional_first("abc", true)
print optional_first("abc", false)
print required_first("abc", true)
print required_first("abc", false)
print forwarded("hello", false)
""")?
  assert output.success, output.stderr
  assert output.stdout == "0\n3\n3\n0\n5\n", output.stdout
}

test test_repeated_optional_guards_reject_a_wrong_payload { |ctx|
  let output = test.run_xsh(ctx, r"""pure consume(value: Path) -> Str { value.display() }
pure selected(value) -> Str {
  let checked: Path? = if let item = value { item } else { null }
  if let item = value { consume(item) } else { "none" }
}
print selected(7)
""")?
  assert ! output.success, output.stdout
  assert "[check." in output.stderr, output.stderr
}

test test_required_inputs_reject_nullable_callers_after_optional_destinations { |ctx|
  let output = test.run_xsh(ctx, r"""pure consume(value: Str) -> Int { value.byte_len() }
pure selected(value, choice: Bool) -> Int {
  if choice { let output: Str? = value; 0 } else { consume(value) }
}
let possible: Str? = null
print selected(possible, false)
""")?
  assert ! output.success, output.stdout
  assert "[check." in output.stderr, output.stderr
}

test test_guard_local_optional_assignments_keep_their_input_requirement { |ctx|
  let accepted = test.run_xsh(ctx, r"""pure selected(value) -> Str {
  if let item = value { let stored: Str? = item; stored ?? "none" } else { "absent" }
}
pure forwarded(value) -> Str { selected(value) }
let present: Str? = "text"
let missing: Str? = null
print selected(present)
print forwarded(missing)
""")?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == "text\nabsent\n", accepted.stdout
  let rejected = test.run_xsh(ctx, r"""pure selected(value) -> Str {
  if let item = value { let stored: Str? = item; stored ?? "none" } else { "absent" }
}
let present: Int? = 7
print selected(present)
""")?
  assert ! rejected.success, rejected.stdout
  assert "[check." in rejected.stderr, rejected.stderr
}
test test_copied_inputs_preserve_nullable_storage_bounds_in_both_orders { |ctx|
  let output = test.run_xsh(ctx, r"""type Item = {name: Str}
pure known(value: Item?) -> Bool { value != null }
pure changed(value) {
  var local = value
  let present = known(value)
  local = {name: "changed"}
  {present: present, local: local}
}
pure changed_first(value) {
  var local = value
  local = {name: "changed"}
  let present = known(value)
  {present: present, local: local}
}
let missing: Item? = null
let original: Item? = {name: "original"}
let absent = changed(null)
let typed = changed(missing)
let present = changed(original)
let reversed = changed_first(null)
assert ! absent.present and ! typed.present and ! reversed.present
assert present.present
assert absent.local == {name: "changed"}
assert typed.local == {name: "changed"}
assert present.local == {name: "changed"}
assert reversed.local == {name: "changed"}
assert original?.name == "original"
""")?
  assert output.success, output.stderr
  let rejected = test.run_xsh(ctx, r"""type Item = {name: Str}
pure known(value: Item?) -> Bool { value != null }
pure changed(value) {
  var local = value
  let present = known(value)
  local = {name: 7}
  {present: present, local: local}
}
let _ = changed({name: "original"})
""")?
  assert ! rejected.success, rejected.stdout
  assert "[check." in rejected.stderr, rejected.stderr
}

test test_copied_nullable_storage_keeps_forwarded_and_independent_calls { |ctx|
  let output = test.expect(ctx, r"""type Item = {name: Str}
pure known(value: Item?) -> Bool { value != null }
pure change(value) { var local = value; local = {name: "updated"}; let present = known(value); {present, local} }
pure forward(value) { change(value) }
let original = {name: "original"}
let present = forward(original)
let missing = forward(null)
let typed: Item? = null
let other = forward(typed)
assert present.present and ! missing.present and ! other.present
assert present.local.name == "updated" and missing.local == {name: "updated"}
assert original.name == "original"
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_copied_storage_accepts_null_without_an_optional_consumer { |ctx|
  let output = test.expect(ctx, r"""pure replace(value) { var local = value; local = {name: "updated"}; local }
let actual = replace(null)
assert actual == {name: "updated"}
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_copied_storage_waits_for_all_call_arguments { |ctx|
  let output = test.expect(ctx, r"""pure replace(value, replacement) { var local = value; local = replacement; local }
pure reverse(replacement, value) { var local = value; local = replacement; local }
let left = replace(null, {name: "left"})
let right = reverse({name: "right"}, null)
assert left == {name: "left"} and right == {name: "right"}
""", status: 0)?
  assert output.stderr == "", output.stderr
  let invalid = test.run_xsh(ctx, r"""type Item = {name: Str}
pure known(value: Item?) -> Bool { value != null }
pure replace(value, replacement) { var local = value; local = replacement; let _ = known(value); local }
let _ = replace(null, {name: 7})
""")?
  assert ! invalid.success, invalid.stdout
}

test test_copied_storage_combines_multiple_replacement_bounds { |ctx|
  let output = test.expect(ctx, r"""pure replace(value, first, second) { var local = value; local = first; local = second; local }
let missing = replace(null, {name: "first"}, {name: "second"})
let present = replace({name: "start"}, {name: "first"}, {name: "last"})
assert missing == {name: "second"} and present == {name: "last"}
""", status: 0)?
  assert output.stderr == "", output.stderr
  let invalid = test.run_xsh(ctx, r"""pure replace(value, first, second) { var local = value; local = first; local = second; local }
let _ = replace(null, [1], ["wrong"])
""")?
  assert ! invalid.success, invalid.stdout
}

test test_copied_storage_selects_signed_witness_independent_of_write_order { |ctx|
  let output = test.expect(ctx, r"""pure change(value, unsigned: UInt, signed: Int) { var local = value; local = unsigned; local = signed; local }
pure reverse(value, unsigned: UInt, signed: Int) { var local = value; local = signed; local = unsigned; local }
assert change(null, 1, -1) == -1
assert reverse(null, 1, -1) == 1
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_copied_storage_requirements_stay_in_the_selected_guard { |ctx|
  let output = test.expect(ctx, r"""pure change(value, enabled) {
  if let flag = enabled {
    var local = value
    local = {name: "updated"}
    local
  } else { value }
}
let none: Int? = null
let some: Int? = 1
let skipped = change(null, none)
let selected = change(null, some)
assert skipped == null
assert selected == {name: "updated"}
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_copied_nullable_storage_uses_the_compatible_record_upper_bound { |ctx|
  let output = test.expect(ctx, r"""type Item = {name: Str}
pure known(value: Item?) -> Bool { value != null }
pure replace(value, replacement) {
  var local = value
  local = replacement
  let present = known(value)
  {present, local}
}
let result = replace(null, {name: "kept", extra: 1})
assert ! result.present and (result.local?.name ?? "") == "kept"
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_copied_root_storage_keeps_unsigned_checks { |ctx|
  let program = r"""proc replace(value, replacement: Int) [error] { var local = value; local = replacement; local }
proc main(...args: List[Str]) [io, error] {
  let original: UInt = 3
  let updated = replace(original, 2)?
  print $updated $original
  if args.len() > 0 { let _ = replace(original, -1)? }
}
"""
  let good = test.expect(ctx, program, status: 0, args: [])?
  assert good.stdout == "2 3\n", good.stdout
  let bad = test.expect(ctx, program, status: 3, args: ["negative"])?
  assert bad.stdout == "2 3\n", bad.stdout
  let nullable = r"""pure known(value: UInt?) -> Bool { value != null }
proc change(value, replacement: Int) [error] { var local = value; local = replacement; let _ = known(value); local }
proc main(...args: List[Str]) [io, error] {
  let result = change(null, 2)?
  print ${result ?? 0}
  if args.len() > 0 { let _ = change(null, -1)? }
}
"""
  let valid = test.expect(ctx, nullable, status: 0, args: [])?
  assert valid.stdout == "2\n", valid.stdout
  let rejected = test.expect(ctx, nullable, status: 3, args: ["negative"])?
  assert rejected.stdout == "2\n", rejected.stdout
}

test test_copied_storage_joins_shared_input_arguments_in_either_order { |ctx|
  let output = test.expect(ctx, r"""pure replace(first, second, replacement) {
  var local = first
  local = replacement
  let _ = first == second
  local
}
let first = replace({name: "original"}, null, {name: "updated"})
let second = replace(null, {name: "original"}, {name: "updated"})
assert first == {name: "updated"} and second == {name: "updated"}
pure keep(first, second) { var local = first; local = first; let _ = first == second; local }
let left = keep([1], [])
let right = keep([], [1])
assert left == [1] and right == []
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_cyclic_copied_storage_combines_call_inputs { |ctx|
  let output = test.expect(ctx, r"""pure replace(first, second) {
  var left = first
  left = second
  var right = second
  right = first
  {left, right}
}
let missing = replace(null, null)
let left: Null = missing.left
let right: Null = missing.right
assert left == null and right == null
let forward = replace(null, {name: "present"})
let reverse = replace({name: "present"}, null)
assert forward.left == {name: "present"} and forward.right == null
assert reverse.left == null and reverse.right == {name: "present"}
type Item = {name: Str}
let absent: Item? = null
let typed = replace(absent, {name: "typed"})
let typed_reverse = replace({name: "typed"}, absent)
assert typed.left == {name: "typed"} and typed.right == null
assert typed_reverse.left == null and typed_reverse.right == {name: "typed"}
pure relay(first, second) { replace(first, second) }
assert relay(null, {name: "forwarded"}).left == {name: "forwarded"}
pure rotate(first, second, third) {
  var left = first; left = second
  var middle = second; middle = third
  var right = third; right = first
  {left, middle, right}
}
let rotated = rotate(null, {name: "middle"}, null)
assert rotated.left == {name: "middle"} and rotated.middle == null and rotated.right == null
let lists = rotate(null, [1], [2])
assert lists.left == [1] and lists.middle == [2] and lists.right == null
let first_map: Map[Str, Int] = {first: 1}
let second_map: Map[Str, Int] = {second: 2}
let maps = rotate(null, first_map, second_map)
assert maps.left == first_map and maps.middle == second_map and maps.right == null
let sets = rotate(null, {1, 2}, {3, 4})
assert sets.left == {1, 2} and sets.middle == {3, 4} and sets.right == null
pure linked(first, second, third, fourth) {
  var a = first; a = second
  var b = second; b = first; b = third
  var c = third; c = fourth
  var d = fourth; d = third
  {a, b, c, d}
}
let dependent = linked(null, null, null, {name: "external"})
assert dependent.a == null and dependent.b == null and dependent.c == {name: "external"} and dependent.d == null
pure fed(first, second, third, replacement) {
  var a = first; a = second
  var b = second; b = first; b = third
  var c = third; c = replacement
  {a, b, c}
}
let feeder = fed(null, null, null, {name: "external"})
assert feeder.a == null and feeder.b == null and feeder.c == {name: "external"}
let present_feeder = fed(null, null, {name: "initial"}, {name: "external"})
assert present_feeder.b == {name: "initial"} and present_feeder.c == {name: "external"}
""", status: 0)?
  assert output.stderr == "", output.stderr
  let invalid = test.run_xsh(ctx, r"""pure replace(first, second) {
  var left = first; left = second
  var right = second; right = first
  {left, right}
}
let _ = replace([1], ["wrong"])
""")?
  assert ! invalid.success, invalid.stdout
  for inputs in [
    "let integers: List[Int] = [1]\nlet nullable: List[Int?] = [null]\nlet _ = rotate(null, integers, nullable)\n",
    "let integers: Map[Str, Int] = {name: 1}\nlet nullable: Map[Str, Int?] = {name: null}\nlet _ = rotate(null, integers, nullable)\n",
    "let signed: Set[Int] = {1, 2}\nlet unsigned: Set[UInt] = {3, 4}\nlet _ = rotate(null, signed, unsigned)\n",
    "let dynamic: Any = 3\nlet _ = rotate(null, dynamic, null)\n",
  ] {
    let containers = test.run_xsh(ctx, r"""pure rotate(first, second, third) {
  var left = first; left = second
  var middle = second; middle = third
  var right = third; right = first
  {left, middle, right}
}
""" + inputs)?
    assert ! containers.success, containers.stdout
  }
  let checked = r"""proc fed(first, second, third, replacement: Int) [error] {
  var a = first; a = second
  var b = second; b = first; b = third
  var c = third; c = replacement
  {a, b, c}
}
proc main(...args: List[Str]) [io, error] {
  let original: UInt = 3
  let valid = fed(null, null, original, 2)?
  print ${valid.c} $original
  if args.len() > 0 { let _ = fed(null, null, original, -1)? }
}
"""
  let good = test.expect(ctx, checked, status: 0, args: [])?
  assert good.stdout == "2 3\n", good.stdout
  let bad = test.expect(ctx, checked, status: 3, args: ["negative"])?
  assert bad.stdout == "2 3\n", bad.stdout
}
