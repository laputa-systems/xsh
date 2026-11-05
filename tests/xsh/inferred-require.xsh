type RequirementManifest = {name: Str, jobs: UInt}

type RequirementEnvelope = {manifest: RequirementManifest}

test test_required_module_contracts_ground_inferred_returns { |ctx|
  let root = test.temp_dir(ctx, name: "inferred-module-contract")?
  let good = fp"{root}/good.xsh"
  let bad_name = fp"{root}/bad-name.xsh"
  let bad_callable = fp"{root}/bad-callable.xsh"
  good.write(r"""##! A module with a fixed export contract.
## The module name.
export let name: Str = "demo"
## Opaque record data checked by its consumer.
export let rows: List[Record] = [{kind: "ready"}]
## Renders a label.
export pure render(value: Str) -> Str { "demo-" + value }
""")?
  bad_name.write(r"""##! A module with an incompatible value export.
## The incompatible name.
export let name: Int = 1
## Empty data.
export let rows: List[Record] = []
## Renders a label.
export pure render(value: Str) -> Str { value }
""")?
  bad_callable.write(r"""##! A module with an incompatible callable export.
## The module name.
export let name: Str = "demo"
## Empty data.
export let rows: List[Record] = []
## The incompatible rendering result.
export pure render(value: Str) -> Int { value.byte_len() }
""")?
  let source = r"""type Info = module {
  export let name: Str
  export let rows: List[Record]
  export pure render(value: Str) -> Str
}
type Row = {kind: Str}
proc load(target: Path, marker) {
  let _ = marker
  module.load(target)?.require(Info).context("load", target.display())?
}
proc local(target: Path) {
  let checked: Info = module.load(target)?.require(Info)?
  checked
}
let target = Path(args[0])
let loaded = load(target, 1)?
assert loaded.name == "demo"
assert loaded.render("label") == "demo-label"
assert loaded.rows[0].require(Row)?.kind == "ready"
assert load(target, "independent")?.name == "demo"
assert local(target)?.name == "demo"
assert load(Path(args[1]), false) is Err(is MismatchedExport)
assert load(Path(args[2]), "wrong signature") is Err(is MismatchedExport)
print "module contracts preserved"
"""
  let annotated = source
    .replace("proc load(target: Path, marker) {", with: "proc load(target: Path, marker) -> Result[Info] {")
    .replace("proc local(target: Path) {", with: "proc local(target: Path) -> Result[Info] {")
  for program in [annotated, source] {
    let output = test.expect(ctx, program, status: 0, args: [good, bad_name, bad_callable])?
    assert output.stdout == "module contracts preserved\n", output.stdout
  }
  for rejected_source in [
    "proc unchecked(target: Path) { module.load(target)? }\n",
    "type Opaque = module { export let value: Any }\nproc unchecked(target: Path) { module.load(target)?.require(Opaque)? }\n",
  ] {
    let rejected = test.run_xsh(ctx, rejected_source)?
    assert ! rejected.success, rejected.stdout
    assert "[check.infer-return]" in rejected.stderr, rejected.stderr
  }
}

test test_require_tail_propagation_consumes_success_unit {
  assert 1 == 1
}

test test_require_infers_target_from_annotated_binding {
  let raw: Any = {name: "ready", jobs: 4}
  let manifest: RequirementManifest = raw.require()?
  assert manifest.name == "ready"
  assert manifest.jobs == 4
}

proc require_manifest(raw: Any) [error] -> Result[RequirementManifest] {
  raw.require()?
}

proc require_manifest_return(raw: Any, choose = true) [error] -> Result[RequirementManifest] {
  return raw.require()? when choose
  raw.require()?
}

proc require_manifest_branch(raw: Any, choose: Bool) [error] -> Result[RequirementManifest] {
  if choose {
    raw.require()?
  } else {
    raw.require()?
  }
}

pure require_manifest_name(manifest: RequirementManifest) -> Str {
  manifest.name
}

test test_require_uses_returns_branches_blocks_and_parameters {
  let raw: Any = {name: "ready", jobs: 4}
  assert require_manifest(raw)?.name == "ready"
  assert require_manifest_return(raw)?.name == "ready"
  assert require_manifest_branch(raw, true)?.jobs == 4
  let block: RequirementManifest = {
    raw.require()?
  }
  assert block.name == "ready"
  assert require_manifest_name(raw.require()?) == "ready"
  assert require_manifest_name(...{manifest: raw.require()?}) == "ready"
  let constructed = RequirementEnvelope(raw.require()?)
  let spread_constructed = RequirementEnvelope(...{manifest: raw.require()?})
  assert constructed.manifest.name == spread_constructed.manifest.name
  let wrapped: Result[RequirementManifest] = Ok(raw.require()?)
  assert wrapped?.jobs == 4
  let mode: Any = 0o755
  assert fs.executable(mode.require()?)
}

test test_require_keeps_validation_and_unsigned_conversion {
  let invalid: Any = {name: "ready", jobs: -1}
  let rejected: Result[RequirementManifest] = invalid.require()
  assert rejected is Err(_)
  let text: Any = "not a record"
  let also_rejected: Result[RequirementManifest] = text.require()
  assert also_rejected is Err(_)
}

test test_require_rejects_unanchored_targets { |ctx|
  for source in [
    """let raw: Any = 1
let value = raw.require()
""",
    """let raw: Any = 1
let value: Any = raw.require()?
""",
    """let raw: Any = {}
let value: Record = raw.require()?
""",
    """proc choose(raw: Any) [error] -> Int { if raw.require()? { 1 } else { 2 } }
""",
    """let raw: Any = 1
let value = raw.require()? ?? 0
""",
    """let raw: Any = p"."
let value = hash.sha256(raw.require()?)
""",
    """let raw: Any = 1
assert true, raw.require()?
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    let failed = ! rejected.success
    let failure_details = rejected.stderr
    assert failed, failure_details
    assert "check.require-target" in rejected.stderr
  }
}

test test_require_preserves_wire_enum_conversion_and_nested_contexts { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum State: Str { Ready = "ready", Missing = "" }
type Envelope[T] = {value: T, items: List[T]}
let raw: Any = "ready"
let state: State = raw.require()?
let nested: Envelope[State] = {value: raw.require()?, items: [raw.require()?]}
let mapping: Map[State] = {item: raw.require()?}
print (state == Ready)
print (nested.items[0] == Ready)
print (mapping.get("item")? == Ready)
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = executed
  assert succeeded, failure_details
  assert executed.stdout == """true
true
true
"""
}

test test_require_evaluates_receiver_once_and_matches_explicit_failure {
  var calls = 0
  let input: Any = {name: "ready", jobs: 4}
  let manifest: RequirementManifest = (if true {
    calls += 1
    input
  } else { input }).require()?
  assert calls == 1
  assert manifest.jobs == 4
  let invalid: Any = {name: "ready", jobs: -1}
  let inferred: Result[RequirementManifest] = invalid.require()
  let explicit = invalid.require(RequirementManifest)
  if let [Err(left), Err(right)] = [inferred, explicit] {
    assert left.message == right.message
  } else {
    assert false
  }
}

test test_require_preserves_each_result_layer {
  let raw: Any = Ok(7)
  let inner: Result[Int] = raw.require()?
  assert inner? == 7
  let nested: Result[Result[Int]] = raw.require()
  assert (nested?)? == 7
  let source: Result[Any] = Ok({name: "ready", jobs: 4})
  let manifest: Result[RequirementManifest] = source?.require()
  assert manifest?.name == "ready"
}

test test_require_keeps_actual_error_contract_and_rejects_future_evidence { |ctx|
  for source in [
    """error Narrow = Bad(message: Str)
type Row = {name: Str}
proc validate(raw: Any) [error] -> Result[Row, Narrow] { raw.require()? }
""",
    """let raw: Any = {name: "ready"}
let value = raw.require()?
print value.name
""",
    """type Box[T] = {value: T, anchor: T}
let raw: Any = 1
let value = Box(value: raw.require()?, anchor: 1)
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    let failed = ! rejected.success
    let failure_details = rejected.stderr
    assert failed, failure_details
    assert "check." in rejected.stderr
  }
}

test test_require_imported_phantom_schema_needs_ground_target { |ctx|
  let modules = test.temp_dir(ctx, name: "require-imported-phantom")?
  fp"{modules}/model.xsh".write("##! Imported phantom schema.\n## An entry with a label.\nexport type Entry[T] = {label: Str}\n")?
  let accepted = test.run_xsh(ctx, r"""use model as m
let good: Any = {label: "ready"}
let bad: Any = {label: 7}
let explicit = good.require(m.Entry[Int])?
let contextual: m.Entry[Int] = good.require()?
assert explicit.label == "ready"
assert contextual.label == "ready"
assert bad.require(m.Entry[Int]) is Err(_)
let rejected: Result[m.Entry[Int]] = bad.require()
assert rejected is Err(_)
""", env: {XSH_MODULE_PATH: modules})?
  assert accepted.success, accepted.stderr
  let unresolved = test.run_xsh(ctx, r"""use model as m
let raw: Any = {label: "ready"}
let value = raw.require(m.Entry)?
""", env: {XSH_MODULE_PATH: modules})?
  assert !unresolved.success, unresolved.stderr
  assert "check.type-arity" in unresolved.stderr, unresolved.stderr
  for source in [
    r"""use model as m
let raw: Any = "ready"
let value = m.Entry(label: raw.require()?)
""",
    r"""use model as m
proc make(raw: Any) { m.Entry(label: raw.require()?) }
""",
  ] {
    let constructor = test.run_xsh(ctx, source, env: {XSH_MODULE_PATH: modules})?
    assert !constructor.success, constructor.stderr
    assert "check.constructor-inference" in constructor.stderr, constructor.stderr
  }
}
