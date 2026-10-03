test test_schema_validation_preserves_contextual_composite_wire_and_uint_rules { |ctx|
  let output = test.run_script(
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
  )?
  assert output.success, output.stderr
  assert output.stdout == """validated
rejected
"""
}

test test_schema_validation_keeps_the_declaring_private_schema { |ctx|
  let root = test.temp_dir(ctx, name: "schema-validation-owner")?
  fp"${root}/model.xsh".write_atomic(r"""##! Schema declarations with private owners.
type Private = {count: UInt}
## A public schema with a private field type.
export type Box[T] = {value: T, owner: Private}
## Validate with the declaring schema.
export pure validated(raw: Any) -> Result[Box[Int]] { raw.require()? }
""")?
  let output = test.run_script(
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
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert output.success, output.stderr
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
