test generic_aggregates { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-aggregates.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_aggregates()?\n", status: 0)?
}

test generic_aggregate_domains { |ctx|
  for stage in ["min", "max"] {
    let source = "pure selected(value) { [value] |> " + stage + "() }\nlet unchecked: Any = 7\nlet _ = selected(unchecked)\n"
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "check.dynamic-boundary" in output.stderr
  }
  let output = test.run_xsh(ctx, "pure total(marker, values: List[Int]) -> Int { let _ = marker; values |> sum() }\nlet _ = total(true, [\"wrong\"])\n")?
  assert output.status == 2
  assert "check.type-mismatch" in output.stderr
}

test generic_aggregate_null_item { |ctx|
  test.expect(ctx, "pure minimum(values) { values |> min() }\nassert minimum([null])? == null\n", status: 0)?
}
