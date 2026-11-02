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
