proc test_parse_log(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(
    ctx,
    name: "app.log",
    contents: b"2026-01-01T00:00:00Z INFO [svc] started\n2026-01-01T00:00:01Z ERROR [svc] crashed at 10.0.0.1\n",
  )?

  let output = run.text "xsh" "showcase/parse-log.xsh" -- $input ?
  test.contains(output, "parsed 2 entries")?
  test.contains(output, "has errors: true")?
  test.contains(output, "<IP>")?
}
