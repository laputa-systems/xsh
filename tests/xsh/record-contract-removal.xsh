type Details = {jobs: Int}

type Package = {name: Str, details: Details}

type Name = {name: Str}

type NullableVersion = {name: Str, version: Str?}

test test_record_require_removed_api_has_actionable_diagnostic { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let checked = record.require({name: "demo"}, {name: "Str"})?
print $checked.name
""",
  )?
  assert output.status == 2
  assert "check.removed-record-require" in output.stderr
  assert ".require(Schema)" in output.stderr
}

test test_record_named_schema_keeps_nested_fields_extras_and_aliases {
  let raw: Record = {name: "demo", details: {jobs: 4}, extra: "retained"}
  let checked = raw.require(Package)?
  assert checked.name == "demo", "name survives validation"
  assert checked.details.jobs == 4, "nested schema preserves its field"
  assert "extra" in checked, "extra fields survive validation"
  assert "extra" in raw, "original alias remains unchanged"
  test.error_kind({name: "demo", details: {jobs: "four"}}.require(Package), "schema")?
  test.error_kind({name: "demo"}.require(Package), "schema")?
}

test test_record_optional_key_validation_distinguishes_absent_null_and_wrong_type {
  for input in ["{\"name\":\"demo\"}", "{\"name\":\"demo\",\"version\":\"1\"}"] {
    let checked = json.decode(input)?.require(Name)?
    if "version" in checked {
      let version = checked.get("version")?.require(Str)?
      assert version == "1", "present optional field is validated"
    }
  }

  for input in ["{\"name\":\"demo\",\"version\":null}", "{\"name\":\"demo\",\"version\":1}"] {
    let checked = json.decode(input)?.require(Name)?
    assert "version" in checked, "null and wrong values remain present"
    test.error_kind(checked.get("version")?.require(Str), "schema")?
  }

  test.error_kind({name: "demo"}.require(NullableVersion), "schema")?
  let nullable: Record = {name: "demo", version: null}
  let present = nullable.require(NullableVersion)?
  assert "version" in present, "nullable field is present"
  assert present.version == null, "present null is retained"
}

test test_record_contract_removal_keeps_cli_descriptor_strings {
  let options = cli.parse(["--jobs", "4"], {jobs: {kind: "Int", form: "--jobs N"}})?
  assert options.jobs == 4, "CLI descriptor string remains supported"
}

test test_record_removed_module_name_does_not_capture_user_module_callable { |ctx|
  let root = test.temp_dir(ctx, name: "record-user-module")?
  fp"${root}/helper.xsh".write_atomic("""##! User module with an ordinary callable.
## Returns its argument unchanged.
export pure require(value: Str) -> Str { value }
""")?
  let output = test.run_script(
    ctx,
    r"""
use helper as record
let selected = record.require("hello")
print $selected
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert output.status == 0
  assert output.stdout == """hello
"""
}
