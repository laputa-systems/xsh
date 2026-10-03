
test test_parametric_record_aliases_keep_selected_field_types { |ctx|
  let output = test.run_script(ctx, r"""type ParametricObservation[T] = {state: Str, value: T? = null, samples: List[T] = [], options: Map[T] = {}}
type ParametricName = ParametricObservation[Str]
type ParametricCount = ParametricObservation[Int]
proc witness() [error] -> Result[Unit] {
  let count = ParametricCount(state: "observed", value: 7, samples: [3, 7])
  let name = ParametricName(state: "observed", value: "demo", samples: ["demo"])
  let {value: count_value, samples: count_samples, ..} = count
  let {value: name_value, ..} = name
  ((count_value ?? 0) + count_samples[0]) == (10)
  (name_value ?? "missing") == ("demo")
  let missing = ParametricCount(state: "absent")
  (missing.value) == (null)
  (missing.samples.len()) == (0)
  (missing.options.len()) == (0)
  var changed = ParametricCount(state: "observed")
  let snapshot = changed
  changed.samples += [9]
  (changed.samples[0]) == (9)
  (snapshot.samples.len()) == (0)
  let literal: ParametricObservation[Int] = {state: "observed", value: 4, samples: [4], options: {}}
  (literal.samples[0]) == (4)
}
witness()
""")?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  output.stdout == ""
}

test test_parametric_records_reject_wrong_specializations_and_nonuniversal_defaults { |ctx|
  for source in [
    "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet value = Count(value: \"wrong\")\n",
    "type Box[T] = {value: T = 1}\n",
    "type Box[T] = {value: T? = 1}\n",
    "type Box[T] = {values: List[T] = [1]}\n",
    "type Box[T] = {value: T}\nlet value: Box[Int] = {value: \"wrong\"}\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
    ("check.type-mismatch" in rejected.stderr)
  }
}

test test_parametric_records_reject_invalid_parameters_and_expanding_types { |ctx|
  for source in [
    "type Box[T, T] = {value: T}\n",
    "type Box[Int] = {value: Int}\n",
    "type Box[T] = {value: Missing}\n",
    "type Box[T] = {value: T}\ntype Bad = Box\n",
    "type Box[T] = {value: T}\ntype Bad = Box[Int, Str]\n",
    "type Bad = Int[Str]\n",
    "type Box[T] = {value: Box[List[T]]}\n",
    "type Box[T] = {value: T}\ntype Bad[T] = Bad[List[T]]\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
    ("check." in rejected.stderr)
  }
  let generic_enum = test.run_script(ctx, "enum Variant[T] { First, Second }\n")?
  {
    let assertion_condition = ! generic_enum.success
    let assertion_message = generic_enum.stderr
    assert assertion_condition, assertion_message
  }
  ("parse." in generic_enum.stderr)

}

test test_parametric_record_require_preserves_nested_selected_types { |ctx|
  let executed = test.run_script(ctx, r"""type Box[T] = {value: T}
type Envelope[T] = {item: Box[T], items: List[Box[T]], maybe: Box[T]?}
type CountEnvelope = Envelope[Int]
let raw: Record = {item: {value: 5}, items: [{value: 7}], maybe: null}
let checked = raw.require(CountEnvelope)?
print ${checked.item.value + checked.items[0].value}
let wrong: Record = {item: {value: "wrong"}, items: [], maybe: null}
match wrong.require(CountEnvelope) {
  Err(_) => print "rejected"
  Ok(_) => print "unexpected"
}
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  (executed.stdout) == ("12\nrejected\n")
}

test test_parametric_records_use_declaring_private_dependencies { |ctx|
  let root = test.temp_dir(ctx, name: "parametric-schema-module")?
  fp"${root}/model.xsh".write_atomic("""##! Parameterized schemas with private dependencies.
type Local = {name: Str}
## A record with a declaration-owned dependency.
export type Box[T] = {value: T, owner: Local}
## A parameterized alias retains its declaring scope.
export type Alias[T] = Box[List[T]]
## A concrete constructor keeps the public alias.
export type Counts = Alias[Int]
""")?
  let executed = test.run_script(ctx, r"""use model as m
type Local = {name: Int}
let direct: m.Box[Int] = {value: 5, owner: {name: "module"}}
let value = m.Counts(value: [3, 7], owner: {name: "owner"})
print ${direct.value + value.value[1]}
print ${value.owner.name.upper()}
""", [], {XSH_MODULE_PATH: root.display()})?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  (executed.stdout) == ("12\nOWNER\n")
}

test test_parametric_records_prepare_concrete_alias_constructors_and_typed_literals { |ctx|
  let executed = test.run_script(ctx, r"""type Box[T] = {value: T, items: List[T] = []}
type Count = Box[Int]
const literal: Box[Int] = {value: 3, items: [4]}
const made = Count(value: 7)
const spread = Count(...{value: 9})
print ${literal.value + literal.items[0]}
print ${made.value + made.items.len()}
print $spread.value
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  (executed.stdout) == ("7\n7\n9\n")
}

test test_parametric_records_keep_specialization_through_nested_updates { |ctx|
  let executed = test.run_script(ctx, r"""type Box[T] = {value: T}
type Envelope[T] = {item: Box[T], label: Str}
type Counts = Envelope[Int]
let base = Counts(item: {value: 3}, label: "same")
let changed = {...base, item.value: 7}
print ${changed.item.value + base.item.value}
print $changed.label
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  (executed.stdout) == ("10\nsame\n")
}

test test_parametric_records_preserve_nominal_enum_arguments { |ctx|
  let accepted = test.run_script(ctx, r"""enum State { Ready, Absent }
type Box[T] = {value: T}
type StateBox = Box[State]
let value = StateBox(value: Ready)
print (value.value == Ready)
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = accepted
    assert assertion_condition, assertion_message
  }
  (accepted.stdout) == ("true\n")
  let rejected = test.run_script(ctx, r"""enum One { First }
enum Two { Second }
type Box[T] = {value: T}
type FirstBox = Box[One]
let wrong = FirstBox(value: Second)
""")?
  {
    let assertion_condition = ! rejected.success
    let assertion_message = rejected.stderr
    assert assertion_condition, assertion_message
  }
  ("check.type-mismatch" in rejected.stderr)
}

test test_parametric_record_instances_keep_exact_field_types { |ctx|
  let declaration = r"""type Observation[T] = {state: Str, value: T? = null, samples: List[T] = []}
type CountObservation = Observation[Int]
let count = CountObservation(state: "observed", value: 7, samples: [7])
"""
  let accepted = test.run_script(ctx, declaration + r"""let sample: Int = count.samples[0]
let maybe: Int? = count.value
let generic: Observation[Int] = count
print ${sample + (maybe ?? 0) + generic.samples.len()}
""")?
  assert accepted.success, accepted.stderr
  accepted.stdout == "15\n"
  for line in [
    "let wrong: Str = count.samples[0]\n",
    "let wrong: Int = count.value\n",
    "let wrong: Observation[Str] = count\n",
  ] {
    let rejected = test.run_script(ctx, declaration + line)?
    assert !rejected.success, line
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }
}

test test_parametric_records_do_not_add_generic_functions_or_error_families { |ctx|
  for source in [
    "pure first[T](value: T) -> T { value }\n",
    "error Failure[T] = Bad(value: T)\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert !rejected.success, source
  }
}
