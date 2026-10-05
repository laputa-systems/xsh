type ProofFirmware = {vendor: Str?}

type ProofReport = {firmware: ProofFirmware}

test test_record_projection_proof_through_boolean_alias {
  let report = ProofReport(firmware: {vendor: " ready "})
  let available = report.firmware.vendor != null
  let retained = available
  guard retained else {
    return error.fail("missing vendor")
  }
  let vendor = proof_string(report.firmware.vendor)
  assert vendor.trim() == "ready"
}

type ProofPair = {left: Str?, right: Int}

test test_record_projection_proofs_keep_disjoint_updates_and_snapshots {
  var pair = ProofPair(left: "ready", right: 1)
  let available = pair.left != null
  pair.right = 2
  guard available else {
    return error.fail("missing left")
  }
  pair.right = 3
  let left = proof_string(pair.left)
  assert left == "ready"
  var original = ProofPair(left: "snapshot", right: 1)
  let snapshot = original
  let retained = snapshot.left != null
  original = {left: null, right: 2}
  guard retained else {
    return error.fail("missing snapshot")
  }
  let value = proof_string(snapshot.left)
  assert value == "snapshot"
}

pure proof_accept(value: Str) -> Bool {
  value != ""
}

pure proof_string(value: Str) -> Str {
  value
}

pure proof_nullable_strings(values: List[Str?]) -> List[Str?] {
  values
}

pure proof_early_exit(report: ProofReport) -> Str {
  let available = report.firmware.vendor != null
  guard available else {
    return "missing"
  }
  report.firmware.vendor
}

test test_record_projection_aliases_short_circuit_and_assert_success {
  let report = ProofReport(firmware: {vendor: "ready"})
  let available = report.firmware.vendor != null
  let accepted = available and proof_accept(report.firmware.vendor)
  assert accepted, "not accepted"
  let value = proof_string(report.firmware.vendor)
  assert value == "ready"
  assert proof_early_exit(report) == "ready"
}

test test_record_projection_proofs_reject_mutation_shadowing_and_recovery { |ctx|
  for source in [
    """type Pair = {left: Str?, right: Int}
var pair: Pair = {left: "ready", right: 1}
let available = pair.left != null
pair.left = null
guard available else { exit 1 }
let value: Str = pair.left
""",
    """type Inner = {value: Str?}
type Outer = {inner: Inner}
var outer: Outer = {inner: {value: "ready"}}
let available = outer.inner.value != null
outer.inner = {value: null}
guard available else { exit 1 }
let value: Str = outer.inner.value
""",
    """let value: Str? = "outer"
let available = value != null
{ let value: Str? = null; guard available else { exit 1 }; let checked: Str = value }
""",
    """var value: Str? = "ready"
let available = value != null
proc mutate() [] { value = null }
guard available else { exit 1 }
mutate()
let checked: Str = value
""",
    """var value: Str? = "ready"
let available = value != null
pure later() -> Str { guard available else { return "missing" }; value }
""",
    """var value: Str? = "ready"
let available = value != null
stream later() [] -> Stream[Str] { guard available else { return }; yield value }
""",
    """var value: Str? = "ready"
let available = value != null
guard available else { exit 1 }
defer { guard available else { exit 1 }; let checked: Str = value }
""",
    """let value: Str? = null
let available = value != null
let recovered: Result[Unit] = try { assert available, "missing" }
let checked: Str = value
""",
    """let value: Str? = null
let available = value != null
if available or true { let checked: Str = value }
""",
    """var value: Str? = "ready"
let available = value != null
guard available else { exit 1 }
[1] |> each { |_| value = null }
let checked: Str = value
""",
  ] {
    let executed = test.run_script(ctx, source)?
    assert executed.success == false
  }
}

proc proof_joined(report: ProofReport, choose: Bool) [error] -> Str {
  let available = report.firmware.vendor != null
  if choose {
    assert available, "missing first"
  } else {
    assert available, "missing second"
  }

  report.firmware.vendor
}

test test_record_projection_join_keeps_only_common_success_proofs { |ctx|
  let report = ProofReport(firmware: {vendor: "ready"})
  assert proof_joined(report, true) == "ready"
  assert proof_joined(report, false) == "ready"
  let rejected = test.run_script(
    ctx,
    """type Item = {value: Str?}
proc select(item: Item, choose: Bool) [error] -> Str { let available = item.value != null; if choose { assert available, "missing" } else { let _ = 1 }; item.value }
""",
  )?
  assert rejected.success == false
}

test test_record_projection_alias_dag_is_bounded_without_expansion { |ctx|
  var source = """let value: Str? = "ready"
let available = value != null
"""
  var previous = "available"
  for index in range(256) {
    let name = f"available_{index}"
    let alias = f"""
      let {name} = {previous} and {previous}

      """
    source = source + alias
    previous = name
  }

  let guard_source = f"""
    guard {previous} else {{ exit 1 }}
    let checked: Str = value

    """
  source = source + guard_source + r"print $checked" + "\n"
  let executed = test.run_script(ctx, source)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """ready
"""
}

test test_record_projection_proved_fallback_skips_runtime_calls { |ctx|
  let executed = test.run_script(
    ctx,
    r"""var calls = 0
proc fallback() [] -> Str { calls += 1; "fallback" }
proc select(value: Str?) [] -> Str {
  let available = value != null
  guard available else { return "missing" }
  value ?? fallback()
}
print ${select("ready")} $calls
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """ready 0
"""
  let retained = test.run_script(
    ctx,
    r"""var value: Str? = "ready"
proc fallback() [] -> Str { value = null; "fallback" }
let available = value != null
guard available else { exit 1 }
let selected = value ?? fallback()
let checked: Str = value
print $selected $checked
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = retained
    assert assertion_condition, assertion_message
  }
  assert retained.stdout == """ready ready
"""
}

test test_record_projection_aliases_combine_presence_and_nullable_field_proofs {
  let report = ProofReport(firmware: {vendor: "ready"})
  let shape = "extra" in report.firmware
  let available = report.firmware.vendor != null
  if available and shape {
    let checked = proof_string(report.firmware.vendor)
    let _ = checked
  }

  let both = available and available
  guard both else {
    return error.fail("missing vendor")
  }
  let checked = proof_string(report.firmware.vendor)
  assert checked == "ready"
}

proc proof_exit_mutation(choose: Bool) [] -> Str {
  var value: Str? = "ready"
  let available = value != null
  if choose {
    value = null
    return "early"
  }

  guard available else {
    value = null
    return "missing"
  }
  let checked = proof_string(value)
  checked
}

test test_record_projection_exiting_mutations_do_not_reach_success { |ctx|
  assert proof_exit_mutation(false) == "ready"
  assert proof_exit_mutation(true) == "early"
  let rejected = test.run_script(
    ctx,
    """var value: Str? = "ready"
let available = value != null
let choose = true
if choose { value = null }
guard available else { exit 1 }
let checked: Str = value
""",
  )?
  assert rejected.success == false
}

test test_record_projection_continue_keeps_success_proof {
  let values = proof_nullable_strings([null, "ready"])
  for value in values {
    let available = value != null
    guard available else {
      continue
    }
    let checked = proof_string(value)
    assert checked == "ready"
  }
}

type ProofData = {payload: Any}

test test_record_projection_type_and_presence_aliases {
  let data = ProofData(payload: "ready")
  let text = data.payload is Str
  guard text else {
    return error.fail("not text")
  }
  let checked = proof_string(data.payload)
  assert checked == "ready"
  let shape: Record = {payload: "present"}
  let present = "payload" in shape
  guard present else {
    return error.fail("missing field")
  }
  let text_field = shape.payload is Str
  guard text_field else {
    return error.fail("not text")
  }
  let value = shape.payload.require(Str)?
  assert value == "present"
}
