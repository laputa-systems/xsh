test test_csv_query { |ctx|
  let input = test.temp_file(ctx, name: "data.csv", contents: b"name,team\nada,core\nbea,docs\ncal,core\n")?
  let output = run.text "xsh" "showcase/csv-query.xsh" -- $input --filter team=core --count
  assert "columns (2): name, team" in output
  assert "2 row(s) match team=core" in output
  assert "total: 2 row(s)" in output
}
