test test_json_diff { |ctx|
  let a = test.temp_file(ctx, name: "a.json", contents: b"{\"name\":\"old\",\"same\":1}")?
  let b = test.temp_file(ctx, name: "b.json", contents: b"{\"name\":\"new\",\"same\":1,\"extra\":true}")?
  let output = run.text "xsh" "showcase/json-diff.xsh" $a $b ?
  assert "added (1):" in output
  assert "changed (1):" in output
  assert "same 1" in output
}

test test_json_diff_preserves_nested_and_null_values { |ctx|
  let a = test.temp_file(
    ctx,
    name: "before.json",
    contents: b"{\"nested\":{\"items\":[null,true,1.5,\"old\"]},\"same\":null,\"removed\":null}",
  )?
  let b = test.temp_file(
    ctx,
    name: "after.json",
    contents: b"{\"nested\":{\"items\":[null,false,1.5,\"new\"]},\"same\":null,\"added\":[null,{\"value\":2}]}",
  )?
  let output = run.text "xsh" "showcase/json-diff.xsh" $a $b ?
  assert "  - removed: null" in output
  assert "  + added: [null,{\"value\":2}]" in output
  assert "    < {\"items\":[null,true,1.5,\"old\"]}" in output
  assert "    > {\"items\":[null,false,1.5,\"new\"]}" in output
  assert "same 1  removed 1  added 1  changed 1" in output
}

test test_json_diff_rejects_non_object_roots { |ctx|
  let object = test.temp_file(ctx, name: "object.json", contents: b"{}")?
  for root in ["null", "[]", "true", "1", "\"text\""] {
    let invalid = test.temp_file(ctx, name: "invalid.json", contents: bytes.from_text(root))?
    let left = run.capture --text "xsh" "showcase/json-diff.xsh" $invalid $object ?
    let right = run.capture --text "xsh" "showcase/json-diff.xsh" $object $invalid ?
    for captured in [left, right] {
      let rejected = ! captured.status.exited_with(0)
      let rejection_message = f"accepted root {root}"
      assert rejected, rejection_message
      assert captured.stdout == ""
      assert "expected Record" in captured.stderr
    }
  }
}
