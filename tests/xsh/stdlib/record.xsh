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

proc test_standard_record_schemas_reject_bad_dynamic_records(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
proc entry_name(entry: FsEntry) -> Str {
  return entry.name
}

let raw: Record = {
  path: "not a path",
  name: "demo",
  kind: "file",
  ext: "",
  size: 1,
  mode: 0,
  uid: 0,
  gid: 0,
  modified: 0,
  accessed: 0,
}

print ${entry_name(raw)}
""",
  )?

  test.eq(output.status, 3)?
  test.contains(output.stderr, "expected FsEntry, found Record")?
}

proc test_schema_runtime_checks_unknown_values(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
type Package = { name: Str, root: Path }
let rows = "{\"name\":\"demo\"}\n" |> json.lines()
let pkg: Package = rows[0]
print ${pkg.name}
""",
  )?

  test.eq(output.status, 3)?
  test.contains(output.stderr, "expected Package, found Record")?
}
