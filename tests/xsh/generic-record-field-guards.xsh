test generic_record_field_guards { |ctx|
  let source = p"tests/fixtures/runtime/generic-record-field-guards.xsh".read_text()?
  test.expect(ctx, source + "\nverify_record_field_guards()?\n", status: 0)?
}

test generic_record_guard_continuations { |ctx|
  let source = p"tests/fixtures/runtime/generic-record-field-guards.xsh".read_text()? + "\n" + p"tests/fixtures/runtime/generic-record-guard-continuations.xsh".read_text()?
  test.expect(ctx, source + "\nverify_record_guard_continuations()?\n", status: 0)?
}

test generic_record_field_null_test_narrows_its_branch_read { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure keep(value: Any) -> Int { 1 }
pure label(row) -> Str { if row.node != null { f"node {row.node}" } else { "none" } }
proc forward(row) {
  if row.node == null { print "absent"; return }
  print $keep(row.node)
}
let missing: Str? = null
let present: Str? = "sda"
print (label({node: missing, other: 1}))
print (label({node: present, other: 2}))
forward({node: missing})
forward({node: present})
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "none\nnode sda\nabsent\n1\n"
}

test generic_record_field_null_test_narrows_after_early_return { |ctx|
  let output = test.run_xsh(ctx, r"""proc label(row) {
  if row.node == null { print "none"; return }
  print f"node {row.node}"
}
let missing: Str? = null
let present: Str? = "sda"
label({node: missing})
label({node: present})
""")?
  assert output.success, output.stderr
  assert output.stdout == "none\nnode sda\n"
  let nested = test.run_xsh(ctx, r"""proc label(row, other) {
  if row.node == null { return }
  if other == null { return }
  print f"{row.node}:{other}"
}
let present: Str? = "sda"
let suffix: Str? = "ok"
label({node: present}, suffix)
""")?
  assert nested.success, nested.stderr
  assert nested.stdout == "sda:ok\n"
  let fallthrough = test.run_xsh(ctx, r"""proc label(row) {
  if row.node == null { print "none" }
  print f"node {row.node}"
}
let missing: Str? = null
label({node: missing})
""")?
  assert !fallthrough.success and "check.type-mismatch" in fallthrough.stderr, fallthrough.stderr
  let written = test.run_xsh(ctx, r"""proc label(input) {
  var row = input
  if row.node == null { return }
  row = {node: null}
  print f"node {row.node}"
}
let present: Str? = "sda"
label({node: present})
""")?
  assert !written.success and "check.type-mismatch" in written.stderr, written.stderr
}
