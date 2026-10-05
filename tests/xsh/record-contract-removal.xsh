type Details = {jobs: Int}

type Package = {name: Str, details: Details}

type Name = {name: Str}

type NullableVersion = {name: Str, version: Str?}

test test_record_require_removed_api_has_actionable_diagnostic { |ctx|
  let _ = test.expect(
    ctx,
    r"""
let checked = record.require({name: "demo"}, {name: "Str"})?
print $checked.name
""",
    status: 2,
    stderr: ["check.removed-record-require", ".require(Schema)"],
  )?
}

test test_record_named_schema_keeps_nested_fields_extras_and_aliases {
  let raw: Record = {name: "demo", details: {jobs: 4}, extra: "retained"}
  let checked = raw.require(Package)?
  assert checked.name == "demo", "name survives validation"
  assert checked.details.jobs == 4, "nested schema preserves its field"
  assert "extra" in checked, "extra fields survive validation"
  assert "extra" in raw, "original alias remains unchanged"
  test.error_kind({name: "demo", details: {jobs: "four"}}.require(Package), "schema")
  test.error_kind({name: "demo"}.require(Package), "schema")
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
    test.error_kind(checked.get("version")?.require(Str), "schema")
  }

  test.error_kind({name: "demo"}.require(NullableVersion), "schema")
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
  fp"{root}/helper.xsh".write_atomic("""##! User module with an ordinary callable.
## Returns its argument unchanged.
export pure require(value: Str) -> Str { value }
""")
  let output = test.expect(
    ctx,
    r"""
use helper as record
let selected = record.require("hello")
print $selected
""",
    status: 0,
    args: [],
    env: {XSH_MODULE_PATH: root},
  )?
  assert output.stdout == """hello
"""
}

# The removed call sits in a module that the linted root imports, and the root
# is long enough to have text at the call's offsets. The fix copies the
# receiver's text from the module's own file, whether the module is linted
# itself or reached through its importer, and never from the importer's text.
test test_record_require_migration_fix_reads_the_text_of_its_own_file { |ctx|
  let root = test.temp_dir(ctx, name: "record-require-module")?
  let module_file = fp"{root}/names.xsh"
  let module_source = r"""##! Package names, validated the way callers used to with a string contract.

## A package name.
export type PackageName = {name: Str}

## The name of the demo package.
export proc demo() [error] -> Result[Str, Error] {
  let checked = record.require(PackageName(name: "demo"), {name: "Str"})?
  checked.name
}
"""
  module_file.write_atomic(module_source)
  let main = fp"{root}/main.xsh"
  main.write_atomic(r"""use names

pure first_label() -> Str { "the first of several labels that only make this file long" }
pure second_label() -> Str { "the second of several labels that only make this file long" }
pure third_label() -> Str { "the third of several labels that only make this file long" }
pure fourth_label() -> Str { "the fourth of several labels that only make this file long" }

print names.demo()?
print first_label().byte_len() second_label().byte_len() third_label().byte_len() fourth_label().byte_len()
""")

  let help = "help: validate the existing named schema -> PackageName(name: \"demo\").require(PackageName)"
  let reported = run.capture --text "xsht" lint --only lint.removed-record-require $main ?
  assert "check.removed-record-require" in reported.stderr, reported.stderr
  assert help in reported.stderr, reported.stderr
  let offered = run.capture --text "xsht" lint --only lint.removed-record-require $module_file ?
  assert help in offered.stderr, offered.stderr

  let under_importer = run.capture --text "xsht" lint --fix --only lint.removed-record-require $main ?
  assert under_importer.status.exited_with(0), under_importer.stderr
  let fixed = module_file.read_text()?
  assert fixed == module_source.replace(
    "record.require(PackageName(name: \"demo\"), {name: \"Str\"})",
    "PackageName(name: \"demo\").require(PackageName)",
  ), fixed
  let after = test.expect(ctx, main.read_text()?, status: 0, args: [], env: {XSH_MODULE_PATH: root})?
  assert after.stdout == "demo\n57 58 57 58\n"
}

# A call with a comment inside has no edit in its own file, and none through
# an importer either: the importer's run reports it and writes nothing.
test test_record_require_migration_in_a_module_keeps_a_commented_call { |ctx|
  let root = test.temp_dir(ctx, name: "record-require-module-comment")?
  let module_file = fp"{root}/names.xsh"
  let module_source = r"""##! Package names.

## A package name.
export type PackageName = {name: Str}

## The name of the demo package.
export proc demo() [error] -> Result[Str, Error] {
  let checked = record.require(
    PackageName(name: "demo"), # the demo package
    {name: "Str"},
  )?
  checked.name
}
"""
  module_file.write_atomic(module_source)
  let main = fp"{root}/main.xsh"
  main.write_atomic("use names\n\nprint names.demo()?\n")

  let reported = run.capture --text "xsht" lint --only lint.removed-record-require $main ?
  assert "check.removed-record-require" in reported.stderr, reported.stderr
  assert "help: validate the existing named schema" not in reported.stderr, reported.stderr
  let under_importer = run.capture --text "xsht" lint --fix --only lint.removed-record-require $main ?
  assert ! under_importer.status.exited_with(0), under_importer.stderr
  assert module_file.read_text()? == module_source
}
