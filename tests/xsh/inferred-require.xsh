type RequirementManifest = {name: Str, jobs: UInt}

type RequirementEnvelope = {manifest: RequirementManifest}

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
