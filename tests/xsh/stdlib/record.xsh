type JsonPackage = {name: Str, version: Str}
type PackageName = {name: Str}

test test_record_schema_validation_and_any_require {
  let required = json.decode("{\"name\":\"pkg\",\"version\":\"1\",\"extra\":1}")?.require(PackageName)?
  assert (required.name) == ("pkg")
  assert ("extra" in required)
  if "version" in required { let _ = required.get("version")?.require(Str)? }
  test.error_kind(({name: 1}).require(PackageName), "schema")?
  let typed: JsonPackage = json.decode("{\"name\":\"pkg\",\"version\":\"1\"}")?.require()?
  assert typed.version == "1"
  let row = {name: "pkg", version: "1"}
  assert ("version" in row)
  assert (row.name) == ("pkg")
  assert row.keys()[0] == "name"
  test.error_kind(row.get("missing"), "missing-field")?
}

test test_standard_record_schemas_reject_bad_dynamic_records { |ctx|
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

  assert (output.status) == (2)
  assert "check.dynamic-boundary" in output.stderr
}

test test_schema_runtime_checks_unknown_values { |ctx|
  let output = test.run_script(
    ctx,
    r"""
type Package = { name: Str, root: Path }
let rows = "{\"name\":\"demo\"}\n" |> json.lines()
let pkg = rows[0].require(Package)?
print ${pkg.name}
""",
  )?

  assert (output.status) == (3)
  assert "schema" in output.stderr
  assert "missing required field root" in output.stderr
}
