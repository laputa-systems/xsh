test test_parametric_record_aliases_keep_selected_field_types { |ctx|
  let output = test.run_script(
    ctx,
    r"""type ParametricObservation[T] = {state: Str, value: T? = null, samples: List[T] = [], options: Map[T] = {}}
type ParametricName = ParametricObservation[Str]
type ParametricCount = ParametricObservation[Int]
proc witness() [error] -> Result[Unit] {
  let count = ParametricCount(state: "observed", value: 7, samples: [3, 7])
  let name = ParametricName(state: "observed", value: "demo", samples: ["demo"])
  let {value: count_value, samples: count_samples, ..} = count
  let {value: name_value, ..} = name
  assert (count_value ?? 0) + count_samples[0] == 10
  assert (name_value ?? "missing") == "demo"
  let missing = ParametricCount(state: "absent")
  assert missing.value == null
  assert missing.samples.len() == 0
  assert missing.options.len() == 0
  var changed = ParametricCount(state: "observed")
  let snapshot = changed
  changed.samples += [9]
  assert changed.samples[0] == 9
  assert snapshot.samples.len() == 0
  let literal: ParametricObservation[Int] = {state: "observed", value: 4, samples: [4], options: {}}
  assert literal.samples[0] == 4
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_parametric_records_reject_wrong_specializations_and_nonuniversal_defaults { |ctx|
  for source in [
    """type Box[T] = {value: T}
type Count = Box[Int]
let value = Count(value: "wrong")
""",
    """type Box[T] = {value: T = 1}
""",
    """type Box[T] = {value: T? = 1}
""",
    """type Box[T] = {values: List[T] = [1]}
""",
    """type Box[T] = {value: T}
let value: Box[Int] = {value: "wrong"}
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
    assert "check.type-mismatch" in rejected.stderr
  }
}

test test_parametric_records_reject_invalid_parameters_and_expanding_types { |ctx|
  for source in [
    """type Box[T, T] = {value: T}
""",
    """type Box[Int] = {value: Int}
""",
    """type Box[T] = {value: Missing}
""",
    """type Box[T] = {value: T}
type Bad = Box
""",
    """type Box[T] = {value: T}
type Bad = Box[Int, Str]
""",
    """type Bad = Int[Str]
""",
    """type Box[T] = {value: Box[List[T]]}
""",
    """type Box[T] = {value: T}
type Bad[T] = Bad[List[T]]
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
    assert "check." in rejected.stderr
  }

  let generic_enum = test.run_script(
    ctx,
    """enum Variant[T] { First, Second }
""",
  )?
  {
    let assertion_condition = ! generic_enum.success
    let assertion_message = generic_enum.stderr
    assert assertion_condition, assertion_message
  }
  assert "parse." in generic_enum.stderr
}

test test_parametric_record_require_preserves_nested_selected_types { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Box[T] = {value: T}
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """12
rejected
"""
}

test test_parametric_records_use_declaring_private_dependencies { |ctx|
  let root = test.temp_dir(ctx, name: "parametric-schema-module")?
  fp"{root}/model.xsh".write_atomic("""##! Parameterized schemas with private dependencies.
type Local = {name: Str}
## A record with a declaration-owned dependency.
export type Box[T] = {value: T, owner: Local}
## A parameterized alias retains its declaring scope.
export type Alias[T] = Box[List[T]]
## A concrete constructor keeps the public alias.
export type Counts = Alias[Int]
""")
  let executed = test.run_script(
    ctx,
    r"""use model as m
type Local = {name: Int}
let direct: m.Box[Int] = {value: 5, owner: {name: "module"}}
let value = m.Counts(value: [3, 7], owner: {name: "owner"})
print ${direct.value + value.value[1]}
print ${value.owner.name.upper()}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """12
OWNER
"""
}

test test_parametric_records_prepare_concrete_alias_constructors_and_typed_literals { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Box[T] = {value: T, items: List[T] = []}
type Count = Box[Int]
const literal: Box[Int] = {value: 3, items: [4]}
const made = Count(value: 7)
const spread = Count(...{value: 9})
print ${literal.value + literal.items[0]}
print ${made.value + made.items.len()}
print $spread.value
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """7
7
9
"""
}

test test_parametric_records_keep_specialization_through_nested_updates { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Box[T] = {value: T}
type Envelope[T] = {item: Box[T], label: Str}
type Counts = Envelope[Int]
let base = Counts(item: {value: 3}, label: "same")
let changed = {...base, item.value: 7}
print ${changed.item.value + base.item.value}
print $changed.label
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """10
same
"""
}

test test_parametric_records_preserve_nominal_enum_arguments { |ctx|
  let accepted = test.run_script(
    ctx,
    r"""enum State { Ready, Absent }
type Box[T] = {value: T}
type StateBox = Box[State]
let value = StateBox(value: Ready)
print (value.value == Ready)
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = accepted
    assert assertion_condition, assertion_message
  }
  assert accepted.stdout == """true
"""
  let rejected = test.run_script(
    ctx,
    r"""enum One { First }
enum Two { Second }
type Box[T] = {value: T}
type FirstBox = Box[One]
let wrong = FirstBox(value: Second)
""",
  )?
  {
    let assertion_condition = ! rejected.success
    let assertion_message = rejected.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.type-mismatch" in rejected.stderr
}

test test_parametric_record_instances_keep_exact_field_types { |ctx|
  let declaration = r"""type Observation[T] = {state: Str, value: T? = null, samples: List[T] = []}
type CountObservation = Observation[Int]
let count = CountObservation(state: "observed", value: 7, samples: [7])
"""
  let accepted = test.expect(
    ctx,
    declaration + r"""let sample: Int = count.samples[0]
let maybe: Int? = count.value
let generic: Observation[Int] = count
print ${sample + (maybe ?? 0) + generic.samples.len()}
""",
    status: 0,
  )?
  assert accepted.stdout == """15
"""
  for line in [
    """let wrong: Str = count.samples[0]
""",
    """let wrong: Int = count.value
""",
    """let wrong: Observation[Str] = count
""",
  ] {
    let rejected = test.run_script(ctx, declaration + line)?
    assert ! rejected.success, line
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }
}

test test_parametric_records_do_not_add_generic_functions { |ctx|
  let rejected = test.run_script(
    ctx,
    """pure first[T](value: T) -> T { value }
""",
  )?
  assert ! rejected.success, rejected.stderr
}

test test_parametric_record_mismatch_names_the_type_applications { |ctx|
  let rejected = test.run_script(
    ctx,
    r"""type Observation[T] = {state: Str, samples: List[T] = []}
type CountObservation = Observation[Int]
let count = CountObservation(state: "observed", samples: [7])
let wrong: Observation[Str] = count
""",
  )?
  assert ! rejected.success, rejected.stdout
  assert "expected Observation[Str], found Observation[Int]" in rejected.stderr, rejected.stderr
}
