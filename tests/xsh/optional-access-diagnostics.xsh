test test_optional_field_diagnostic_names_presence_and_guarded_access { |ctx|
  let output = test.expect(ctx, "type Row = {name: Str}\nlet row: Row? = null\nlet value = row.name\n", status: 2)?
  assert "check.field-access" in output.stderr, output.stderr
  assert "field `name` needs a present value" in output.stderr, output.stderr
  assert "use `?.name` or test for null" in output.stderr, output.stderr
}

test test_optional_index_diagnostic_keeps_its_recovery { |ctx|
  let output = test.expect(ctx, "let values: List[Int]? = null\nlet value = values[missing]\n", status: 2)?
  assert "check.index-type" in output.stderr, output.stderr
  assert "indexing needs a present value, found List[Int]?" in output.stderr, output.stderr
  assert "use `?[...]` or test for null" in output.stderr, output.stderr
  assert "check.unresolved-name" not in output.stderr, output.stderr
}

test test_optional_access_preserves_present_narrowing_and_guarded_values { |ctx|
  let output = test.expect(ctx, r"""type Row = {name: Str}
let row: Row? = Row(name: "ready")
let values: List[Int]? = [7]
if row != null { print $row.name }
if values != null { print (values[0]) }
let absent: Row? = null
let empty: List[Int]? = null
print (absent?.name ?? "missing") (empty?[0] ?? -1)
""", status: 0)?
  assert output.stdout == "ready\n7\nmissing -1\n"
}

test test_optional_command_references_get_the_same_presence_guidance { |ctx|
  let field = test.expect(ctx, "type Row = {name: Path}\nlet row: Row? = null\nfs.fsync row.name\n", status: 2)?
  assert "check.field-access" in field.stderr, field.stderr
  assert "field `name` needs a present value" in field.stderr, field.stderr
  let index = test.expect(ctx, "let values: List[Path]? = null\nfs.fsync values[0]\n", status: 2)?
  assert "check.index-type" in index.stderr, index.stderr
  assert "indexing needs a present value" in index.stderr, index.stderr
}
