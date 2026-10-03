type RequirementManifest = {name: Str, jobs: UInt}
type RequirementEnvelope = {manifest: RequirementManifest}

test test_require_tail_propagation_consumes_success_unit {
  1 == 1
}

test test_require_infers_target_from_annotated_binding {
  let raw: Any = {name: "ready", jobs: 4}
  let manifest: RequirementManifest = raw.require()?
  manifest.name == "ready"
  manifest.jobs == 4
}

proc require_manifest(raw: Any) [error] -> Result[RequirementManifest] {
  raw.require()?
}

proc require_manifest_return(raw: Any, choose = true) [error] -> Result[RequirementManifest] {
  return raw.require()? when choose
  raw.require()?
}

proc require_manifest_branch(raw: Any, choose: Bool) [error] -> Result[RequirementManifest] {
  if choose { raw.require()? } else { raw.require()? }
}

pure require_manifest_name(manifest: RequirementManifest) -> Str { manifest.name }

test test_require_uses_returns_branches_blocks_and_parameters {
  let raw: Any = {name: "ready", jobs: 4}
  require_manifest(raw)?.name == "ready"
  require_manifest_return(raw)?.name == "ready"
  require_manifest_branch(raw, true)?.jobs == 4
  let block: RequirementManifest = { raw.require()? }
  block.name == "ready"
  require_manifest_name(raw.require()?) == "ready"
  require_manifest_name(...{manifest: raw.require()?}) == "ready"
  let constructed = RequirementEnvelope(manifest: raw.require()?)
  let spread_constructed = RequirementEnvelope(...{manifest: raw.require()?})
  constructed.manifest.name == spread_constructed.manifest.name
  let wrapped: Result[RequirementManifest] = Ok(raw.require()?)
  wrapped?.jobs == 4
}

test test_require_keeps_validation_and_unsigned_conversion {
  let invalid: Any = {name: "ready", jobs: -1}
  let rejected: Result[RequirementManifest] = invalid.require()
  rejected is Err(_)
  let text: Any = "not a record"
  let also_rejected: Result[RequirementManifest] = text.require()
  also_rejected is Err(_)
}

test test_require_rejects_unanchored_targets { |ctx|
  for source in [
    "let raw: Any = 1\nlet value = raw.require()\n",
    "let raw: Any = 1\nlet value: Any = raw.require()?\n",
    "let raw: Any = {}\nlet value: Record = raw.require()?\n",
    "proc choose(raw: Any) [error] -> Int { if raw.require()? { 1 } else { 2 } }\n",
    "let raw: Any = 1\nlet value = raw.require()? ?? 0\n",
    "let raw: Any = p\".\"\nlet value = fs.executable(raw.require()?)\n",
    "let raw: Any = 1\nassert true, raw.require()?\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    let failed = ! rejected.success
    let failure_details = rejected.stderr
    assert failed, failure_details
    "check.require-target" in rejected.stderr
  }
}

test test_require_preserves_wire_enum_conversion_and_nested_contexts { |ctx|
  let executed = test.run_script(ctx, r"""enum State: Str { Ready = "ready", Missing = "" }
type Envelope[T] = {value: T, items: List[T]}
let raw: Any = "ready"
let state: State = raw.require()?
let nested: Envelope[State] = {value: raw.require()?, items: [raw.require()?]}
let mapping: Map[State] = {item: raw.require()?}
print (state == Ready)
print (nested.items[0] == Ready)
print (mapping.get("item")? == Ready)
""")?
  let {success: succeeded, stderr: failure_details, ..} = executed
  assert succeeded, failure_details
  executed.stdout == "true\ntrue\ntrue\n"
}

test test_require_evaluates_receiver_once_and_matches_explicit_failure {
  var calls = 0
  let input: Any = {name: "ready", jobs: 4}
  let manifest: RequirementManifest = (if true { calls += 1; input } else { input }).require()?
  calls == 1
  manifest.jobs == 4
  let invalid: Any = {name: "ready", jobs: -1}
  let inferred: Result[RequirementManifest] = invalid.require()
  let explicit = invalid.require(RequirementManifest)
  if let [Err(left), Err(right)] = [inferred, explicit] {
    left.message == right.message
  } else {
    false
  }
}

test test_require_preserves_each_result_layer {
  let raw: Any = Ok(7)
  let inner: Result[Int] = raw.require()?
  inner? == 7
  let nested: Result[Result[Int]] = raw.require()
  (nested?)? == 7
  let source: Result[Any] = Ok({name: "ready", jobs: 4})
  let manifest: Result[RequirementManifest] = source?.require()
  manifest?.name == "ready"
}

test test_require_keeps_actual_error_contract_and_rejects_future_evidence { |ctx|
  for source in [
    "error Narrow = Bad(message: Str)\ntype Row = {name: Str}\nproc validate(raw: Any) [error] -> Result[Row, Narrow] { raw.require()? }\n",
    "let raw: Any = {name: \"ready\"}\nlet value = raw.require()?\nprint value.name\n",
    "type Box[T] = {value: T, anchor: T}\nlet raw: Any = 1\nlet value = Box(value: raw.require()?, anchor: 1)\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    let failed = ! rejected.success
    let failure_details = rejected.stderr
    assert failed, failure_details
    "check." in rejected.stderr
  }
}
