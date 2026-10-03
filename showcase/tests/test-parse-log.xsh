test test_parse_log { |ctx|
  let input = test.temp_file(
    ctx,
    name: "app.log",
    contents: b"2026-01-01T00:00:00Z INFO [svc] started\n2026-01-01T00:00:01Z ERROR [svc] crashed at 10.0.0.1\n",
  )?

  let output = run.text "xsh" "showcase/parse-log.xsh" -- $input ?
  assert "parsed 2 entries" in output
  assert "has errors: true" in output
  assert "<IP>" in output
}
