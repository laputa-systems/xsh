pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_csv_query(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "data.csv", contents: b"name,team\nada,core\nbea,docs\ncal,core\n")?
  let output = run.text xsh_bin() "showcase/csv-query.xsh" -- $input --filter team=core --count ?
  test.contains(output, "columns (2): name, team")?
  test.contains(output, "2 row(s) match team=core")?
  test.contains(output, "total: 2 row(s)")?
}
