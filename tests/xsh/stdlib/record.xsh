type JsonPackage = {name: Str, version: Str}
type PackageName = {name: Str}

test explicit_any_record_fields_preserve_the_dynamic_boundary [error] { |ctx|
  let accepted = test.run_script(ctx, r"""pure answer(value: Any) -> Any { value.answer }
print ${answer({answer: 7})} ${answer({answer: "word"})}
""")?
  assert accepted.status == 0, accepted.stderr
  assert accepted.stdout == "7 word\n", accepted.stdout
  let rejected = test.run_script(ctx, r"""pure answer(value: Any) -> Int { value.answer }
let _ = answer({answer: 7})
""")?
  assert rejected.status == 2, rejected.stderr
  assert "check.dynamic-boundary" in rejected.stderr, rejected.stderr
  true
}

test test_record_schema_validation_and_any_require [error] {
  let required = json.decode("{\"name\":\"pkg\",\"version\":\"1\",\"extra\":1}")?.require(PackageName)?
  (required.name) == ("pkg")
  ("extra" in required)
  if "version" in required { let _ = required.get("version")?.require(Str)? }
  test.error_kind(({name: 1}).require(PackageName), "schema")?
  let typed: JsonPackage = json.decode("{\"name\":\"pkg\",\"version\":\"1\"}")?.require()?
  typed.version == "1"
  let row = {name: "pkg", version: "1"}
  ("version" in row)
  (row.name) == ("pkg")
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

  (output.status) == (2)
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

  (output.status) == (3)
  "schema" in output.stderr
  "missing required field root" in output.stderr
}
