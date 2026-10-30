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
