proc test_json_diff(ctx: TestContext) [fs, process, error] {
  let a = test.temp_file(ctx, name: "a.json", contents: b"{\"name\":\"old\",\"same\":1}")?
  let b = test.temp_file(ctx, name: "b.json", contents: b"{\"name\":\"new\",\"same\":1,\"extra\":true}")?
  let output = run.text "xsh" "showcase/json-diff.xsh" -- $a $b ?
  "added (1):" in output
  "changed (1):" in output
  "same 1" in output
}
