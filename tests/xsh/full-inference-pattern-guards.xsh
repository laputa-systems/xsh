test test_inferred_optional_bindings_keep_payload_requirements_in_the_present_branch { |ctx|
  let good = test.expect(ctx, r"""pure render(label) -> Str {
  if let note = label { f"{note}" } else { "absent" }
}
type Row = {label: Str?}
pure identity(value) { value }
pure render_call(label) -> Str {
  if let note = identity(label) { f"{note}" } else { "absent" }
}
pure forward(value) { render_call(value) }
pure present_or_null(value) { if let note = value { note } else { null } }
pure exists(value) -> Bool { if let _ = value { true } else { false } }
var reads = 0
proc observed(value) [] { reads += 1; value }
proc counted(value) [error] -> Result[Str] {
  if let note = observed(value)? { f"{note}" } else { "absent" }
}
pure render_row(row) -> Str {
  if let note = row.label { f"{note}" } else { "absent" }
}
let present: Str? = "typed"
let missing: Str? = null
let present_row: Row = {label: present}
let missing_row: Row = {label: missing}
assert render(null) == "absent"
assert render_call(null) == "absent"
assert render_call("once") == "once"
assert forward(null) == "absent"
assert forward("forwarded") == "forwarded"
assert present_or_null(null) == null
assert present_or_null("kept") == "kept"
assert present_or_null(9) == 9
assert !exists(null)
assert exists(7)
assert counted(null)? == "absent"
assert counted("once")? == "once"
assert reads == 2
assert render("text") == "text"
assert render(7) == "7"
assert render(present) == "typed"
assert render(missing) == "absent"
assert render_row(present_row) == "typed"
assert render_row(missing_row) == "absent"
""", status: 0)?
  assert good.stderr == "", good.stderr
  test.expect(ctx, r"""pure render(label) -> Str {
  if let note = label { f"{note}" } else { "absent" }
}
let invalid = render({value: 7})
""", status: 2, stderr: ["[check.callable-mismatch]"])?
}

test test_guards_with_a_known_source_stay_forwarded_through_generic_callers { |ctx|
  let good = test.expect(ctx, r"""pure inner(name, compress) -> Str {
  if let program = compress { f"{name}:{program}" } else { "plain" }
}
pure checked(name, compress) -> Str {
  if compress != null { f"{name}:{compress}" } else { "plain" }
}
pure absent(name) -> Str { inner(name, null) }
pure present(name) -> Str { inner(name, "gz") }
pure relay(name, compress: Str?) -> Str { inner(name, compress) }
pure relay_twice(name, compress: Str?) -> Str { relay(name, compress) }
pure checked_relay(name, compress: Str?) -> Str { checked(name, compress) }
let none: Str? = null
let some: Str? = "xz"
assert absent({a: 1}) == "plain"
assert absent("a") == "plain"
assert present("a") == "a:gz"
assert present(3) == "3:gz"
assert relay("a", "gz") == "a:gz"
assert relay("a", none) == "plain"
assert relay_twice(1, some) == "1:xz"
assert relay_twice(1, null) == "plain"
assert checked_relay("a", "gz") == "a:gz"
assert checked_relay("a", null) == "plain"
""", status: 0)?
  assert good.stderr == "", good.stderr
  let rejected = [
    "present({a: 1})",
    "relay({a: 1}, \"gz\")",
    "relay_twice({a: 1}, null)",
    "checked_relay({a: 1}, \"gz\")",
  ]
  for call in rejected {
    test.expect(ctx, f"""pure inner(name, compress) -> Str {{
  if let program = compress {{ f"{{name}}:{{program}}" }} else {{ "plain" }}
}}
pure checked(name, compress) -> Str {{
  if compress != null {{ f"{{name}}:{{compress}}" }} else {{ "plain" }}
}}
pure present(name) -> Str {{ inner(name, "gz") }}
pure relay(name, compress: Str?) -> Str {{ inner(name, compress) }}
pure relay_twice(name, compress: Str?) -> Str {{ relay(name, compress) }}
pure checked_relay(name, compress: Str?) -> Str {{ checked(name, compress) }}
let invalid = {call}
""", status: 2, stderr: ["[check.callable-mismatch]"])?
  }
}

test test_inferred_guarded_calls_with_effect_clauses_keep_each_guard_scope_own_effects { |ctx|
  let inferred = test.run_xsh(
    ctx,
    r"""type Options = {output: Path?, output_nocache: Bool, conv: List[Str], ibs: Int}
type Copied = {complete: Int, discard_failed: Bool}

proc quoted_path(file_path: Path) [env] -> Str {
  file_path.display()
}

proc discard_caches(opts) [fs, process, env] -> Bool {
  var failed = false
  if opts.output_nocache {
    let label = if let output = opts.output { quoted_path(output) } else { "standard output" }
    if label == "" { failed = true }
  }
  failed
}

proc plain_copy(opts, limit: Int) [fs, error, io, process, env] -> Result[Copied] {
  let dest = if let output = opts.output { output } else { fp"/dev/stdout" }
  if opts.output != null and "append" in opts.conv and limit > 1 {
    let copied = dest.metadata()?.size
    return Ok({complete: copied / opts.ibs, discard_failed: discard_caches(opts)})
  }
  if limit > 5 and opts.output != null { dest.write(b"")? }
  Ok({complete: 0, discard_failed: discard_caches(opts)})
}
print "defined"
""",
  )?
  let annotated = test.run_xsh(
    ctx,
    r"""type Options = {output: Path?, output_nocache: Bool, conv: List[Str], ibs: Int}
type Copied = {complete: Int, discard_failed: Bool}

proc quoted_path(file_path: Path) [env] -> Str {
  file_path.display()
}

proc discard_caches(opts: Options) [fs, process, env] -> Bool {
  var failed = false
  if opts.output_nocache {
    let label = if let output = opts.output { quoted_path(output) } else { "standard output" }
    if label == "" { failed = true }
  }
  failed
}

proc plain_copy(opts: Options, limit: Int) [fs, error, io, process, env] -> Result[Copied] {
  let dest = if let output = opts.output { output } else { fp"/dev/stdout" }
  if opts.output != null and "append" in opts.conv and limit > 1 {
    let copied = dest.metadata()?.size
    return Ok({complete: copied / opts.ibs, discard_failed: discard_caches(opts)})
  }
  if limit > 5 and opts.output != null { dest.write(b"")? }
  Ok({complete: 0, discard_failed: discard_caches(opts)})
}
print "defined"
""",
  )?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "defined\n"
}

test test_inferred_guard_calls_inside_a_guard_resolve_their_local_binders_from_the_call_output { |ctx|
  let body = r"""type Options = {output: Path?, output_nocache: Bool, ibs: Int}
pure quoted_path(file_path: Path) -> Str { file_path.display() }
pure discard_caches(OPTS) -> Bool {
  var failed = false
  if opts.output_nocache {
    let label = if let output = opts.output { quoted_path(output) } else { "standard output" }
    if label == "" { failed = true }
  }
  failed
}
pure plain_copy(OPTS, limit: Int) -> Bool {
  if opts.output != null and limit > 1 { return discard_caches(opts) }
  discard_caches(opts)
}
let standard: Options = {output: null, output_nocache: true, ibs: 512}
let named: Options = {output: p"/dev/null", output_nocache: true, ibs: 512}
print $plain_copy(standard, 0)
print $plain_copy(named, 2)
"""
  let inferred = test.run_xsh(ctx, body.replace("OPTS", with: "opts"))?
  let annotated = test.run_xsh(ctx, body.replace("OPTS", with: "opts: Options"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "false\nfalse\n"
  assert annotated.stdout == inferred.stdout
}

test test_inferred_guarded_output_survives_rechecking_the_expression { |ctx|
  let body = r"""proc launch(WORDS, ENVIRONMENT) [fs, process, env, error] {
  let plan = process.command_argv(words[0], words)
  let replaced = if let variables = environment { unix.exec_env(plan, variables) } else { unix.exec(plan) }
  if let Err(failure) = replaced { exit 1 }
}
print "defined"
"""
  let inferred = test.run_xsh(ctx, body.replace("WORDS", with: "words").replace("ENVIRONMENT", with: "environment"))?
  let annotated = test.run_xsh(ctx, body.replace("WORDS", with: "words: List[Str]").replace("ENVIRONMENT", with: "environment: Map[Str, Str]?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "defined\n"
  assert annotated.stdout == inferred.stdout
}

test test_inferred_guarded_source_reads_store_into_optional_fields_as_present_values { |ctx|
  let body = r"""type Output = {target: Str, linked: Bool}
type Check = {sink: Output?, removed: Bool}
pure check(PREVIOUS, current: Output) -> Check {
  if let before = previous {
    if current.target != before.target { return {sink: current, removed: false} }
    if ! current.linked { return {sink: previous, removed: true} }
  }
  {sink: previous, removed: false}
}
let held: Output = {target: "a", linked: true}
let moved: Output = {target: "b", linked: true}
let unlinked: Output = {target: "a", linked: false}
let missing: Output? = null
if let moved_sink = check(held, moved).sink { print $moved_sink.target }
print $check(held, unlinked).removed
print $check(held, held).removed
assert check(missing, held).sink == null
print "absent"
"""
  let inferred = test.run_xsh(ctx, body.replace("PREVIOUS", with: "previous"))?
  let annotated = test.run_xsh(ctx, body.replace("PREVIOUS", with: "previous: Output?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "b\ntrue\nfalse\nabsent\n"
  assert annotated.stdout == inferred.stdout
}

test test_inferred_nested_optional_bindings_on_independent_sources_match_annotations { |ctx|
  let source = r"""pure value_form(name: NAME, other: NAME) -> Str {
  if let found = name { if let o = other { f"{found} {o}" } else { f"{found}" } } else { "none" }
}
pure early_return_form(name: NAME, other: NAME) -> Str {
  if let found = name {
    if let o = other { return f"{found} {o}" }
    return f"{found}"
  }
  "none"
}
pure bound_form(name: NAME, other: NAME) -> Str {
  if let found = name {
    let tail = if let o = other { f"{found} {o}" } else { f"{found}" }
    return tail
  }
  "none"
}
pure else_form(name: NAME, other: NAME) -> Str {
  if let found = name { f"{found}" } else { if let o = other { f"-{o}" } else { "none" } }
}
pure deep_form(a: NAME, b: NAME, c: NAME) -> Str {
  if let x = a { if let y = b { if let z = c { f"{x}{y}{z}" } else { f"{x}{y}" } } else { f"{x}" } } else { "none" }
}
print value_form("a", "b")
print value_form("a", null)
print value_form(null, "b")
print early_return_form("a", "b")
print early_return_form("a", null)
print early_return_form(null, "b")
print bound_form("a", "b")
print bound_form("a", null)
print bound_form(null, "b")
print else_form("a", "b")
print else_form(null, "b")
print else_form(null, null)
print deep_form("a", "b", "c")
print deep_form("a", "b", null)
print deep_form("a", null, "c")
print deep_form(null, "b", "c")
"""
  let inferred = test.run_xsh(ctx, source.replace(": NAME", with: ""))?
  let annotated = test.run_xsh(ctx, source.replace("NAME", with: "Str?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "a b\na\nnone\na b\na\nnone\na b\na\nnone\na\n-b\nnone\nabc\nab\na\nnone\n"
  assert inferred.stdout == annotated.stdout
}

test test_inferred_unused_optional_bindings_match_annotations { |ctx|
  let source = r"""pure value_form(a: SUBJECT) -> Str { if let x = a { "present" } else { "absent" } }
pure return_form(a: SUBJECT) -> Str {
  if let x = a { return "present" }
  "absent"
}
pure nested_form(a: SUBJECT, b: SUBJECT) -> Str {
  if let x = a { if let y = b { "both" } else { "first" } } else { "none" }
}
pure forwarded_form(a: SUBJECT) -> Str { value_form(a) }
let some: Int? = 4
let none: Int? = null
print value_form(5)
print value_form(null)
print value_form(some)
print value_form(none)
print return_form(5)
print return_form(null)
print nested_form(1, 2)
print nested_form(1, null)
print nested_form(null, 2)
print forwarded_form(5)
print forwarded_form(null)
"""
  let inferred = test.run_xsh(ctx, source.replace(": SUBJECT", with: ""))?
  let annotated = test.run_xsh(ctx, source.replace("SUBJECT", with: "Int?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "present\nabsent\npresent\nabsent\npresent\nabsent\nboth\nfirst\nnone\npresent\nabsent\n"
  assert inferred.stdout == annotated.stdout
}

test test_inferred_unused_optional_bindings_accept_each_payload_type { |ctx|
  let good = test.expect(ctx, r"""pure present(a) -> Str { if let x = a { "present" } else { "absent" } }
assert present(1) == "present"
assert present("text") == "present"
assert present([1, 2]) == "present"
assert present({k: 1}) == "present"
assert present(null) == "absent"
""", status: 0)?
  assert good.stderr == "", good.stderr
}

test test_inferred_null_matches_on_an_unresolved_subject_match_annotations { |ctx|
  let source = r"""pure null_first(a: SUBJECT) -> Str { match a { null => "absent", else => "present" } }
pure bound(a: SUBJECT) -> Int { match a { null => 0, x => 1 } }
pure value_arm(a: SUBJECT) -> Int { match a { null => 0, 7 => 7, else => 1 } }
pure nested(a: SUBJECT) -> Int { match [a] { [null] => 0, else => 1 } }
pure forwarded(a: SUBJECT) -> Str { null_first(a) }
let some: Int? = 7
let none: Int? = null
print null_first(7)
print null_first(null)
print null_first(some)
print null_first(none)
print bound(3)
print bound(null)
print value_arm(7)
print value_arm(2)
print nested(null)
print nested(3)
print forwarded(null)
print forwarded(3)
"""
  let inferred = test.run_xsh(ctx, source.replace(": SUBJECT", with: ""))?
  let annotated = test.run_xsh(ctx, source.replace("SUBJECT", with: "Int?"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "present\nabsent\npresent\nabsent\n1\n0\n7\n1\n0\n1\nabsent\npresent\n"
  assert inferred.stdout == annotated.stdout
}

test test_inferred_null_matches_accept_each_payload_type { |ctx|
  let good = test.expect(ctx, r"""pure absent(a) -> Bool { match a { null => true, else => false } }
assert absent(null)
assert !absent(1)
assert !absent("text")
assert !absent([1])
assert !absent({k: 1})
""", status: 0)?
  assert good.stderr == "", good.stderr
}

test test_inferred_ordering_inside_a_guard_keeps_its_operand_domain { |ctx|
  let body = r"""type Policy = {threshold: Int?, negative: Bool}
pure outside(SIZE, policy) -> Bool {
  if let threshold = policy.threshold {
    return (! policy.negative and size < threshold) or (policy.negative and size > threshold)
  }
  false
}
let unset: Policy = {threshold: null, negative: false}
let below: Policy = {threshold: 5, negative: false}
let above: Policy = {threshold: 5, negative: true}
print $outside(3, unset)
print $outside(3, below)
print $outside(9, below)
print $outside(9, above)
print $outside(3, above)
"""
  let inferred = test.run_xsh(ctx, body.replace("SIZE", with: "size"))?
  let annotated = test.run_xsh(ctx, body.replace("SIZE", with: "size: Int"))?
  assert inferred.success, inferred.stderr
  assert annotated.success, annotated.stderr
  assert inferred.stdout == "false\ntrue\nfalse\ntrue\nfalse\n"
  assert annotated.stdout == inferred.stdout
  test.expect(ctx, r"""pure outside(size, policy) -> Bool {
  if let threshold = policy.threshold { return size < threshold }
  false
}
assert outside({kind: 1}, {threshold: {kind: 2}})
""", status: 2, stderr: ["[check."])?
}

test test_full_inference_fallback_on_a_null_tested_field_matches_its_annotated_twin { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Socket = {family: Int, mark: Int?}
pure bound(item) -> Bool { item.mark != null and (item.mark ?? -1) == 1 }
pure bound_annotated(item: Socket) -> Bool { item.mark != null and (item.mark ?? -1) == 1 }
let present: Socket = {family: 1, mark: 1}
let other: Socket = {family: 1, mark: 2}
let absent: Socket = {family: 1, mark: null}
for socket in [present, other, absent] {
  assert bound(socket) == bound_annotated(socket)
  print $bound(socket)
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "true\nfalse\nfalse\n"
  test.expect(
    ctx,
    "type Plain = {family: Int, mark: Int}\npure bound(item) -> Bool { item.family != null and (item.mark ?? -1) == 1 }\nlet plain: Plain = {family: 1, mark: 1}\nlet rejected = bound(plain)\n",
    status: 2,
    stderr: ["[check.result-fallback]", "UnsupportedSource"],
  )?
}

test test_full_inference_error_variant_tests_infer_a_plain_error_subject { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""error CopyError = Invalid
proc report(failure) {
  if let CopyError.Invalid {message} = failure { print $message } else { print "other" }
}
proc classify(failure) {
  if failure is CopyError.Invalid { print "invalid" } else { print "other" }
}
proc report_annotated(failure: Error) {
  if let CopyError.Invalid {message} = failure { print $message } else { print "other" }
}
let invalid: Error = CopyError.Invalid("bad copy")
let other: Error = error.failure("plain")
report(invalid)
report(other)
report_annotated(invalid)
report_annotated(other)
classify(invalid)
classify(other)
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "bad copy\nother\nbad copy\nother\ninvalid\nother\n"
  test.expect(
    ctx,
    "error CopyError = Invalid\nproc classify(failure) {\n  if failure is CopyError.Invalid { print \"invalid\" } else { print \"other\" }\n}\nclassify(5)\n",
    status: 2,
    stderr: ["[check.type-mismatch]", "expected Error, found Int"],
  )?
}

test test_full_inference_whole_record_read_under_a_field_guard_matches_its_annotated_twin { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Options = {mnt: Str?, other: Int}
pure persistent(opts) -> Str? {
  let entry = {flag: "--mount", file: opts.mnt}
  if let file = entry.file { let flag = entry.flag; return f"{flag}={file}" }
  null
}
pure persistent_annotated(opts: Options) -> Str? {
  let entry = {flag: "--mount", file: opts.mnt}
  if let file = entry.file { let flag = entry.flag; return f"{flag}={file}" }
  null
}
let some: Options = {mnt: "x", other: 1}
let none: Options = {mnt: null, other: 1}
for options in [some, none] {
  let narrowed = persistent(options) ?? "-"
  print $narrowed
  let twin = persistent_annotated(options) ?? "-"
  print $twin
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "--mount=x\n--mount=x\n-\n-\n"
  for header in ["pure persistent(opts)", "pure persistent(opts: Options)"] {
    test.expect(
      ctx,
      "type Options = {mnt: Str?, other: Int}\ntype Present = {flag: Str, file: Str}\n" + header + " -> Str? {\n  let entry = {flag: \"--mount\", file: opts.mnt}\n  if let file = entry.file { return file }\n  let stale: Present = entry\n  null\n}\nlet some: Options = {mnt: \"x\", other: 1}\nlet checked = persistent(some)\n",
      status: 2,
      stderr: ["[check.type-mismatch]"],
    )?
  }
}

test test_full_inference_null_guarded_local_read_inside_a_loop_matches_its_annotated_twin { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Slot = {id: Int?}
pure total(slot) -> Int? {
  let id = slot.id
  if id == null { return null }
  var sum = 0
  for index in range(3) { sum += id + index }
  sum
}
pure total_annotated(slot: Slot) -> Int? {
  let id = slot.id
  if id == null { return null }
  var sum = 0
  for index in range(3) { sum += id + index }
  sum
}
let present: Slot = {id: 10}
let absent: Slot = {id: null}
for slot in [present, absent] {
  let inferred = total(slot) ?? -1
  let annotated = total_annotated(slot) ?? -1
  assert inferred == annotated
  print $inferred
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "33\n-1\n"
  for header in ["pure total(slot)", "pure total(slot: Slot)"] {
    test.expect(
      ctx,
      "type Slot = {id: Int?}\n" + header + " -> Int? {\n  let id = slot.id\n  var sum = 0\n  for index in range(3) { sum += id + index }\n  sum\n}\nlet present: Slot = {id: 10}\nlet checked = total(present)\n",
      status: 2,
      stderr: ["err["],
    )?
  }
}
