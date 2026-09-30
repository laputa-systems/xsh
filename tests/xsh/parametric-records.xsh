type ParametricObservation[T] = {state: Str, value: T? = null, samples: List[T] = [], options: Map[T] = {}}
type ParametricName = ParametricObservation[Str]
type ParametricCount = ParametricObservation[Int]

test test_parametric_record_aliases_keep_selected_field_types [error] {
  let count = ParametricCount(state: "observed", value: 7, samples: [3, 7])
  let name = ParametricName(state: "observed", value: "demo", samples: ["demo"])
  let {value: count_value, samples: count_samples, ..} = count
  let {value: name_value, ..} = name
  test.eq((count_value ?? 0) + count_samples[0], 10)?
  test.eq(name_value ?? "missing", "demo")?
  let missing = ParametricCount(state: "absent")
  test.eq(missing.value, null)?
  test.eq(missing.samples.len(), 0)?
  test.eq(missing.options.len(), 0)?
  var changed = ParametricCount(state: "observed")
  let snapshot = changed
  changed.samples += [9]
  test.eq(changed.samples[0], 9)?
  test.eq(snapshot.samples.len(), 0)?
  let literal: ParametricObservation[Int] = {state: "observed", value: 4, samples: [4], options: {}}
  test.eq(literal.samples[0], 4)?
}

test test_parametric_records_reject_wrong_specializations_and_nonuniversal_defaults [error] { |ctx|
  for source in [
    "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet value = Count(value: \"wrong\")\n",
    "type Box[T] = {value: T = 1}\n",
    "type Box[T] = {value: T? = 1}\n",
    "type Box[T] = {values: List[T] = [1]}\n",
    "type Box[T] = {value: T}\nlet value: Box[Int] = {value: \"wrong\"}\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    test.ok(! rejected.success, rejected.stderr)?
    test.ok("check.type-mismatch" in rejected.stderr)?
  }
}

test test_parametric_records_reject_invalid_parameters_and_expanding_types [error] { |ctx|
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
    test.ok(! rejected.success, rejected.stderr)?
    test.ok("check." in rejected.stderr)?
  }
  let generic_enum = test.run_script(ctx, "enum Variant[T] { First, Second }\n")?
  test.ok(! generic_enum.success, generic_enum.stderr)?
  test.ok("parse." in generic_enum.stderr)?

}

test test_parametric_record_require_preserves_nested_selected_types [error] { |ctx|
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
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "12\nrejected\n")?
}

test test_parametric_records_use_declaring_private_dependencies [fs, error] { |ctx|
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
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "12\nOWNER\n")?
}

test test_parametric_records_prepare_concrete_alias_constructors_and_typed_literals [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Box[T] = {value: T, items: List[T] = []}
type Count = Box[Int]
const literal: Box[Int] = {value: 3, items: [4]}
const made = Count(value: 7)
const spread = Count(...{value: 9})
print ${literal.value + literal.items[0]}
print ${made.value + made.items.len()}
print $spread.value
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "7\n7\n9\n")?
}

test test_parametric_records_keep_specialization_through_nested_updates [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Box[T] = {value: T}
type Envelope[T] = {item: Box[T], label: Str}
type Counts = Envelope[Int]
let base = Counts(item: {value: 3}, label: "same")
let changed = {...base, item.value: 7}
print ${changed.item.value + base.item.value}
print $changed.label
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "10\nsame\n")?
}

test test_parametric_records_preserve_nominal_enum_arguments [error] { |ctx|
  let accepted = test.run_script(ctx, r"""enum State { Ready, Absent }
type Box[T] = {value: T}
type StateBox = Box[State]
let value = StateBox(value: Ready)
print (value.value == Ready)
""")?
  test.ok(accepted.success, accepted.stderr)?
  test.eq(accepted.stdout, "true\n")?
  let rejected = test.run_script(ctx, r"""enum One { First }
enum Two { Second }
type Box[T] = {value: T}
type FirstBox = Box[One]
let wrong = FirstBox(value: Second)
""")?
  test.ok(! rejected.success, rejected.stderr)?
  test.ok("check.type-mismatch" in rejected.stderr)?
}
