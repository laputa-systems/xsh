test test_json_inference_admission_write_lines_preserves_order_and_existing_file_on_error [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "json-inference-write-lines")?
  let output_path = fp"${root}/values.jsonl"
  let expected = "1\n\"two\"\nnull\ntrue\n"
  json.write_lines(values: [1, "two", null, true], path: output_path)?
  output_path.read_text()? == expected

  let invalid: Any = p"unserializable"
  test.error_kind(json.write_lines(values: [1, invalid], path: output_path), "json-compatible")?
  output_path.read_text()? == expected
}

