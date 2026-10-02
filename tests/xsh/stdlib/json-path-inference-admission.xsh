test test_json_inference_admission_keeps_path_segments_and_runtime_validation [error] {
  let input = {rows: [{name: "first"}]}
  json.encode(json.set(input, ["rows", 0, "name"], "second")?)? == "{\"rows\":[{\"name\":\"second\"}]}"
  test.error_kind(json.set(input, ["rows", 1.5], "second"), "json-path")?
}

