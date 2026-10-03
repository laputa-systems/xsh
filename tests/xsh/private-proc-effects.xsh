test test_private_proc_effects_are_inferred_transitively { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [] -> Int {
  forwarding()
}

proc forwarding() -> Int {
  leaf()
}
proc leaf() -> Int {
  42
}
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """42
"""
}

test test_private_proc_effects_reach_recursive_fixed_point { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [] -> Int { even(4) }
proc even(value: Int) -> Int {
  if value == 0 { return 42 }
  odd(value - 1)
}
proc odd(value: Int) -> Int { even(value - 1) }
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """42
"""
}

test test_private_proc_effects_keep_transitive_host_requirements { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [time] -> Int { forwarding() }
proc forwarding() -> Int { clock() }
proc clock() -> Int {
  let _ = time.now()
  42
}
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """42
"""
}

test test_private_proc_effects_enforce_explicit_upper_bounds { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [] -> Int { deliberate_bound() }
proc deliberate_bound() [time] -> Int { 42 }
""",
  )?
  assert output.status != 0
  assert "check.effect-violation" in output.stderr == true
  assert "time" in output.stderr == true
}

test test_private_proc_effects_do_not_execute_references { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [] -> Int {
  let reference = clock
  42
}
proc clock() -> Int {
  let _ = time.now()
  0
}
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """42
"""
}

test test_private_proc_effects_preserve_proc_pure_separation { |ctx|
  let output = test.run_xsh(
    ctx,
    """
pure caller() -> Int { helper() }
proc helper() -> Int { 42 }
""",
  )?
  assert output.status != 0
  assert "check.pure-effect" in output.stderr == true
}

test test_private_proc_effects_report_unknown_dynamic_call_chain { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller(callback: Proc) [] -> Int { forwarding(callback) }
proc forwarding(callback: Proc) -> Int { dynamic(callback) }
proc dynamic(callback: Proc) -> Int {
  let _ = callback.call()
  42
}
""",
  )?
  assert output.status != 0
  assert "check.effect-violation" in output.stderr == true
  assert "forwarding -> dynamic -> Proc.call" in output.stderr == true
}

test test_private_proc_effects_include_implicit_assertion_failure { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [] -> Int { checked() }
proc checked() -> Int {
  assert true
  42
}
""",
  )?
  assert output.status != 0
  assert "check.effect-violation" in output.stderr == true
  assert "error" in output.stderr == true
}

test test_private_proc_effects_capture_assertion_error_locally { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [] -> Int { locally_caught() }
proc locally_caught() -> Int {
  let outcome = try { assert false; 0 }
  42
}
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """42
"""
}

test test_private_proc_effects_capture_keeps_host_requirements { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [] -> Int { locally_caught() }
proc locally_caught() -> Int {
  let outcome = try {
    let _ = time.now()
    false
  }
  42
}
""",
  )?
  assert output.status != 0
  assert "check.effect-violation" in output.stderr == true
  assert "time" in output.stderr == true
}

test test_private_proc_effects_distinguish_captured_mutation_from_host_effects { |ctx|
  let output = test.run_xsh(
    ctx,
    """
var count = 0
proc caller() [] -> Int { increment() }
proc increment() -> Int {
  count = count + 1
  count
}
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """1
"""
}

test test_private_proc_effects_propagate_host_requirement_through_recursion { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [time] -> Int { second(1) }
proc first(value: Int) -> Int {
  if value == 0 {
    let _ = time.now()
    return 42
  }
  second(value - 1)
}
proc second(value: Int) -> Int { first(value) }
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """42
"""
}

test test_private_proc_effects_preserve_authored_public_upper_bound { |ctx|
  let output = test.run_xsh(
    ctx,
    """
##! Public effect boundary.
## Retains its written host capability promise even when unused.
export proc published() [time] -> Int { 42 }
proc caller() [] -> Int { published() }
""",
  )?
  assert output.status != 0
  assert "check.effect-violation" in output.stderr == true
  assert "time" in output.stderr == true
}

test test_private_proc_effects_preserve_authored_stream_upper_bound { |ctx|
  let output = test.run_xsh(
    ctx,
    """
stream values() [time] -> Stream[Int] { yield 42 }
proc caller() [] -> Stream[Int] { values() }
""",
  )?
  assert output.status != 0
  assert "check.effect-violation" in output.stderr == true
  assert "time" in output.stderr == true
}

test test_private_proc_effects_capture_transitive_plain_return_failure { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [] -> Int { locally_caught() }
proc locally_caught() -> Int {
  let outcome = try { failing() }
  if outcome is Err(_) { return 42 }
  0
}
proc failing() -> Int {
  assert false, "captured"
  0
}
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """42
"""
}

test test_private_proc_effects_include_explicit_result_propagation { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller() [error] -> Int { parsed() }
proc parsed() -> Int { "42".parse_int()? }
print \${caller()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """42
"""
}

test test_private_proc_effects_keep_erased_pure_calls_unknown { |ctx|
  let output = test.run_xsh(
    ctx,
    """
proc caller(callback: Pure) [] -> Int { forwarding(callback) }
proc forwarding(callback: Pure) -> Int {
  let _ = callback.call()
  42
}
""",
  )?
  assert output.status != 0
  assert "check.effect-violation" in output.stderr == true
  assert "forwarding -> Pure.call" in output.stderr == true
}

test test_private_proc_effects_include_typed_method_requirements { |ctx|
  let accepted = test.run_xsh(
    ctx,
    """
proc caller(file: Path) [fs, error] -> Str { reader(file) }
proc reader(file: Path) -> Str { file.read_text()? }
""",
  )?
  assert accepted.status == 0
  let rejected = test.run_xsh(
    ctx,
    """
proc caller(file: Path) [] -> Str { reader(file) }
proc reader(file: Path) -> Str { file.read_text()? }
""",
  )?
  assert rejected.status != 0
  assert "check.effect-violation" in rejected.stderr == true
  assert "fs" in rejected.stderr == true
}

test test_private_proc_effects_include_executed_stage_body_requirements { |ctx|
  let accepted = test.run_xsh(
    ctx,
    """
proc caller() [time] -> List[Int] { projected() }
proc projected() -> List[Int] {
  [42] |> map { |value|
    let _ = time.now()
    value
  } |> collect
}
""",
  )?
  assert accepted.status == 0
  let rejected = test.run_xsh(
    ctx,
    """
proc caller() [] -> List[Int] { projected() }
proc projected() -> List[Int] {
  [42] |> map { |value|
    let _ = time.now()
    value
  } |> collect
}
""",
  )?
  assert rejected.status != 0
  assert "check.effect-violation" in rejected.stderr == true
  assert "time" in rejected.stderr == true
}

test test_private_proc_effects_infer_module_call_and_propagation_requirements { |ctx|
  let declaration = r"""type Manifest = {name: Str}
proc read_manifest(file: Path) -> Result[Manifest] {
  json.read(file)?.require(Manifest)?
}
"""
  let accepted = test.run_xsh(
    ctx,
    declaration + """proc caller(file: Path) [fs, error] -> Result[Manifest] { read_manifest(file) }
""",
  )?
  assert accepted.status == 0
  for {declared, missing} in [{declared: "error", missing: "fs"}, {declared: "fs", missing: "error"}] {
    let caller = "proc caller(file: Path) [" + declared + """] -> Result[Manifest] { read_manifest(file) }
"""
    let rejected = test.run_xsh(ctx, declaration + caller)?
    assert rejected.status != 0
    assert "check.effect-violation" in rejected.stderr == true
    assert f"effect `${missing}` required by `read_manifest`" in rejected.stderr == true
  }
}

test test_private_proc_effects_leave_missing_public_and_stream_clauses_unrestricted { |ctx|
  for source in [
    """export proc published() -> Int { 42 }
proc caller() [] -> Int { published() }
""",
    """stream values() -> Stream[Int] { yield 42 }
proc caller() [] -> Stream[Int] { values() }
""",
  ] {
    let rejected = test.run_xsh(ctx, source)?
    assert rejected.status != 0
    assert "check.effect-violation" in rejected.stderr == true
    assert "unknown or unrestricted effect contract" in rejected.stderr == true
  }

  let accepted = test.run_xsh(
    ctx,
    """
export proc published() [] -> Int { helper() }
proc helper() -> Int { 42 }
proc caller() [] -> Int { published() }
print \${caller()}
""",
  )?
  assert accepted.status == 0
  assert accepted.stdout == """42
"""
}
