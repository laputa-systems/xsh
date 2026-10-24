test test_generic_pipeline_table_retains_rows_and_stream_cleanup { |ctx|
  let output = test.run_xsh(ctx, p"tests/fixtures/runtime/generic-pipeline-table.xsh".read_text()?)?
  assert output.success, output.stderr
  for name in ["alpha", "beta", "gamma", "delta", "stream"] {
    assert name in output.stdout
  }
  assert "closed\n┌" in output.stdout
  assert output.stdout.ends_with("table stages preserved\n")
}

test test_generic_pipeline_table_rejects_nonrecord_and_invalid_column_inputs { |ctx|
  for source in [
    "proc show(row) [] -> Unit { [row] |> table.print() }\nshow(7)",
    "proc show(row) [] -> Unit { [row] |> table.print() }\nlet erased: Any = {name: \"row\"}\nshow(erased)",
    "proc show(row: Any) [] -> Unit { [row] |> table.print() }",
    "proc show(row) [] -> Unit { [row] |> table.print(columns: [1]) }",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "err[check." in output.stderr
  }
}
