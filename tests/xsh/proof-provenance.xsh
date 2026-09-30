type ProofFirmware = {vendor: Str?}
type ProofReport = {firmware: ProofFirmware}

test test_record_projection_proof_through_boolean_alias [error] {
  let report = ProofReport(firmware: {vendor: " ready "})
  let available = report.firmware.vendor != null
  let retained = available
  guard retained else { return error.fail("missing vendor") }
  let vendor = proof_string(report.firmware.vendor)
  (vendor.trim()) == ("ready")
}

type ProofPair = {left: Str?, right: Int}

test test_record_projection_proofs_keep_disjoint_updates_and_snapshots [error] {
  var pair = ProofPair(left: "ready", right: 1)
  let available = pair.left != null
  pair.right = 2
  guard available else { return error.fail("missing left") }
  pair.right = 3
  let left = proof_string(pair.left)
  (left) == ("ready")
  var original = ProofPair(left: "snapshot", right: 1)
  let snapshot = original
  let retained = snapshot.left != null
  original = {left: null, right: 2}
  guard retained else { return error.fail("missing snapshot") }
  let value = proof_string(snapshot.left)
  (value) == ("snapshot")
}

pure proof_accept(value: Str) -> Bool { value != "" }
pure proof_string(value: Str) -> Str { value }
pure proof_nullable_strings(values: List[Str?]) -> List[Str?] { values }
pure proof_early_exit(report: ProofReport) -> Str {
  let available = report.firmware.vendor != null
  guard available else { return "missing" }
  report.firmware.vendor
}

test test_record_projection_aliases_short_circuit_and_assert_success [error] {
  let report = ProofReport(firmware: {vendor: "ready"})
  let available = report.firmware.vendor != null
  let accepted = available and proof_accept(report.firmware.vendor)
  assert accepted, "not accepted"
  let value = proof_string(report.firmware.vendor)
  (value) == ("ready")
  (proof_early_exit(report)) == ("ready")
}

test test_record_projection_proofs_reject_mutation_shadowing_and_recovery [error] { |ctx|
  for source in [
    "type Pair = {left: Str?, right: Int}\nvar pair: Pair = {left: \"ready\", right: 1}\nlet available = pair.left != null\npair.left = null\nguard available else { abort(1) }\nlet value: Str = pair.left\n",
    "type Inner = {value: Str?}\ntype Outer = {inner: Inner}\nvar outer: Outer = {inner: {value: \"ready\"}}\nlet available = outer.inner.value != null\nouter.inner = {value: null}\nguard available else { abort(1) }\nlet value: Str = outer.inner.value\n",
    "let value: Str? = \"outer\"\nlet available = value != null\n{ let value: Str? = null; guard available else { abort(1) }; let checked: Str = value }\n",
    "var value: Str? = \"ready\"\nlet available = value != null\nproc mutate() [] { value = null }\nguard available else { abort(1) }\nmutate()\nlet checked: Str = value\n",
    "var value: Str? = \"ready\"\nlet available = value != null\npure later() -> Str { guard available else { return \"missing\" }; value }\n",
    "var value: Str? = \"ready\"\nlet available = value != null\nstream later() [] -> Stream[Str] { guard available else { return }; yield value }\n",
    "var value: Str? = \"ready\"\nlet available = value != null\nguard available else { abort(1) }\ndefer { guard available else { abort(1) }; let checked: Str = value }\n",
    "let value: Str? = null\nlet available = value != null\nlet recovered: Result[Unit] = try { assert available, \"missing\" }\nlet checked: Str = value\n",
    "let value: Str? = null\nlet available = value != null\nif available or true { let checked: Str = value }\n",
    "var value: Str? = \"ready\"\nlet available = value != null\nguard available else { abort(1) }\n[1] |> each { |_| value = null }\nlet checked: Str = value\n",
  ] {
    let executed = test.run_script(ctx, source)?
    (executed.success) == (false)
  }
}

proc proof_joined(report: ProofReport, choose: Bool) [error] -> Str {
  let available = report.firmware.vendor != null
  if choose { assert available, "missing first" } else { assert available, "missing second" }
  report.firmware.vendor
}

test test_record_projection_join_keeps_only_common_success_proofs [error] { |ctx|
  let report = ProofReport(firmware: {vendor: "ready"})
  (proof_joined(report, true)) == ("ready")
  (proof_joined(report, false)) == ("ready")
  let rejected = test.run_script(ctx, "type Item = {value: Str?}\nproc select(item: Item, choose: Bool) [error] -> Str { let available = item.value != null; if choose { assert available, \"missing\" } else { let _ = 1 }; item.value }\n")?
  (rejected.success) == (false)
}

test test_record_projection_alias_dag_is_bounded_without_expansion [error] { |ctx|
  var source = "let value: Str? = \"ready\"\nlet available = value != null\n"
  var previous = "available"
  for index in range(256) {
    let name = f"available_${index}"
    let alias = f"""
      let ${name} = ${previous} and ${previous}

      """
    source = source + alias
    previous = name
  }
  let guard_source = f"""
    guard ${previous} else { abort(1) }
    let checked: Str = value

    """
  source = source + guard_source + r"print $checked" + "\n"
  let executed = test.run_script(ctx, source)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  (executed.stdout) == ("ready\n")
}

test test_record_projection_proved_fallback_skips_runtime_calls [error] { |ctx|
  let executed = test.run_script(ctx, r"""var calls = 0
proc fallback() [] -> Str { calls += 1; "fallback" }
proc select(value: Str?) [] -> Str {
  let available = value != null
  guard available else { return "missing" }
  value ?? fallback()
}
print ${select("ready")} $calls
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  (executed.stdout) == ("ready 0\n")
  let retained = test.run_script(ctx, r"""var value: Str? = "ready"
proc fallback() [] -> Str { value = null; "fallback" }
let available = value != null
guard available else { abort(1) }
let selected = value ?? fallback()
let checked: Str = value
print $selected $checked
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = retained
    assert assertion_condition, assertion_message
  }
  (retained.stdout) == ("ready ready\n")
}

test test_record_projection_aliases_combine_presence_and_nullable_field_proofs [error] {
  let report = ProofReport(firmware: {vendor: "ready"})
  let shape = ("extra" in report.firmware)
  let available = report.firmware.vendor != null
  if available and shape {
    let checked = proof_string(report.firmware.vendor)
    let _ = checked
  }
  let both = available and available
  guard both else { return error.fail("missing vendor") }
  let checked = proof_string(report.firmware.vendor)
  (checked) == ("ready")
}

proc proof_exit_mutation(choose: Bool) [] -> Str {
  var value: Str? = "ready"
  let available = value != null
  if choose { value = null; return "early" }
  guard available else { value = null; return "missing" }
  let checked = proof_string(value)
  checked
}

test test_record_projection_exiting_mutations_do_not_reach_success [error] { |ctx|
  (proof_exit_mutation(false)) == ("ready")
  (proof_exit_mutation(true)) == ("early")
  let rejected = test.run_script(ctx, "var value: Str? = \"ready\"\nlet available = value != null\nlet choose = true\nif choose { value = null }\nguard available else { abort(1) }\nlet checked: Str = value\n")?
  (rejected.success) == (false)
}


test test_record_projection_continue_keeps_success_proof [error] {
  let values = proof_nullable_strings([null, "ready"])
  for value in values {
    let available = value != null
    guard available else { continue }
    let checked = proof_string(value)
    (checked) == ("ready")
  }
}


type ProofData = {payload: Any}
test test_record_projection_type_and_presence_aliases [error] {
  let data = ProofData(payload: "ready")
  let text = data.payload is Str
  guard text else { return error.fail("not text") }
  let checked = proof_string(data.payload)
  (checked) == ("ready")
  let shape: Record = {payload: "present"}
  let present = ("payload" in shape)
  guard present else { return error.fail("missing field") }
  let text_field = shape.payload is Str
  guard text_field else { return error.fail("not text") }
  let value = shape.payload.require(Str)?
  (value) == ("present")
}
