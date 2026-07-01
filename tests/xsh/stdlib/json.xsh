proc test_json_read_write_lines_and_paths(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "json")?
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true}}")?
  test.eq(json.get(value, ["name"])?, "pkg")?
  test.eq(json.get(value, ["missing"], "fallback"), "fallback")?
  let updated = json.set(value, ["meta", "status"], "ready")?
  test.eq(json.get(updated, ["meta", "status"])?, "ready")?
  let removed = json.remove(updated, ["items", 0])?
  test.eq(json.get(removed, ["items", 0])?, 2)?
  test.contains(json.encode(updated, pretty: true)?, "\"status\"")?
  test.contains(json.encode_lines([{a: 1}, {a: 2}])?, "{\"a\":1}")?
  let json_path = fp"${root}/data.json"
  json.write(json_path, updated, pretty: false)?
  test.eq(json.read(json_path)?["name"], "pkg")?
  let lines_path = fp"${root}/lines.jsonl"
  json.write_lines(lines_path, [{a: 1}, {a: 2}])?
  test.eq(lines_path.read_text()?.count_lines(), 2)?
  test.error_kind(json.decode("{"), "json")?
  test.error_kind(json.get(value, ["items", "bad"]), "json-path")?
}
