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

test test_full_inference_generic_body_reads_a_named_constant_at_independent_types { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""const marker = "constant:"
pure attach(value) {
  {marker: marker, value: value}
}
let integer = attach(42)
let text = attach("demo")
assert integer.marker == "constant:"
assert integer.value == 42
assert text.marker == "constant:"
assert text.value == "demo"
print f"{integer.marker}{integer.value}"
print f"{text.marker}{text.value}"
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "constant:42\nconstant:demo\n"
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
    stderr: ["[check.callable-mismatch]"],
  )?
}

test test_full_inference_sealed_binary_arithmetic_instantiates_independent_domains { |ctx|
  test.expect(
    ctx,
    r"""pure combine(a, b) { a + b }
let number: Int = combine(1, 2)
let text: Str = combine("one", " two")
assert number == 3
assert text == "one two"
""",
    status: 0,
  )?
}

test test_full_inference_sealed_binary_arithmetic_rejects_an_unsupported_tuple { |ctx|
  test.expect(
    ctx,
    r"""pure combine(a, b) { a + b }
let rejected = combine(1, true)
""",
    status: 2,
    stderr: ["[check.operator-type]"],
  )?
}

test test_full_inference_sealed_binary_arithmetic_preserves_dimensions_and_collections { |ctx|
  test.expect(
    ctx,
    r"""pure combine(a, b) { a + b }
pure divide(a, b) { a / b }
pure subtract(a, b) { a - b }
pure relay(a, b) { combine(a, b) }
pure remove(values) { values - {1, 2} }
let number: Int = relay(1, 2)
let text: Str = relay("one", " two")
let joined: List[Int] = combine([1], [2])
let remaining: Set[Int] = subtract({1, 2, 3, 4}, {3, 4})
let ratio: Int = divide(4s, 2s)
let scaled: Duration = divide(4s, 2)
let filtered: Set[Int] = remove({1, 2, 3, 4})
assert number == 3
assert text == "one two"
assert joined == [1, 2]
assert remaining == {1, 2}
assert ratio == 2
assert scaled == 2s
assert filtered == {3, 4}
""",
    status: 0,
  )?
}

test test_full_inference_wrapper_fallback_distinguishes_optional_results_from_result_failures { |ctx|
  test.expect(
    ctx,
    r"""pure optional_fallback(value: Result[Int, Str]?, backup: Result[Int, Str]) -> Result[Int, Str] { value ?? backup }
pure result_fallback(value: Result[Int, Str], backup: Int) -> Int { value ?? backup }
let failed: Result[Int, Str] = Err("kept")
let present: Result[Int, Str]? = failed
assert optional_fallback(present, Ok(0)) == failed
assert result_fallback(failed, 0) == 0
""",
    status: 0,
  )?
  test.expect(
    ctx,
    r"""pure fallback(value, backup) { value ?? backup }
pure relay(value, backup) { fallback(value, backup) }
let failed: Result[Int, Str] = Err("kept")
let present: Result[Int, Str]? = failed
assert relay(present, Ok(0)) == failed
assert relay(failed, 0) == 0
let successful: Result[Int, Str] = Ok(7)
let held_success: Result[Int, Str]? = successful
assert relay(held_success, Ok(0)) == successful
assert relay(successful, 0) == 7
let missing: Result[Int, Str]? = null
let backup: Result[Int, Str] = Ok(0)
assert relay(missing, backup) == backup
let nullable: Result[Int?, Str] = Ok(null)
assert relay(nullable, 3) == null
""",
    status: 0,
  )?
}

test test_full_inference_wrapper_fields_preserve_null_and_optional_field_idempotence { |ctx|
  test.expect(
    ctx,
    r"""pure optional_name(value) { value?.name }
type TextRow = {name: Str}
type IntRow = {name: Int}
type MaybeRow = {name: Str?}
let text: TextRow? = {name: "demo"}
let number: IntRow? = {name: 7}
let missing: TextRow? = null
let nested: MaybeRow? = {name: null}
let text_name: Str? = optional_name(text)
let number_name: Int? = optional_name(number)
let missing_name: Str? = optional_name(missing)
let nullable_name: Str? = optional_name(nested)
assert text_name == "demo"
assert number_name == 7
assert missing_name == null
assert nullable_name == null
""",
    status: 0,
  )?
}

test test_full_inference_wrapper_fields_preserve_result_propagation_and_caught_errors { |ctx|
  test.expect(
    ctx,
    r"""type TextRow = {name: Str}
pure result_name(value) -> Result[Str, Str] { Ok(value?.name) }
pure captured_name(value) { try { value?.name } }
let source: Result[TextRow, Str] = Ok({name: "demo"})
let failure: Result[TextRow, Str] = Err("kept")
let expected_success: Result[Str, Str] = Ok("demo")
let expected_failure: Result[Str, Str] = Err("kept")
assert result_name(source) == expected_success
assert result_name(failure) == expected_failure
let optional: TextRow? = {name: "optional"}
let absent: TextRow? = null
let optional_name: Result[Str?] = captured_name(optional)
let caught_result: Result[Str, Str] = captured_name(source)
let expected_optional: Result[Str?] = Ok("optional")
let expected_absent: Result[Str?] = Ok(null)
assert optional_name == expected_optional
assert caught_result == expected_success
assert captured_name(absent) == expected_absent
assert captured_name(failure) == expected_failure
""",
    status: 0,
  )?
}

test test_full_inference_wrapper_fields_reject_a_second_optional_hop { |ctx|
  test.expect(
    ctx,
    r"""type TextRow = {name: Str}
pure result_name(value) -> Result[Str, Str] { Ok(value?.name) }
let source: Result[TextRow?, Str] = Ok(null)
let rejected = result_name(source)
""",
    status: 2,
    stderr: ["[check.null-safe-field]"],
  )?
}

test test_full_inference_wrapper_handlers_preserve_named_and_implicit_errors { |ctx|
  test.expect(
    ctx,
    r"""pure handled(value, backup) { value ?? { |error| backup } }
pure relay(value, backup) { handled(value, backup) }
pure item(value) { value ?? { . } }
let success: Result[Int, Str] = Ok(3)
let failure: Result[Int, Str] = Err("failure")
assert relay(success, 9) == 3
assert relay(failure, 9) == 9
let text_success: Result[Str, Str] = Ok("kept")
let text_failure: Result[Str, Str] = Err("recovered")
assert item(text_success) == "kept"
assert item(text_failure) == "recovered"
assert handled(text_success, "backup") == "kept"
assert handled(text_failure, "backup") == "backup"
""",
    status: 0,
  )?
}

test test_full_inference_dynamic_return_cleanup_with_monomorphic_capture { |ctx|
  test.expect(
    ctx,
    r"""var errors = 0
var cleanups = 0
proc keep(marker, value: Any, fail_cleanup: Bool) [error] -> Any {
  let _ = marker
  errdefer { errors += 1 }
  defer {
    cleanups += 1
    if fail_cleanup {
      let failure: Result[Unit, Error] = Err(error.failure("cleanup"))
      let _ = failure?
    }
  }
  return value
}
let opaque: Result[Int, Error] = Err(error.failure("opaque"))
let preserved: Result[Any, Error] = try { keep(1, opaque, false) }
let cleanup_failed: Result[Any, Error] = try { keep("marker", opaque, true) }
let expected: Result[Any, Error] = Ok(opaque)
assert preserved == expected
let cleanup_message = match cleanup_failed {
  Err(failure) => failure.message
  Ok(_) => ""
}
assert cleanup_message == "cleanup"
assert errors == 1
assert cleanups == 2
""",
    status: 0,
  )?
}

test test_full_inference_ordering_retains_its_domain_for_quantified_operands { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure less(a, b) { a < b }
pure smaller(a, b) { if a < b { a } else { b } }
pure largest(items) { var best = items[0]; for item in items { if item > best { best = item } }; best }
pure within(value, low, high) { low <= value and value <= high }
stream above(items, floor) { for item in items { if item > floor { yield item } } }
let stronger = smaller
assert less(1, 2)
assert less("a", "b")
assert !less(2.5, 1.5)
assert less(1s, 2s)
assert smaller(3, 2) == 2
assert stronger("b", "a") == "a"
assert largest([1, 5, 2]) == 5
assert largest(["a", "c", "b"]) == "c"
assert within(2.5, 1.0, 3.0)
let kept = above([1, 5, 3], 2) |> collect()
assert kept == [5, 3]
print "ordered"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "ordered\n"
}

test test_full_inference_ordering_rejects_operands_outside_its_domain { |ctx|
  for case in [
    {call: "less(true, false)", rejection: "[check.operator-type]"},
    {call: "less([1], [2])", rejection: "[check.operator-type]"},
    {call: "less(1, \"a\")", rejection: "[check.type-mismatch]"},
  ] {
    test.expect(
      ctx,
      "pure less(a, b) { a < b }\nlet rejected = " + case.call + "\n",
      status: 2,
      stderr: [case.rejection],
    )?
  }
}

test test_full_inference_negation_instantiates_independent_numeric_domains { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure negate(value) { -value }
pure magnitude(value) { if value < 0 { -value } else { value } }
pure magnitude_of(value, zero) { if value < zero { -value } else { value } }
pure scale_down(value) { -value + value + value }
pure last_negated(items) { var kept = items[0]; for item in items { kept = -item }; kept }
let negated = negate
assert negate(4) == -4
assert negate(2.5) == -2.5
assert negated(7) == -7
assert magnitude(-3) == 3
assert magnitude_of(-1.5, 0.0) == 1.5
assert scale_down(5) == 5
assert last_negated([1, 2]) == -2
assert last_negated([1.5, 2.5]) == -2.5
print "negated"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "negated\n"
}

test test_full_inference_negation_rejects_operands_outside_its_domain { |ctx|
  for case in [
    {call: "negate(\"text\")", rejection: "NegationOperand does not support Str"},
    {call: "negate(3s)", rejection: "NegationOperand does not support Duration"},
    {call: "negate(true)", rejection: "NegationOperand does not support Bool"},
  ] {
    test.expect(
      ctx,
      "pure negate(value) { -value }\nlet rejected = " + case.call + "\n",
      status: 2,
      stderr: ["[check.operator-type]", case.rejection],
    )?
  }
}

test test_full_inference_statement_call_classifies_the_instantiated_result { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure ignore(value) -> Unit { let _ = value }
pure relay(callback, value) { callback(value) }
relay(ignore, 1)
relay(ignore, "text")
print "discarded"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "discarded\n"
  test.expect(
    ctx,
    "pure keep(value) { value }\npure relay(callback, value) { callback(value) }\nrelay(keep, 1)\nprint \"tail\"\n",
    status: 2,
    stderr: ["[check.ignored-result]", "ignored `Int` value"],
  )?
}
