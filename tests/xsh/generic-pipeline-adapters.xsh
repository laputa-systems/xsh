test test_generic_pipeline_adapters_keep_buffer_types_and_items { |ctx|
  let output = test.run_xsh(ctx, p"tests/fixtures/runtime/generic-pipeline-adapters.xsh".read_text()?)?
  assert output.success, output.stderr
  assert output.stdout == "pipeline adapters preserved\n"
}

test test_generic_pipeline_adapters_reject_invalid_sources_and_configuration { |ctx|
  for source in [
    "pure convert(marker, value: Bytes) { let _ = marker; value |> text.lines() }",
    "pure convert(marker, value: Str) { let _ = marker; value |> bytes.chunks(2) }",
    "pure convert(marker, value: Any) { let _ = marker; value |> json.lines() }",
    "pure convert(marker, value: Str) { let _ = marker; value |> json.stream(2) }",
    "pure convert(marker, value: Bytes) { let _ = marker; value |> bytes.chunks(true) }",
    "pure convert(marker, value: Bytes) { let _ = marker; value |> bytes.chunks(0) }",
    "pure convert(marker) { [marker] |> map { \"text\" } |> text.lines() }",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "err[check." in output.stderr
  }
}
