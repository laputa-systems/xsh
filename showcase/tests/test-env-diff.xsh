proc test_env_diff(ctx: TestContext) [fs, process, error] {
  let file_a = test.temp_file(ctx, name: "a.env", contents: b"HOST=localhost\nPORT=5432\nDB=mydb\n")?
  let file_b = test.temp_file(ctx, name: "b.env", contents: b"HOST=localhost\nPORT=5433\nSECRET=xyz\n")?
  let output = run.text "xsh" "showcase/env-diff.xsh" -- --a $file_a --b $file_b ?
  "- DB=" in output
  "+ SECRET=" in output
  "~ PORT" in output
}
