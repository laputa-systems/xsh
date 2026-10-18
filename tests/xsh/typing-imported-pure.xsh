test test_imported_required_pure_calls_match_local_and_module_aliases { |ctx|
  let modules = test.temp_dir(ctx, name: "typing-imported-required-pure")?
  for name in ["left", "right"] {
    fp"{modules}/{name}.xsh".write(f"""##! Required pure operations.
## Identifies the selected operation.
export pure required(field: Str, state: Str, error_kind: Str?, errno: Int?) -> Str {{ "{name}:" + field }}
""")?
  }
  fp"{modules}/renamed.xsh".write("""##! Renamed callable export.
pure original(field: Str, state: Str, error_kind: Str?, errno: Int?) -> Str { "alias:" + field }
## Exposes the original required pure operation.
export let public = original
""")?
  let result = test.run_xsh(ctx, r"""use left as selected
use right as other
use renamed as rebound
pure required(field: Str, state: Str, error_kind: Str?, errno: Int?) -> Str { "local:" + field }
pure local_value(row) { required("meminfo", row.observation.state, row.error_kind, row.errno) }
pure imported_value(row) { selected.required("meminfo", row.observation.state, row.error_kind, row.errno) }
pure other_value(row) { other.required("meminfo", row.observation.state, row.error_kind, row.errno) }
pure aliased_value(row) { rebound.public("meminfo", row.observation.state, row.error_kind, row.errno) }
let row = {observation: {state: "failed"}, error_kind: "io", errno: 5}
assert local_value(row) == "local:meminfo"
assert imported_value(row) == "left:meminfo"
assert other_value(row) == "right:meminfo"
assert aliased_value(row) == "alias:meminfo"
""", env: {XSH_MODULE_PATH: modules})?
  assert result.status == 0, result.stderr
}

test test_imported_source_issue_preserves_required_field_arguments { |ctx|
  let result = test.run_xsh(ctx, r"""use lib.sys_source as src
use lib.system_report as report
pure source_error(source) {
  src.issue("meminfo", source.observation.state, source.error_kind, source.errno)
}
let issue = source_error({observation: {state: report.ReadFailure}, error_kind: "read_failed", errno: 5})
assert issue.field == "meminfo"
assert issue.state == report.ReadFailure
assert issue.error_kind == "read_failed"
assert issue.errno == 5
assert issue.detail.state == report.ReadFailure
""")?
  assert result.status == 0, result.stderr
}

test test_imported_generic_calls_keep_kind_and_parameter_restrictions { |ctx|
  let modules = test.temp_dir(ctx, name: "typing-imported-pure-restrictions")?
  fp"{modules}/decoy.xsh".write("##! Same-name pure operation.\n## Returns its argument.\nexport pure required(value: Int) -> Int { value }\n")?
  for declaration in [
    "export proc required(value: Int) -> Int { value }",
    "export stream required(value: Int) -> Int { yield value }",
    "export pure required(value: Int, other: Int = 0) -> Int { value }",
    "export pure required(value: Int, ...others: List[Int]) -> Int { value }",
  ] {
    fp"{modules}/selected.xsh".write(f"##! Selected operation.\n## Returns the supplied value.\n{declaration}\n")?
    let result = test.run_xsh(ctx, "use decoy as trusted\nuse selected as actual\npure invoke(row) { actual.required(row.value) }\n", env: {XSH_MODULE_PATH: modules})?
    assert result.status == 2, result.stderr
    assert "[check.local-inference]" in result.stderr, result.stderr
  }
}
