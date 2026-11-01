test test_inferred_record_update_passes_to_erased_record_consumers { |ctx|
  let output = test.run_xsh(ctx, r"""type Attrs = {ispeed: Int, ospeed: Int, label: Str}
pure accept(value: Record) -> Bool { true }
pure apply(wanted) -> Int {
  let input = if wanted.ispeed == wanted.ospeed { 0 } else { wanted.ispeed }
  if accept({...wanted, ispeed: input}) { input } else { -1 }
}
assert apply({ispeed: 7, ospeed: 7, label: "same"}) == 0
assert apply({ispeed: 7, ospeed: 8, label: "different"}) == 7
""")?
  assert output.success, output.stderr
  let rejected = test.run_xsh(ctx, r"""pure accept(value: Record) -> Bool { true }
pure apply(wanted) -> Bool { accept({...wanted, ispeed: 0}) }
print apply(7)
""")?
  assert ! rejected.success, rejected.stdout
  assert "[check.type-mismatch]" in rejected.stderr, rejected.stderr
}

test test_inferred_tty_update_passes_to_native_record_parameter { |ctx|
  test.unix_fake(ctx, {})
  let output = test.run_xsh(ctx, r"""proc apply(wanted) -> Result[Int] {
  let input = if wanted.ispeed == wanted.ospeed { 0 } else { wanted.ispeed }
  unix.set_tty_attrs({...wanted, ispeed: input}, -1, "now")?
  input
}
let attrs = unix.tty_attrs()?
assert apply({...attrs, ispeed: 7, ospeed: 7})? == 0
assert apply({...attrs, ispeed: 7, ospeed: 8})? == 7
""")?
  assert output.success, output.stderr
}

test test_inferred_record_rows_cannot_change_a_list_element_contract { |ctx|
  let rejected = test.run_xsh(ctx, r"""pure accept(values: List[Record]) -> Bool { values.len() == 1 }
pure apply(wanted) -> Bool {
  let row = {...wanted, ispeed: 0}
  let rows = [row]
  accept(rows)
}
assert apply({ispeed: 7, label: "kept"})
""")?
  assert ! rejected.success, rejected.stdout
  assert "[check.type-mismatch]" in rejected.stderr, rejected.stderr
}
