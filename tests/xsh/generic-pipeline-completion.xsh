test generic_pipeline_completion { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-completion.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_pipeline_completion()?\n", status: 0)?
}

test nonproducing_pipeline_requires_actual_consumption { |ctx|
  for source in [
    "pure pending(value) -> Int { let ignored = [value] |> map { |item| return 1 } |> take(0); 0 }\n",
    "pure pending(value, items: List[Int]) -> Int { let ignored = items |> map { |item| return 1 } |> sort |> take(0); 0 }\n",
    "pure pending(value) { let ignored = []; value }\n"
  ] {
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "check.local-inference" in output.stderr
    assert "unsupported-indexed" not in output.stderr
  }
}

test completing_pipeline_and_effect_permissions_remain_checked { |ctx|
  let normal = test.run_xsh(ctx, "pure pending(items: List[Int]) -> Int { items |> map { |item| item }; 0 }\n")?
  assert normal.status == 2
  assert "check.ignored-result" in normal.stderr
  let effectful = test.run_xsh(ctx, "proc pending(value) [] -> Result[Int] { [value] |> map { |item| error.fail(\"failed\")?; return 13 }; 0 }\n")?
  assert effectful.status == 2
  assert "check.effect-violation" in effectful.stderr
}

test named_pipeline_inputs_require_actual_nonempty_initializers { |ctx|
  for source in [
    "pure pending(value) -> Int { let rows: List[Int] = []; let ignored = rows |> map { |item| return 1 } |> sort |> take(0); let _ = value; 0 }\n",
    "pure pending(value) -> Int { var rows = [value]; rows = []; let ignored = rows |> map { |item| return 1 } |> sort |> take(0); 0 }\n",
    "pure pending(value, values: List[Int]) -> Int { let rows = values; let ignored = rows |> map { |item| return 1 } |> sort |> take(0); let _ = value; 0 }\n"
  ] {
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "check.stream-sort" in output.stderr
  }
}
