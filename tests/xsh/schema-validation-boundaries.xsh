test test_schema_validation_preserves_contextual_composite_wire_and_uint_rules { |ctx|
  let output = test.expect(
    ctx,
    r"""enum State: Str { Ready = "ready", Missing = "missing" }
type Marker[T] = {amount: UInt, state: State}
type TextMarker = Marker[Str]
pure validated(raw: Any) -> Result[Marker[Int]] { raw.require()? }
let raw: Any = {amount: 7, state: "ready"}
let explicit = raw.require(TextMarker)?
let contextual = validated(raw)?
assert explicit.amount == 7, "explicit UInt payload"
assert explicit.state == Ready, "explicit wire enum"
assert contextual.state == Ready, "contextual wire enum"
let many: List[Any] = [{amount: 4, state: "missing"}, null]
let composite: List[Marker[Bool]?] = many.require()?
assert composite[0]?.amount == 4, "nested UInt payload"
assert composite[0]?.state == Missing, "nested wire enum"
assert composite[1] == null, "optional slot"
print "validated"
let wrong: Any = {amount: -1, state: "ready"}
match wrong.require(TextMarker) {
  Err(_) => print rejected
  Ok(_) => print unexpected
}
""",
    status: 0,
  )?
  assert output.stdout == """validated
rejected
"""
}

test test_schema_validation_keeps_the_declaring_private_schema { |ctx|
  let root = test.temp_dir(ctx, name: "schema-validation-owner")?
  fp"{root}/model.xsh".write_atomic(r"""##! Schema declarations with private owners.
type Private = {count: UInt}
## A public schema with a private field type.
export type Box[T] = {value: T, owner: Private}
## Validate with the declaring schema.
export pure validated(raw: Any) -> Result[Box[Int], Error] { raw.require()? }
""")
  let output = test.expect(
    ctx,
    r"""use model as m
type Private = {count: Str}
let raw: Any = {value: 4, owner: {count: 7}}
let direct = raw.require(m.Box[Int])?
let forwarded = m.validated(raw)?
assert direct.owner.count == 7, "qualified private owner"
assert forwarded.owner.count == 7, "forwarded private owner"
print "validated"
""",
    status: 0,
    args: [],
    env: {XSH_MODULE_PATH: root},
  )?
  assert output.stdout == """validated
"""
}

test test_schema_validation_rejects_context_inferred_from_desired_access { |ctx|
  for source in [
    """let raw: Any = {name: "valid"}
let name = raw.require()?.name
print reached
""",
    """pure invalid(raw: Any) -> Result[Any] { raw.require()? }
print reached
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert output.stdout == ""
    assert "check.require-target" in output.stderr
  }
}

type TableSection = {size: Int}

test test_decoded_tables_and_objects_require_as_string_keyed_maps {
  let table = toml.decode("[a]\nsize = 1\n[b]\nsize = 2\n")?
  let sections = table.require(Map[Str, Any])?
  assert sections.len() == 2
  assert sections.keys() == ["a", "b"]
  assert json.encode(sections)? == r"""{"a":{"size":1},"b":{"size":2}}"""
  let typed = table.require(Map[Str, TableSection])?
  assert typed.get("b")?.size == 2
  let object = json.decode(r"""{"x": 1, "y": 2}""")?
  let counts = object.require(Map[Str, Int])?
  assert counts.get("y")? == 2
  let nested = json.decode(r"""{"outer": {"inner": 1}}""")?
  assert nested.require(Map[Str, Map[Str, Int]])?.get("outer")?.get("inner")? == 1
}

type TableWrapper = {sections: Map[Str, Any], tags: List[Map[Str, Any]]}

test test_map_slots_inside_records_and_lists_hold_maps_after_require {
  let raw = json.decode(r"""{"sections": {"a": 1}, "tags": [{"k": 1}, {}]}""")?
  let wrapper = raw.require(TableWrapper)?
  let expected: Map[Str, Any] = {["a"]: 1}
  assert wrapper.sections == expected
  assert wrapper.tags.len() == 2
  assert wrapper.tags[0].keys() == ["k"]
  assert wrapper.tags[1].is_empty()
}

test test_map_requirements_still_reject_other_shapes {
  let wrong_value = json.decode(r"""{"x": "text"}""")?
  assert wrong_value.require(Map[Str, Int]) is Err(_)
  let not_a_table: Any = [1, 2]
  assert not_a_table.require(Map[Str, Any]) is Err(_)
  let object = json.decode(r"""{"1": 1}""")?
  assert object.require(Map[Int, Int]) is Err(_)
}
