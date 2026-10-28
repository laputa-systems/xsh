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
  ] {
    fp"{modules}/selected.xsh".write(f"##! Selected operation.\n## Returns the supplied value.\n{declaration}\n")?
    let result = test.run_xsh(ctx, "use decoy as trusted\nuse selected as actual\npure invoke(row) { actual.required(row.value) }\n", env: {XSH_MODULE_PATH: modules})?
    assert result.status == 2, result.stderr
    assert "[check.local-inference]" in result.stderr, result.stderr
  }
}

test test_imported_defaulted_and_rest_calls_infer_the_caller_parameter { |ctx|
  let modules = test.temp_dir(ctx, name: "typing-imported-pure-shapes")?
  for declaration in [
    "export pure required(value: Int, other: Int = 0) -> Int { value + other }",
    "export pure required(value: Int, ...others: List[Int]) -> Int { value + others.len() }",
  ] {
    fp"{modules}/selected.xsh".write(f"##! Selected operation.\n## Returns the supplied value.\n{declaration}\n")?
    let result = test.run_xsh(
      ctx,
      r"""use selected as actual
pure invoke(row) { actual.required(row.value) }
print ${invoke({value: 5})}
""",
      env: {XSH_MODULE_PATH: modules},
    )?
    assert result.success, result.stderr
    assert result.stdout == "5\n", result.stdout
  }
}

test test_exported_value_from_a_generic_call_publishes_its_resolved_type { |ctx|
  let modules = test.temp_dir(ctx, name: "typing-exported-generic-value")?
  fp"{modules}/shared.xsh".write("##! Shared identity.\n## Returns its argument.\nexport pure identity(value) { value }\n")?
  fp"{modules}/branch.xsh".write("""##! Branch holding a generic call result.
use shared
## The identity of one.
export let value = shared.identity(1)
## The identity of text.
export let label = shared.identity("x")
""")?
  let result = test.run_xsh(ctx, r"""use branch
let total: Int = branch.value + 1
let text: Str = branch.label + "y"
assert total == 2
assert text == "xy"
""", env: {XSH_MODULE_PATH: modules})?
  assert result.status == 0, result.stderr
}

test test_exported_value_with_an_unresolved_type_is_rejected_in_its_module { |ctx|
  let modules = test.temp_dir(ctx, name: "typing-exported-unresolved-value")?
  fp"{modules}/branch.xsh".write("""##! Branch holding an unresolved call result.
## Returns its argument.
export pure keep(value) { value }
## The unresolved result.
export let value = keep([])
""")?
  let result = test.run_xsh(ctx, "use branch\nprint $branch.value.len()\n", env: {XSH_MODULE_PATH: modules})?
  assert result.status == 2, result.stderr
  assert "[check.local-inference]" in result.stderr, result.stderr
  assert "branch.xsh:5" in result.stderr, result.stderr
  assert "[check.field-access]" not in result.stderr, result.stderr
}

test test_imported_callables_pass_directly_as_arguments_to_generic_callees { |ctx|
  let modules = test.temp_dir(ctx, name: "typing-imported-callable-arguments")?
  fp"{modules}/helpers.xsh".write("""##! Generic and ground helpers.
## Returns its argument.
export pure identity(value) { value }
## Applies a callback twice.
export pure twice(callback, value) { callback(callback(value)) }
## Adds one.
export pure increment(value: Int) -> Int { value + 1 }
""")?
  let output = test.run_xsh(ctx, r"""use helpers
pure apply(callback, value) { callback(value) }
assert helpers.twice(helpers.identity, 3) == 3
assert helpers.twice(helpers.identity, "text") == "text"
assert helpers.twice(helpers.increment, 3) == 5
assert apply(helpers.identity, 7) == 7
assert apply(helpers.identity, "kept") == "kept"
assert apply(helpers.increment, 7) == 8
print "direct"
""", env: {XSH_MODULE_PATH: modules})?
  assert output.status == 0, output.stderr
  assert output.stdout == "direct\n"
  let rejected = test.run_xsh(ctx, "use helpers\npure apply(callback, value) { callback(value) }\nlet bad: Str = apply(helpers.increment, 1)\n", env: {XSH_MODULE_PATH: modules})?
  assert rejected.status == 2, rejected.stderr
  assert "[check.type-mismatch]" in rejected.stderr, rejected.stderr
}

test test_imported_callable_arguments_keep_their_module_environment { |ctx|
  let modules = test.temp_dir(ctx, name: "typing-imported-callable-environment")?
  fp"{modules}/state.xsh".write("""##! Exports that read module state.
let offset = 10
let label = "tag"
## Adds the module offset.
export pure shifted(value: Int) -> Int { value + offset }
## Pairs a value with the module offset.
export pure with_offset(value) { {value, offset} }
## Pairs a value with the module label.
export pure tagged(value) { {value, label} }
## Applies a callback twice.
export pure twice(callback, value) { callback(callback(value)) }
## Applies a callback.
export pure apply(callback, value) { callback(value) }
""")?
  let output = test.run_xsh(ctx, r"""use state
pure relay(callback, value) { callback(value) }
let shifted_twice = state.twice(state.shifted, 1)
let first = state.apply(state.with_offset, 1)
let text = relay(state.tagged, "x")
let number = relay(state.tagged, 2)
assert shifted_twice == 21
assert first.offset == 10 and first.value == 1
assert text.label == "tag" and text.value == "x"
assert number.label == "tag" and number.value == 2
assert relay(state.shifted, 5) == 15
print "environment"
""", env: {XSH_MODULE_PATH: modules})?
  assert output.status == 0, output.stderr
  assert output.stdout == "environment\n"
}
