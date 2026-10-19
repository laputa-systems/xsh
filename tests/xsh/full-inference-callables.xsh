proc full_inference_modules(ctx: TestContext) [fs, error] -> Result[Path] {
  let modules = test.temp_dir(ctx, name: "full-inference-modules")?
  let root = fp"{ctx.file.parent()}/../fixtures/modules/full-inference"
  fp"{modules}/identity.xsh".write(fp"{root}/identity.xsh".read_text()?)?
  fp"{modules}/utilities.xsh".write(fp"{root}/utilities.xsh".read_text()?)?
  modules
}

test test_full_inference_imported_generic_alias_instantiates_int_str_bool { |ctx|
  let modules = full_inference_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use identity as models
let transform = models.identity
let integer = transform(42)
let text = transform("demo")
let boolean = transform(false)
assert integer == 42
assert text == "demo"
assert !boolean
print $integer
print $text
print $boolean
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\nfalse\n"
}

test test_full_inference_imported_generic_alias_survives_constant_field_get_and_index { |ctx|
  let modules = full_inference_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use identity as models
const identity_name = "identity"
let by_field = models.identity
let by_get = models.get(identity_name)?
let by_index = models[identity_name]
let field_integer = by_field(value: 42)
let get_text = by_get("demo")
let index_boolean = by_index(false)
assert field_integer == 42
assert get_text == "demo"
assert !index_boolean
print $field_integer
print $get_text
print $index_boolean
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\nfalse\n"
}

test test_full_inference_imported_alias_flows_through_proc_result_and_producer { |ctx|
  let modules = full_inference_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use identity as models
use utilities
let transform = models.identity
let integers = utilities.apply_many(transform, 42, 2) |> collect()
let number = integers[0]?
let texts = utilities.apply_many(transform, "demo", 1) |> collect()
let text = texts[0]?
assert integers == [Ok(42), Ok(42)]
assert texts == [Ok("demo")]
assert number == 42
assert text == "demo"
print $number
print $text
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_generic_callable_storage_accepts_a_typed_instance { |ctx|
  let modules = full_inference_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use identity as models
type StringIdentity = pure(value: Str) -> Str
let generic = models.identity
let stored: StringIdentity = generic
assert stored("demo") == "demo"
print $stored("demo")
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "demo\n"
}

test test_full_inference_inferred_alias_preserves_labels_rest_and_captures { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""let suffix = "!"
pure finish(value) {
  let captured = suffix
  value
}
let formatter = finish
let formatted = formatter(value: "done")
pure head(prefix, ...items: List[Int]) { prefix }
let integer = head(7)
let text = head("demo", 1, 2)
let boolean = head(false, @[3])
assert formatted == "done"
assert integer == 7
assert text == "demo"
assert !boolean
print $formatted
print $integer
print $text
print $boolean
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "done\n7\ndemo\nfalse\n"
}

test test_full_inference_conditional_callable_selection_preserves_one_contract { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure increment(value: Int) -> Int { value + 1 }
pure decrement(value: Int) -> Int { value - 1 }
let selected = if true {
  let candidate = increment
  candidate
} else {
  let candidate = decrement
  candidate
}
let result = selected(3)
assert result == 4
print $result
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "4\n"
}

test test_full_inference_unannotated_conditional_selection_infers_one_callable { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure choose(flag, first, second) {
  if flag { first } else { second }
}
let chosen = choose(true, 42, 7)
assert chosen == 42
print $chosen
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n"
}

test test_full_inference_callable_values_in_records_keep_the_outer_scheme { |ctx|
  let modules = full_inference_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use identity as models
let operations = {copy: models.identity}
let integer = operations.copy(42)
let text = operations.copy("demo")
assert integer == 42
assert text == "demo"
print $integer
print $text
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_generic_bodies_compose_rows_results_optionals_and_loops { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure choose(flag: Bool, first, second) {
  if flag { first } else { second }
}
pure name_of(value) { value.name }
pure wrap(value) { Ok(value) }
pure maybe(flag: Bool, value) {
  if flag { value } else { null }
}
pure gather(items) {
  var output = []
  for item in items {
    output += [item]
  }
  output
}
let chosen = choose(false, 7, 42)
let maybe_value = maybe(false, 9)
let fallback = maybe_value ?? 0
let wrapped = wrap("Ada")
let name = name_of({name: wrapped?, enabled: true})
let numbers = gather([1, 2, 3])
let words = gather(["one", "two"])
assert chosen == 42
assert fallback == 0
assert name == "Ada"
assert numbers == [1, 2, 3]
assert words == ["one", "two"]
print $chosen
print $name
print $numbers.len()
print $words.len()
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\nAda\n3\n2\n"
}

test test_full_inference_operator_relationships_reuse_per_instantiation { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure combine(left, right) { left + right }
let integer = combine(40, 2)
let text = combine("one", " two")
assert integer == 42
assert text == "one two"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\none two\n"
}

test test_full_inference_operator_relationship_rejects_an_unsupported_instance { |ctx|
  test.expect(
    ctx,
    r"""pure combine(left, right) { left + right }
let invalid = combine(1, true)
""",
    status: 2,
    stderr: ["[check.operator-type]"],
  )?
}

test test_full_inference_private_generic_calls_infer_independent_instances { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure private_identity(value) { value }
pure relay(value) { private_identity(value) }
let integer = relay(42)
let text = relay("demo")
assert integer == 42
assert text == "demo"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_private_module_callables_remain_inaccessible { |ctx|
  let modules = full_inference_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    "use identity as models\nlet value = models.hidden_identity(42)\n",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert !output.success, output.stderr
  assert "[check.unknown-method]" in output.stderr, output.stderr
}

test test_full_inference_dynamic_callable_does_not_gain_a_signature { |ctx|
  test.expect(
    ctx,
    r"""type IntegerIdentity = pure(value: Int) -> Int
pure identity(value: Int) -> Int { value }
let dynamic: Pure = identity
let checked: IntegerIdentity = dynamic
""",
    status: 2,
    stderr: ["[check.callable-mismatch]", "a dynamic `Pure` has no checked signature"],
  )?
}

test test_full_inference_callback_effects_flow_through_lazy_producers { |ctx|
  let modules = full_inference_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use utilities
proc observe(value: Str) [io] -> Result[Str] {
  print "callback"
  Ok(value)
}
let results = utilities.apply_many(observe, "demo", 1) |> collect()
assert results[0]? == "demo"
print "done"
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "callback\ndone\n"
}

test test_full_inference_explicit_effect_bound_rejects_callback_laundering { |ctx|
  test.expect(
    ctx,
    r"""type Observer = proc(value: Str) [io, error] -> Result[Str]
proc forbidden(callback: Observer) [error] -> Result[Str] {
  callback("demo")?
}
""",
    status: 2,
    stderr: ["[check.effect-violation]"],
  )?
}

test test_full_inference_unannotated_callback_parameter_is_monomorphic_across_calls { |ctx|
  test.expect(
    ctx,
    r"""pure apply_at_two_types(callback, integer, text) {
  let first = callback(integer)
  let second = callback(text)
  first
}
pure identity(value) { value }
let result = apply_at_two_types(identity, 42, "demo")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}
