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

proc test_json_decode_type_patterns_and_public_boundaries() [error] {
  let decoded = json.decode("{\"quote\":\"\\\"\",\"line\":\"a\\nb\",\"snow\":\"\\u2603\",\"music\":\"\\uD834\\uDD1E\"}")?
  test.eq(decoded.quote, "\"")?
  test.eq(decoded.line, "a\nb")?
  test.eq(decoded.snow, "\u{2603}")?
  test.eq(decoded.music, "\u{1d11e}")?
  test.eq(json.decode("1.25")?.require(Float)?.format(precision: 2), "1.25")?
  test.error_kind(json.decode("9223372036854775808"), "json")?

  test.eq(json_label(json.decode("1")?)?, "int 1.0")?
  test.eq(json_label(json.decode("1.25")?)?, "float 1.25")?
  test.eq(json_label(json.decode("\"x\"")?)?, "str x")?
  test.eq(json_label(json.decode("null")?)?, "null")?
  test.eq(json_label(json.decode("[1,2]")?)?, "int-list")?
  test.eq(json_label(json.decode("[1,\"x\"]")?)?, "other")?

  let rows = "\n{\"b\":2}\n\n{\"a\":1}\n" |> json.lines()
  let encoded = json.encode({z: 1, a: 2, nested: {b: 1, a: 2}})?
  test.eq(rows[0].b, 2)?
  test.eq(rows[1].a, 1)?
  test.eq(encoded, "{\"a\":2,\"nested\":{\"a\":2,\"b\":1},\"z\":1}")?
  test.error_kind(json.decode("not json"), "json")?

  let data = {path: Path("src")}
  let path_value = data["path"]
  test.error_kind(json.encode(path_value), "json-compatible")?
}

pure json_label(value: Any) -> Result[Str] {
  match value {
    i is Int => return Ok(f"int ${i.float().format(precision: 1)}")
    f is Float => return Ok(f"float ${f.format(precision: 2)}")
    s is Str => return Ok(f"str ${s}")
    _ is Null => return Ok("null")
    _ is List[Int] => return Ok("int-list")
    _ => return Ok("other")
  }
}

proc test_json_path_helpers_report_invalid_paths() [error] {
  let data = {items: [1]}
  test.error_kind(json.get(data, ["items", 4]), "json-path")?
  test.error_kind(json.set(data, ["items", 2], 3), "json-path")?
  test.error_kind(json.remove(data, ["missing"]), "json-path")?
  test.error_kind(json.get(data, [-1]), "json-path")?
}

proc test_json_rejection_is_trace_visible(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(
    ctx,
    """
let data = {path: Path("src")}
let value = data["path"]
let _encoded = json.encode(value) ?
""",
    ["--trace", "--raw"],
  )?

  test.eq(output.status, 3)?
  test.contains(output.stderr, "kind=result.propagate")?
  test.contains(output.stderr, "json-compatible")?
  test.contains(output.stderr, "Path is not JSON-compatible")?
  test.contains(output.stderr, "traceback")?
}
