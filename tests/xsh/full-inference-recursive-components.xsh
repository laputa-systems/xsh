test test_full_inference_self_recursive_scc_generalizes_after_checking { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure descend(value, count) {
  if count == 0 { value } else { descend(value, count - 1) }
}
let integer = descend(42, 2)
let text = descend("demo", 3)
assert integer == 42
assert text == "demo"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_mutual_recursive_scc_is_declaration_order_independent { |ctx|
  let forward = test.run_xsh(
    ctx,
    r"""pure left(value, count) {
  if count == 0 { value } else { right(value, count - 1) }
}
pure right(value, count) {
  if count == 0 { value } else { left(value, count - 1) }
}
let integer = left(17, 4)
let text = right("mutual", 3)
assert integer == 17
assert text == "mutual"
print $integer
print $text
""",
  )?
  let reverse = test.run_xsh(
    ctx,
    r"""pure right(value, count) {
  if count == 0 { value } else { left(value, count - 1) }
}
pure left(value, count) {
  if count == 0 { value } else { right(value, count - 1) }
}
let integer = left(17, 4)
let text = right("mutual", 3)
assert integer == 17
assert text == "mutual"
print $integer
print $text
""",
  )?
  assert forward.success, forward.stderr
  assert reverse.success, reverse.stderr
  assert forward.stdout == "17\nmutual\n"
  assert reverse.stdout == forward.stdout
}

test test_full_inference_recursive_peer_callback_requirements_remain_independent { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure left(action, other, value, count) {
  if count == 0 { action(value) } else { right(action, other, value, count - 1) }
}
pure right(action, other, value, count) {
  if count == 0 { other(payload: value) } else { left(action, other, value, count - 1) }
}
pure integer(value: Int) -> Int { value + 1 }
pure integer_other(payload: Int) -> Int { payload + 2 }
pure text(value: Str) -> Str { value + "!" }
pure text_other(payload: Str) -> Str { payload + "?" }
assert left(integer, integer_other, 41, 2) == 42
assert right(text, text_other, "demo", 2) == "demo?"
""",
  )?
  assert output.success, output.stderr
  test.expect(
    ctx,
    r"""pure left(action, other, value, count) {
  if count == 0 { action(value) } else { right(action, other, value, count - 1) }
}
pure right(action, other, value, count) {
  if count == 0 { other(payload: value) } else { left(action, other, value, count - 1) }
}
pure integer(value: Int) -> Int { value + 1 }
pure text_other(payload: Str) -> Str { payload + "?" }
let invalid = left(integer, text_other, 41, 2)
""",
    status: 2,
    stderr: ["[check.callable-mismatch]"],
  )?
}

test test_full_inference_recursive_proc_preserves_independent_result_instances { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc descend(value, count) {
  if count == 0 { return value }
  descend(value, count - 1)
}
assert descend(42, 2)? == 42
assert descend("demo", 3)? == "demo"
pure inner_ok() -> Result[Int, Str] { Ok(7) }
pure inner_error() -> Result[Int, Str] { Err("stored") }
assert descend(inner_ok(), 0)? == inner_ok()
assert descend(inner_ok(), 2)? == inner_ok()
assert descend(inner_error(), 0)? == inner_error()
assert descend(inner_error(), 2)? == inner_error()
""",
  )?
  assert output.success, output.stderr
}

test test_full_inference_recursive_peer_retains_callback_effects { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc left(action, value, count) {
  if count == 0 { return value }
  right(action, value, count - 1)
}
proc right(action, value, count) {
  if count == 0 { return action(value) }
  left(action, value, count - 1)
}
proc report(value: Str) [io] -> Str { print $value; return value }
let result = left(report, "peer", 1)?
assert result == "peer"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "peer\n"
  test.expect(
    ctx,
    r"""proc left(action, value, count) [] {
  if count == 0 { return value }
  right(action, value, count - 1)
}
proc right(action, value, count) {
  if count == 0 { return action(value) }
  left(action, value, count - 1)
}
proc report(value: Str) [io] -> Str { print $value; return value }
let result = left(report, "peer", 1)?
""",
    status: 2,
    stderr: ["[check.effect-violation]"],
  )?
}

test test_full_inference_recursive_capture_keeps_nested_mutable_state_monomorphic { |ctx|
  let accepted = test.run_xsh(
    ctx,
    r"""var shared = {items: [1]}
proc store(value, count) {
  if count == 0 {
    shared.items = [value]
    return value
  }
  store(value, count - 1)
}
assert store(42, 2)? == 42
assert shared.items == [42]
""",
  )?
  assert accepted.success, accepted.stderr
  test.expect(
    ctx,
    r"""var shared = {items: [1]}
proc store(value, count) {
  if count == 0 {
    shared.items = [value]
    return value
  }
  store(value, count - 1)
}
let integer = store(42, 2)?
let text = store("wrong", 2)?
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_recursive_pure_indexes_a_generic_receiver { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure total(items, count) {
  if count == 0 { 0 } else { items[0] + total(items, count - 1) }
}
pure weigh(items, count) {
  if count == 0 { 0.0 } else { items[0] * 0.5 + weigh(items, count - 1) }
}
let integers = total([1, 2, 3], 2)
let floats = weigh([1.0, 2.0], 2)
assert integers == 2
assert floats == 1.0
print $integers
print $floats
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "2\n1\n"
}

test test_full_inference_rejected_recursive_proc_reports_its_own_error_not_an_effect_fault { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc descend(count, mode) {
  if mode != null { exit 1 }
  let wrong: Int = "text"
  if count > 0 { descend(count - 1, mode) }
}
""",
  )?
  assert output.status == 2, output.stderr
  assert "[check.type-mismatch]" in output.stderr, output.stderr
  assert "[check.local-inference]" not in output.stderr, output.stderr
}

test test_full_inference_self_recursive_proc_keeps_its_guard_over_an_unresolved_parameter { |ctx|
  let inferred = test.run_xsh(
    ctx,
    r"""pure render(name, depth: Int) -> Str {
  if depth > 0 { return render(name, depth - 1) }
  if let found = name { found } else { "none" }
}
proc build(name, depth: Int) [error] {
  if depth > 0 { build(name, depth - 1) }
  if let found = name { assert found.byte_len() == 4 }
}
print render("text", 2)
print render(null, 2)
build("deep", 1)
print "built"
""",
  )?
  let annotated = test.run_xsh(
    ctx,
    r"""pure render(name: Str?, depth: Int) -> Str {
  if depth > 0 { return render(name, depth - 1) }
  if let found = name { found } else { "none" }
}
proc build(name: Str?, depth: Int) [error] {
  if depth > 0 { build(name, depth - 1) }
  if let found = name { assert found.byte_len() == 4 }
}
print render("text", 2)
print render(null, 2)
build("deep", 1)
print "built"
""",
  )?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "text\nnone\nbuilt\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_self_recursive_guard_keeps_a_local_view_binder { |ctx|
  let source = r"""pure render(name: NAME, depth: Int) -> Str {
  if depth > 0 { return render(name, depth - 1) }
  if let found = name { f"got {found}" } else { "none" }
}
print render("text", 2)
print render(null, 1)
"""
  let inferred = test.run_xsh(ctx, source.replace("name: NAME", with: "name") + "print render(5, 1)\n")?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == annotated.stdout + "got 5\n"
  assert annotated.stdout == "got text\nnone\n"
}

test test_full_inference_recursive_null_argument_makes_the_shared_parameter_nullable { |ctx|
  let inferred = test.run_xsh(
    ctx,
    r"""pure thin(value, depth: Int) {
  if depth > 0 { return thin(null, depth - 1) }
  value
}
proc walk(target, depth: Int) [error] {
  if depth > 0 { walk(null, depth - 1) }
  if let found = target { assert found.byte_len() == 4 }
}
assert thin("text", 2) == null
if let kept = thin("text", 0) { print $kept }
walk("deep", 0)
walk("deep", 2)
print "walked"
""",
  )?
  let annotated = test.run_xsh(
    ctx,
    r"""pure thin(value: Str?, depth: Int) -> Str? {
  if depth > 0 { return thin(null, depth - 1) }
  value
}
proc walk(target: Str?, depth: Int) [error] {
  if depth > 0 { walk(null, depth - 1) }
  if let found = target { assert found.byte_len() == 4 }
}
assert thin("text", 2) == null
if let kept = thin("text", 0) { print $kept }
walk("deep", 0)
walk("deep", 2)
print "walked"
""",
  )?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "text\nwalked\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_recursive_null_argument_rejects_a_non_nullable_use { |ctx|
  test.expect(ctx, r"""pure thin(value, depth: Int) {
  if depth > 0 { return thin(null, depth - 1) }
  value.byte_len()
}
""", status: 2, stderr: ["[check.optional-method]"])?
}

test test_full_inference_mutual_recursion_shares_guards_on_one_parameter { |ctx|
  let body = r"""
  if depth > 0 { return right(name, depth - 1) }
  if let found = name { f"left {found}" } else { "left none" }
}
pure right(name: NAME, depth: Int) -> Str {
  if depth > 0 { return left(name, depth - 1) }
  if let found = name { f"right {found}" } else { "right none" }
}
print left("a", 1)
print left(null, 2)
print right("b", 0)
print right(null, 3)
"""
  let inferred = test.run_xsh(ctx, "pure left(name, depth: Int) -> Str {" + body.replace("name: NAME", with: "name"))?
  let annotated = test.run_xsh(ctx, "pure left(name: Str?, depth: Int) -> Str {" + body.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "right a\nleft none\nright b\nleft none\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_three_member_cycle_shares_guards { |ctx|
  let source = r"""pure first(name: NAME, depth: Int) -> Str {
  if depth > 0 { return second(name, depth - 1) }
  if let found = name { f"first {found}" } else { "first none" }
}
pure second(name: NAME, depth: Int) -> Str {
  if depth > 0 { return third(name, depth - 1) }
  if let found = name { f"second {found}" } else { "second none" }
}
pure third(name: NAME, depth: Int) -> Str {
  if depth > 0 { return first(name, depth - 1) }
  if let found = name { f"third {found}" } else { "third none" }
}
print first("a", 2)
print second(null, 2)
print third("c", 4)
"""
  let inferred = test.run_xsh(ctx, source.replace("name: NAME", with: "name"))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "third a\nfirst none\nfirst c\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_mutual_recursion_with_a_guard_in_one_member { |ctx|
  let source = r"""pure render(name: NAME, depth: Int) -> Str {
  if depth > 0 { return relay(name, depth - 1) }
  if let found = name { f"found {found}" } else { "none" }
}
pure relay(name: NAME, depth: Int) -> Str { render(name, depth) }
print relay("a", 2)
print relay(null, 1)
print render("b", 3)
"""
  let inferred = test.run_xsh(ctx, source.replace("name: NAME", with: "name"))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "found a\nnone\nfound b\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_mutual_recursion_guards_different_parameters { |ctx|
  let source = r"""pure left(name: NAME, count: COUNT, depth: Int) -> Str {
  if depth > 0 { return right(count, name, depth - 1) }
  if let found = name { f"name {found}" } else { "no name" }
}
pure right(count: COUNT, name: NAME, depth: Int) -> Str {
  if depth > 0 { return left(name, count, depth - 1) }
  if let found = count { f"count {found}" } else { "no count" }
}
print left("a", 3, 0)
print left("a", 3, 1)
print left(null, null, 2)
print right(null, "b", 1)
print right(4, null, 0)
"""
  let inferred = test.run_xsh(ctx, source.replace("name: NAME", with: "name").replace("count: COUNT", with: "count"))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?").replace("COUNT", with: "Int?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "name a\ncount 3\nno name\nname b\ncount 4\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_recursive_component_mixes_self_and_peer_guard_calls { |ctx|
  let source = r"""pure left(name: NAME, depth: Int) -> Str {
  if depth > 2 { return left(name, depth - 1) }
  if depth > 0 { return right(name, depth - 1) }
  if let found = name { f"left {found}" } else { "left none" }
}
pure right(name: NAME, depth: Int) -> Str {
  if depth > 0 { return left(name, depth - 1) }
  if let found = name { f"right {found}" } else { "right none" }
}
proc walk(target: NAME, depth: Int) [error] {
  if depth > 1 { walk(target, depth - 1) }
  if depth == 1 { visit(target, 0) }
  if let found = target { assert found.byte_len() == 4 }
}
proc visit(target: NAME, depth: Int) [error] {
  if depth == 0 { walk(target, depth) }
  if let found = target { assert found.byte_len() == 4 }
}
print left("a", 4)
print left(null, 5)
print right("b", 2)
walk("deep", 3)
walk(null, 3)
visit("deep", 0)
print "walked"
"""
  let inferred = test.run_xsh(ctx, source.replace("name: NAME", with: "name").replace("target: NAME", with: "target"))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "left a\nleft none\nright b\nwalked\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_self_recursive_proc_forwards_a_callee_guard { |ctx|
  let source = r"""pure show(value: NAME) -> Str {
  if let found = value { f"got {found}" } else { "none" }
}
pure walk(value: NAME, depth: Int) -> Str {
  if depth > 0 { return walk(value, depth - 1) }
  show(value)
}
proc visit(target: NAME, depth: Int) [error] {
  if depth > 0 { visit(target, depth - 1) }
  assert show(target).byte_len() > 3
}
print walk("a", 2)
print walk(null, 1)
visit("deep", 2)
visit(null, 1)
"""
  let inferred = test.run_xsh(ctx, source.replace("value: NAME", with: "value").replace("target: NAME", with: "target"))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "got a\nnone\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_self_recursive_callable_keeps_nested_guards { |ctx|
  let source = r"""pure left(name: NAME, other: NAME, depth: Int) -> Str {
  if depth > 0 { return left(name, other, depth - 1) }
  if let found = name { if let o = other { f"left {found} {o}" } else { f"left {found}" } } else { "left none" }
}
pure locate(name: NAME, depth: Int) -> Str {
  if depth > 0 { return locate(name, depth - 1) }
  if let found = name { if let at = found.find("b") { f"at {at}" } else { f"no b in {found}" } } else { "none" }
}
print left("a", "b", 1)
print left("a", null, 2)
print left(null, "c", 1)
print locate("abc", 1)
print locate("xyz", 1)
print locate(null, 1)
"""
  let inferred = test.run_xsh(ctx, source.replace(": NAME", with: ""))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "left a b\nleft a\nleft none\nat 1\nno b in xyz\nnone\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_mutual_recursion_keeps_nested_guards { |ctx|
  let source = r"""pure left(name: NAME, other: NAME, depth: Int) -> Str {
  if depth > 0 { return right(name, other, depth - 1) }
  if let found = name { if let o = other { f"left {found} {o}" } else { f"left {found}" } } else { "left none" }
}
pure right(name: NAME, other: NAME, depth: Int) -> Str {
  if depth > 0 { return left(name, other, depth - 1) }
  if let found = name { f"right {found}" } else { "right none" }
}
print left("a", "b", 1)
print left("a", null, 2)
print right(null, "c", 1)
"""
  let inferred = test.run_xsh(ctx, source.replace(": NAME", with: ""))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "right a\nleft a\nleft none\n"
  assert inferred.stdout == annotated.stdout
}

test test_full_inference_mutual_recursion_keeps_nested_guards_on_a_found_value { |ctx|
  let source = r"""pure left(name: NAME, depth: Int) -> Str {
  if depth > 0 { return right(name, depth - 1) }
  if let found = name { if let at = found.find("b") { f"left {at}" } else { f"left {found}" } } else { "left none" }
}
pure right(name: NAME, depth: Int) -> Str {
  if depth > 0 { return left(name, depth - 1) }
  if let found = name { f"right {found}" } else { "right none" }
}
print left("abc", 1)
print left(null, 2)
print left("abc", 2)
print left("xyz", 2)
"""
  let inferred = test.run_xsh(ctx, source.replace(": NAME", with: ""))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "right abc\nleft none\nleft 1\nleft xyz\n"
  assert inferred.stdout == annotated.stdout
}
