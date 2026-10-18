test rank1_nested_nominal_schemas_preserve_generic_parameters_and_returns { |ctx|
  let output = test.run_xsh(ctx, r"""
type Item = {value: Int}
pure keep(value, items: List[Item]) { value }
pure make(value, items: List[Item]) -> List[Item] { items }
let answer: Bool = keep(false, [Item(1)])
print $answer
print make(false, [Item(1)])[0].value
""")?
  assert output.success, output.stderr
  assert output.stdout == "false\n1\n"
}

test rank1_exported_identity_instantiates_each_import_root { |ctx|
  let modules = test.temp_dir(ctx, name: "rank1-identity")?
  fp"{modules}/identity.xsh".write(r"""##! Generic identity module.
## Returns its input unchanged.
export pure identity(value) {
  value
}
""")

  let integer = test.run_xsh(
    ctx,
    "use identity as shared\nlet value: Int = shared.identity(42)\nprint $value\n",
    env: {XSH_MODULE_PATH: modules},
  )?
  let text = test.run_xsh(
    ctx,
    "use identity as shared\nlet value: Str = shared.identity(\"demo\")\nprint $value\n",
    env: {XSH_MODULE_PATH: modules},
  )?
  let boolean = test.run_xsh(
    ctx,
    "use identity as shared\nlet value: Bool = shared.identity(false)\nassert !value\nprint $value\n",
    env: {XSH_MODULE_PATH: modules},
  )?

  assert integer.success, integer.stderr
  assert integer.stdout == "42\n"
  assert text.success, text.stderr
  assert text.stdout == "demo\n"
  assert boolean.success, boolean.stderr
  assert boolean.stdout == "false\n"
}

test rank1_exported_composition_calls_identity_and_projection { |ctx|
  let modules = test.temp_dir(ctx, name: "rank1-composition")?
  fp"{modules}/composition.xsh".write(r"""##! Exports a generic helper composed from identity and projection.
## Returns the required name field through both generic helpers.
export pure identity(value) {
  value
}
## Projects the required name field.
export pure name_of(value) {
  value.name
}
## Preserves the projected type through generic composition.
export pure project_identity(value) {
  identity(name_of(value))
}
""")?
  fp"{modules}/composition-int.xsh".write(r"""##! Integer specialization of the composed helper.
use composition as helpers
## Exposes the integer specialization of the composed helper.
export let value: Int = helpers.project_identity({name: 42, enabled: true})
""")?
  fp"{modules}/composition-text.xsh".write(r"""##! Text specialization of the composed helper.
use composition as helpers
## Exposes the text specialization of the composed helper.
export let value: Str = helpers.project_identity({name: "demo", version: "1.0"})
""")?

  let integer_first = test.run_xsh(
    ctx,
    "use composition-int as integer\nuse composition-text as text\nassert integer.value == 42\nassert text.value == \"demo\"\nprint $integer.value\nprint $text.value\n",
    env: {XSH_MODULE_PATH: modules},
  )?
  let text_first = test.run_xsh(
    ctx,
    "use composition-text as text\nuse composition-int as integer\nassert integer.value == 42\nassert text.value == \"demo\"\nprint $integer.value\nprint $text.value\n",
    env: {XSH_MODULE_PATH: modules},
  )?

  assert integer_first.success, integer_first.stderr
  assert integer_first.stdout == "42\ndemo\n"
  assert text_first.success, text_first.stderr
  assert text_first.stdout == "42\ndemo\n"

  let missing_projection = test.run_xsh(
    ctx,
    "use composition as helpers\nlet invalid = helpers.project_identity({enabled: true})\n",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert !missing_projection.success, missing_projection.stderr
  assert "err[check.unknown-field]" in missing_projection.stderr, missing_projection.stderr
}

test rank1_immutable_alias_instantiates_independent_direct_calls { |ctx|
  let result = test.run_xsh(
    ctx,
    r"""pure identity(value) {
  value
}
let shared = identity
let integer: Int = shared(42)
let text: Str = shared("demo")
let boolean: Bool = shared(false)
assert !boolean
print $integer
print $text
print $boolean
""",
  )?
  assert result.success, result.stderr
  assert result.stdout == "42\ndemo\nfalse\n"
}

test rank1_named_required_calls_preserve_source_argument_order { |ctx|
  let result = test.run_xsh(
    ctx,
    r"""pure choose(first, second) {
  first
}
proc mark_bool(label: Str, value: Bool) [io] -> Result[Bool] {
  print $label
  return Ok(value)
}
proc mark_text(label: Str, value: Str) [io] -> Result[Str] {
  print $label
  return Ok(value)
}
let selected: Bool = choose(second: mark_text("second", "ignored")?, first: mark_bool("first", false)?)
assert !selected
print $selected
""",
  )?
  assert result.success, result.stderr
  assert result.stdout == "second\nfirst\nfalse\n"
}

test rank1_generic_rest_calls_preserve_prefix_and_splice_shapes { |ctx|
  let result = test.run_xsh(
    ctx,
    r"""pure head(prefix, ...items: List[Int]) {
  prefix
}
let integer: Int = head(7)
let multiple: Int = head(8, 1, 2)
let spliced: Int = head(9, @[1, 2])
let named: Int = head(...{prefix: 10}, @[])
let text: Str = head("demo")
let boolean: Bool = head(false)
let shared = head
let alias_integer: Int = shared(11)
let alias_multiple: Int = shared(12, 2, 3)
let alias_spliced: Int = shared(13, @[4, 5])
let alias_text: Str = shared("alias")
let alias_boolean: Bool = shared(false)
assert integer == 7
assert multiple == 8
assert spliced == 9
assert named == 10
assert text == "demo"
assert !boolean
assert alias_integer == 11
assert alias_multiple == 12
assert alias_spliced == 13
assert alias_text == "alias"
assert !alias_boolean
print $integer
print $multiple
print $spliced
print $named
print $text
print $boolean
""",
  )?
  assert result.success, result.stderr
  assert result.stdout == "7\n8\n9\n10\ndemo\nfalse\n"
}

test rank1_generic_function_cannot_be_stored_as_typed_callable { |ctx|
  let result = test.run_xsh(
    ctx,
    r"""type StringIdentity = pure(value: Str) -> Str
pure identity(value) {
  value
}
let stored: StringIdentity = identity
""",
  )?
  assert !result.success, result.stderr

  let aliased = test.run_xsh(
    ctx,
    r"""type StringIdentity = pure(value: Str) -> Str
pure identity(value) {
  value
}
let shared = identity
let stored: StringIdentity = shared
""",
  )?
  assert !aliased.success, aliased.stderr
  assert "[check.callable-mismatch]" in aliased.stderr, aliased.stderr
}

test rank1_generic_rest_items_reject_wrong_element_types { |ctx|
  for source in [
    r"""pure head(prefix, ...items: List[Int]) {
  prefix
}
let result: Int = head(7, "wrong")
""",
    r"""pure head(prefix, ...items: List[Int]) {
  prefix
}
let words: List[Str] = ["wrong"]
let result: Int = head(7, @words)
""",
  ] {
    test.expect(
      ctx,
      source,
      status: 2,
      stderr: ["[check.type-mismatch]"],
    )?
  }
}

test rank1_generic_rest_splice_before_fixed_slot_fails_closed { |ctx|
  test.expect(
    ctx,
    r"""pure head(prefix, ...items: List[Int]) {
  prefix
}
pure selected() -> Int {
  head(@[7, 8], 7)
}
let result: Int = selected()
""",
    status: 2,
    stderr: [
      "[compact.indexed-build]",
      "generic positional splice must target the final rest parameter",
    ],
  )?
}

test rank1_generic_default_parameters_fail_closed_in_lowering { |ctx|
  test.expect(
    ctx,
    r"""pure first(value, ignored = 0) {
  value
}
let result: Str = first("ok")
""",
    status: 2,
    stderr: [
      "[compact.indexed-build]",
      "generic function parameters with defaults are not lowerable",
    ],
  )?
}

test rank1_private_and_exported_projections_accept_different_rows { |ctx|
  let modules = test.temp_dir(ctx, name: "rank1-projections")?
  fp"{modules}/projection.xsh".write(r"""##! Open record projection.
## Return a required name field while retaining extra fields.
export pure name_of(value) {
  value.name
}
## Return the first of two generic arguments.
export pure choose(first, second) {
  first
}
""")

  let imported = test.run_xsh(
    ctx,
    r"""use projection
let package: Str = projection.name_of({name: "package", version: "1.0"})
let count: Int = projection.name_of({name: 42, enabled: true})
let chosen: Bool = projection.choose(second: "ignored", first: false)
assert !chosen
print $package
print $count
print $chosen
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  let private = test.run_xsh(
    ctx,
    r"""pure name_of(value) {
  value.name
}
pure identity(value) {
  value
}
let integer: Int = identity(7)
let text: Str = identity("private")
let boolean: Bool = identity(false)
let package: Str = name_of({name: "private", version: "2.0"})
let enabled: Bool = name_of({name: false, count: 3})
assert !boolean
assert !enabled
print $integer
print $text
print $boolean
print $package
print $enabled
""",
  )?

  assert imported.success, imported.stderr
  assert imported.stdout == "package\n42\nfalse\n"
  assert private.success, private.stderr
  assert private.stdout == "7\nprivate\nfalse\nprivate\nfalse\n"
}

test rank1_explicit_any_and_monomorphic_prefix_body_keep_their_contracts { |ctx|
  let accepted = test.run_xsh(
    ctx,
    r"""pure explicit(value: Any) -> Any {
  value
}
pure monomorphic(value: Int) {
  let copy = value
  copy
}
pure explicit_any_parameter(value, ignored: Any) { value }
let dynamic: Any = explicit(42)
let result: Int = monomorphic(41)
let retained: Bool = explicit_any_parameter(false, 42)
assert result == 41
assert !retained
""",
  )?
  assert accepted.success, accepted.stderr

  test.expect(
    ctx,
    r"""pure explicit(value: Any) -> Any {
  value
}
let raw: Any = 42
let concrete: Int = explicit(raw)
""",
    status: 2,
    stderr: ["[check.dynamic-boundary]"],
  )?
}

test rank1_quantified_bodies_accept_immutable_prefixes_and_explicit_returns { |ctx|
  test.expect(ctx, r"""pure prefixed(value) {
  let copy = value
  value
}
pure returned(value) {
  return value
}
assert prefixed(7) == 7
assert prefixed(false) == false
assert returned(7) == 7
assert returned(false) == false
""", status: 0)?
}

test rank1_quantified_bodies_reject_optional_fields_and_collection_literals { |ctx|
  for source in [
    r"""pure optional_name(value) {
  value?.name
}
""",
    r"""pure wrapped(value) {
  [value]
}
""",
  ] {
    test.expect(
      ctx,
      source,
      status: 2,
      stderr: ["[check.local-inference]"],
    )?
  }
}

test rank1_nongeneric_unresolved_function_material_still_needs_annotation { |ctx|
  test.expect(
    ctx,
    r"""pure build() {
  let unresolved = []
  let copy = unresolved
  1
}
""",
    status: 2,
    stderr: ["[check.local-inference]"],
  )?
}

test rank1_concrete_module_error_branches_retain_callable_signatures { |ctx|
  let modules = test.temp_dir(ctx, name: "rank1-concrete-error-exports")?
  fp"{modules}/requests.xsh".write(r"""##! Concrete functions with an absent Result arm.
## A rejected request.
export error RequestError = Rejected
## Builds a rejected request without a success value.
export pure rejected() -> Result[Int, Error] {
  Err(RequestError.Rejected())
}
## Accepts seven and rejects every other integer.
export proc execute(value: Int) [error] -> Result[Unit, Error] {
  return when value == 7
  Err(RequestError.Rejected())
}
""")?
  let accepted = test.run_xsh(
    ctx,
    "use requests as actions\nactions.execute(7)?\nassert actions.rejected() is Err(_)\nprint \"accepted\"\n",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == "accepted\n"

  let rejected = test.run_xsh(
    ctx,
    "use requests as actions\nactions.execute(8)?\n",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert rejected.status == 3, rejected.stderr
  assert "Rejected" in rejected.stderr, rejected.stderr

  let wrong_argument = test.run_xsh(
    ctx,
    "use requests as actions\nactions.execute(\"seven\")?\n",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert wrong_argument.status == 2, wrong_argument.stderr
  assert "[check.type-mismatch]" in wrong_argument.stderr, wrong_argument.stderr
}
