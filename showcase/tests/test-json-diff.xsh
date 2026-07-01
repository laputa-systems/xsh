pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_json_diff(ctx: TestContext) [fs, process, error] {
  let a = test.temp_file(ctx, name: "a.json", contents: b"{\"name\":\"old\",\"same\":1}")?
  let b = test.temp_file(ctx, name: "b.json", contents: b"{\"name\":\"new\",\"same\":1,\"extra\":true}")?
  let output = run.text xsh_bin() "showcase/json-diff.xsh" -- $a $b ?
  test.contains(output, "added (1):")?
  test.contains(output, "changed (1):")?
  test.contains(output, "same 1")?
}
