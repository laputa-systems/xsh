type JsonPackage = {name: Str, version: Str}
type PackageName = {name: Str}

test test_record_schema_validation_and_any_require [error] {
  let required = ({name: "pkg", version: "1", extra: 1}).require(PackageName)?
  test.eq(required.name, "pkg")?
  test.ok("extra" in required)?
  if "version" in required { let _ = required.get("version")?.require(Str)? }
  test.error_kind(({name: 1}).require(PackageName), "schema")?
  let typed: JsonPackage = json.decode("{\"name\":\"pkg\",\"version\":\"1\"}")?.require(JsonPackage)?
  typed.version == "1"
  let row = {name: "pkg", version: "1"}
  ("version" in row)
  test.eq(row.get("name")?, "pkg")?
  row.keys()[0] == "name"
  test.error_kind(row.get("missing"), "missing-field")?
}

test test_standard_record_schemas_reject_bad_dynamic_records [error] { |ctx|
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

  test.eq(output.status, 2)?
  "check.dynamic-boundary" in output.stderr
}

test test_schema_runtime_checks_unknown_values [error] { |ctx|
  let output = test.run_script(
    ctx,
    r"""
type Package = { name: Str, root: Path }
let rows = "{\"name\":\"demo\"}\n" |> json.lines()
let pkg = rows[0].require(Package)?
print ${pkg.name}
""",
  )?

  test.eq(output.status, 3)?
  "schema" in output.stderr
  "missing required field root" in output.stderr
}
