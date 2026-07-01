type JsonPackage = {name: Str, version: Str}

proc test_record_require_and_any_require() [error] {
  let required = record.require({name: "pkg", version: "1", extra: 1}, {name: "Str"}, optional: {version: "Str"})?
  test.eq(required.name, "pkg")?
  test.error_kind(record.require({name: 1}, {name: "Str"}), "record-contract")?
  let typed: JsonPackage = json.decode("{\"name\":\"pkg\",\"version\":\"1\"}")?.require(JsonPackage)?
  test.eq(typed.version, "1")?
  let row = {name: "pkg", version: "1"}
  test.ok(row.has("version"))?
  test.eq(row.get("name")?, "pkg")?
  test.eq(row.keys()[0], "name")?
  test.error_kind(row.get("missing"), "missing-field")?
}
