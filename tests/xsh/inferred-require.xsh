type RequirementManifest = {name: Str, jobs: UInt}
type RequirementEnvelope = {manifest: RequirementManifest}

test test_require_infers_target_from_annotated_binding [error] {
  let raw: Any = {name: "ready", jobs: 4}
  let manifest: RequirementManifest = raw.require()?
  test.eq(manifest.name, "ready")?
  test.eq(manifest.jobs, 4)?
}

proc require_manifest(raw: Any) [error] -> Result[RequirementManifest] {
  raw.require()?
}

proc require_manifest_return(raw: Any) [error] -> Result[RequirementManifest] { return raw.require()? }

proc require_manifest_branch(raw: Any, choose: Bool) [error] -> Result[RequirementManifest] {
  if choose { raw.require()? } else { raw.require()? }
}

pure require_manifest_name(manifest: RequirementManifest) -> Str { manifest.name }

test test_require_uses_returns_branches_blocks_and_parameters [error] {
  let raw: Any = {name: "ready", jobs: 4}
  test.eq(require_manifest(raw)?.name, "ready")?
  test.eq(require_manifest_return(raw)?.name, "ready")?
  test.eq(require_manifest_branch(raw, true)?.jobs, 4)?
  let block: RequirementManifest = { raw.require()? }
  test.eq(block.name, "ready")?
  test.eq(require_manifest_name(raw.require()?), "ready")?
  test.eq(require_manifest_name(...{manifest: raw.require()?}), "ready")?
  let constructed = RequirementEnvelope(manifest: raw.require()?)
  let spread_constructed = RequirementEnvelope(...{manifest: raw.require()?})
  test.eq(constructed.manifest.name, spread_constructed.manifest.name)?
  let wrapped: Result[RequirementManifest] = Ok(raw.require()?)
  test.eq(wrapped?.jobs, 4)?
}

test test_require_keeps_validation_and_unsigned_conversion [error] {
  let invalid: Any = {name: "ready", jobs: -1}
  let rejected: Result[RequirementManifest] = invalid.require()
  match rejected { Err(_) => test.ok(true)?, Ok(_) => test.ok(false)? }
  let text: Any = "not a record"
  let also_rejected: Result[RequirementManifest] = text.require()
  match also_rejected { Err(_) => test.ok(true)?, Ok(_) => test.ok(false)? }
}

test test_require_rejects_unanchored_targets [error] { |ctx|
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
    test.ok(! rejected.success, rejected.stderr)?
    test.contains(rejected.stderr, "check.require-target")?
  }
}

test test_require_preserves_wire_enum_conversion_and_nested_contexts [error] { |ctx|
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
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "true\ntrue\ntrue\n")?
}

test test_require_evaluates_receiver_once_and_matches_explicit_failure [error] {
  var calls = 0
  let input: Any = {name: "ready", jobs: 4}
  let manifest: RequirementManifest = (if true { calls += 1; input } else { input }).require()?
  test.eq(calls, 1)?
  test.eq(manifest.jobs, 4)?
  let invalid: Any = {name: "ready", jobs: -1}
  let inferred: Result[RequirementManifest] = invalid.require()
  let explicit = invalid.require(RequirementManifest)
  match [inferred, explicit] {
    [Err(left), Err(right)] => test.eq(left.message, right.message)?
    _ => test.ok(false)?
  }
}

test test_require_preserves_each_result_layer [error] {
  let raw: Any = Ok(7)
  let inner: Result[Int] = raw.require()?
  test.eq(inner?, 7)?
  let nested: Result[Result[Int]] = raw.require()
  test.eq((nested?)?, 7)?
  let source: Result[Any] = Ok({name: "ready", jobs: 4})
  let manifest: Result[RequirementManifest] = source?.require()
  test.eq(manifest?.name, "ready")?
}

test test_require_keeps_actual_error_contract_and_rejects_future_evidence [error] { |ctx|
  for source in [
    "error Narrow = Bad(message: Str)\ntype Row = {name: Str}\nproc validate(raw: Any) [error] -> Result[Row, Narrow] { raw.require()? }\n",
    "let raw: Any = {name: \"ready\"}\nlet value = raw.require()?\nprint value.name\n",
    "type Box[T] = {value: T, anchor: T}\nlet raw: Any = 1\nlet value = Box(value: raw.require()?, anchor: 1)\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    test.ok(! rejected.success, rejected.stderr)?
    test.contains(rejected.stderr, "check.")?
  }
}
