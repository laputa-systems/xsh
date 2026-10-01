type CallableCheck = {status: Status, stdout: Str, stderr: Str}

proc check_callable_source(ctx: TestContext, source: Str) [fs, process, error] -> Result[CallableCheck] {
  let file = test.temp_file(ctx, name: "callable-contract.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file ?
  let {status, stdout, stderr, ..} = checked
  {status, stdout, stderr}
}

test immutable_empty_aliases_generalize_in_independent_contexts [fs, process, error] { |ctx|
  for calls in ["paths(entries) + integers(alias)", "integers(alias) + paths(entries)"] {
    let checked = check_callable_source(ctx, f"""pure paths(values: List[Path]) -> Int { values.len() }
pure integers(values: List[Int]) -> Int { values.len() }
pure inspect() -> Int {
  let entries = []
  let alias = entries
  ${calls}
}
""")?
    assert checked.status.exited_with(0), checked.stderr
    checked.stdout == ""
  }
}

test mutable_aliases_and_captured_state_keep_one_type [fs, process, error] { |ctx|
  for source in [
    r"""proc paths(values: List[Path]) [] -> Unit {}
proc integers(values: List[Int]) [] -> Unit {}
proc inspect() [] -> Unit {
  var entries = []
  let alias = entries
  paths(entries)
  integers(alias)
}
""",
    r"""var retained: List[Int] = []
proc append(value) [] -> Unit { retained = retained.push(value) }
proc inspect() [] -> Unit { append(1); append("wrong") }
""",
  ] {
    let checked = check_callable_source(ctx, source)?
    assert checked.status.exited_with(2), checked.stderr
    assert "check.type-mismatch" in checked.stderr, checked.stderr
    assert "parse." not in checked.stderr, checked.stderr
  }
}

test transparent_generic_aliases_instantiate_without_use_order_training [fs, process, error] { |ctx|
  for bindings in [
    "let numeric: Int = alias(7)\n  let text: Str = alias(\"seven\")",
    "let text: Str = alias(\"seven\")\n  let numeric: Int = alias(7)",
  ] {
    let checked = check_callable_source(ctx, f"""pure identity(value) { value }
pure inspect() -> Int {
  let direct = identity
  let alias = direct
  ${bindings}
  numeric + text.count_chars()
}
""")?
    assert checked.status.exited_with(0), checked.stderr
    checked.stdout == ""
  }
}

test transparent_generic_alias_preserves_payload_relationship [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""pure identity(value) { value }
pure inspect() -> Int {
  let direct = identity
  let alias = direct
  let wrong: Int = alias("seven")
  wrong
}
""")?
  assert checked.status.exited_with(2), checked.stderr
  assert "check.type-mismatch" in checked.stderr, checked.stderr
  assert "expected Int, found Str" in checked.stderr, checked.stderr
  assert "parse." not in checked.stderr, checked.stderr
}

test computed_callables_keep_labels_defaults_and_container_signature [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""pure first(value: Int, offset: Int = 1) -> Int { value + offset }
pure second(value: Int, offset: Int = 2) -> Int { value + offset }
pure inspect(select: Bool) -> Int {
  let chosen = if select { (first) } else { (second) }
  let boxed = {call: chosen}
  let callbacks = [boxed.call]
  let explicit: Int = callbacks[0](offset: 4, value: 3)
  let defaulted: Int = callbacks[0](value: 3)
  explicit + defaulted
}
""")?
  assert checked.status.exited_with(0), checked.stderr
  checked.stdout == ""
}

test computed_callable_effects_keep_a_finite_union [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""proc clock(value: Int) [time] -> Int { let _ = time.now(); value }
proc setting(value: Int) [env] -> Int { let _ = env.get("UNREAD_SETTING"); value }
proc inspect(select: Bool) [time, env] -> Int {
  let chosen = if select { (clock) } else { (setting) }
  chosen(value: 3)
}
""")?
  assert checked.status.exited_with(0), checked.stderr
  checked.stdout == ""
}

test computed_callable_selection_does_not_execute_latent_effects [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""proc clock(value: Int) [time] -> Int { let _ = time.now(); value }
proc setting(value: Int) [env] -> Int { let _ = env.get("UNREAD_SETTING"); value }
proc inspect(select: Bool) [] -> Int {
  let chosen = if select { (clock) } else { (setting) }
  let boxed = {call: chosen}
  let _ = [boxed.call]
  3
}
""")?
  assert checked.status.exited_with(0), checked.stderr
  checked.stdout == ""
}

test computed_callable_finite_effect_union_respects_written_bound [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""proc clock(value: Int) [time] -> Int { let _ = time.now(); value }
proc setting(value: Int) [env] -> Int { let _ = env.get("UNREAD_SETTING"); value }
proc inspect(select: Bool) [time] -> Int {
  let chosen = if select { (clock) } else { (setting) }
  chosen(value: 3)
}
""")?
  assert checked.status.exited_with(2), checked.stderr
  assert "check.effect-violation" in checked.stderr, checked.stderr
  assert "env" in checked.stderr, checked.stderr
  assert "check.unresolved-call" not in checked.stderr, checked.stderr
  assert "check.call-target" not in checked.stderr, checked.stderr
  assert "unknown" not in checked.stderr, checked.stderr
  assert "unrestricted" not in checked.stderr, checked.stderr
  assert "parse." not in checked.stderr, checked.stderr
}

test computed_callable_incompatible_labels_require_a_contract [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""pure first(value: Int) -> Int { value }
pure second(item: Int) -> Int { item }
pure inspect(select: Bool) -> Int {
  let chosen = if select { (first) } else { (second) }
  chosen(value: 3)
}
""")?
  assert checked.status.exited_with(2), checked.stderr
  assert "check.type-mismatch" in checked.stderr, checked.stderr
  assert "check.unresolved-call" not in checked.stderr, checked.stderr
  assert "check.call-target" not in checked.stderr, checked.stderr
  assert "parse." not in checked.stderr, checked.stderr
}

test inferred_export_effects_are_definition_owned_across_client_order [fs, process, error] { |ctx|
  let root = test.temp_dir(ctx, name: "inferred-export-effects")?
  let library = fp"${root}/library.xsh"
  let constant_client = fp"${root}/constant-client.xsh"
  let clock_client = fp"${root}/clock-client.xsh"
  library.write_atomic(r"""##! Definition-owned callable contracts.
## Return a constant without host requirements.
export proc published() -> Int { 42 }
## Read the host clock.
export proc timed() -> Int { let _ = time.now(); 42 }
""")?
  constant_client.write_atomic("use library\nproc caller() [] -> Int { library.published() }\n")?
  clock_client.write_atomic("use library\nproc caller() [time] -> Int { library.timed() }\n")?
  for clients in [[constant_client, clock_client], [clock_client, constant_client]] {
    let first = clients[0]
    let second = clients[1]
    let checked = run.capture --text "xsht" check $first $second ?
    let {status, stdout, stderr, ..} = checked
    assert status.exited_with(0), stderr
    stdout == ""
  }
}

test inferred_mutable_callable_assignment_preserves_its_signature [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""pure first(value: Int) -> Int { value }
pure second(value: Str) -> Str { value }
pure inspect() -> Int {
  var current = first
  current = second
  1
}
""")?
  assert checked.status.exited_with(2), checked.stderr
  assert "check.type-mismatch" in checked.stderr, checked.stderr
  assert "check.unresolved-call" not in checked.stderr, checked.stderr
  assert "check.call-target" not in checked.stderr, checked.stderr
  assert "parse." not in checked.stderr, checked.stderr
}

test authored_pure_slots_keep_the_explicit_erased_boundary [fs, process, error] { |ctx|
  for source in [
    r"""pure first(value: Int) -> Int { value }
pure second(value: Str) -> Str { value }
pure inspect() -> Int {
  var current: Pure = first
  current = second
  1
}
""",
    r"""pure identity(value) { value }
pure inspect() -> Int {
  var current: Pure = identity
  let alias = current
  let _ = alias.call(7)
  let _ = alias.call("word")
  1
}
""",
  ] {
    let checked = check_callable_source(ctx, source)?
    assert checked.status.exited_with(0), checked.stderr
    checked.stdout == ""
  }
}

test mutable_generic_callable_aliases_share_one_instantiation [fs, process, error] { |ctx|
  for arguments in [["7", "\"word\""], ["\"word\"", "7"]] {
    let first = arguments[0]
    let second = arguments[1]
    let checked = check_callable_source(ctx, f"""pure identity(value) { value }
pure inspect() -> Int {
  var current = identity
  let alias = current
  let _ = alias.call(${first})
  let _ = alias.call(${second})
  1
}
""")?
    assert checked.status.exited_with(2), checked.stderr
    assert "check.type-mismatch" in checked.stderr, checked.stderr
    assert "check.unresolved-call" not in checked.stderr, checked.stderr
    assert "check.call-target" not in checked.stderr, checked.stderr
    assert "parse." not in checked.stderr, checked.stderr
  }
}

test computed_callable_call_method_preserves_its_parameter_type [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""pure first(value: Int) -> Int { value }
pure second(value: Int) -> Int { value }
pure inspect(select: Bool) -> Int {
  let chosen = if select { (first) } else { (second) }
  let alias = chosen
  let _ = alias.call(true)
  1
}
""")?
  assert checked.status.exited_with(2), checked.stderr
  assert "check.type-mismatch" in checked.stderr, checked.stderr
  assert "expected Int, found Bool" in checked.stderr, checked.stderr
  assert "check.unresolved-call" not in checked.stderr, checked.stderr
  assert "check.call-target" not in checked.stderr, checked.stderr
  assert "parse." not in checked.stderr, checked.stderr
}

test compatible_mutable_callable_assignment_keeps_parameter_and_result_types [fs, process, error] { |ctx|
  let checked = check_callable_source(ctx, r"""pure first(value: Int) -> Int { value }
pure second(value: Int) -> Int { value + 1 }
pure inspect() -> Int {
  var current = first
  let before: Int = current.call(7)
  current = second
  let alias = current
  let after: Int = alias.call(8)
  before + after
}
""")?
  assert checked.status.exited_with(0), checked.stderr
  checked.stdout == ""
}

test inert_callable_aggregates_generalize_without_use_order_training [fs, process, error] { |ctx|
  for storage in [["{callback: identity}", "boxed.callback"], ["[identity]", "boxed[0]"]] {
    let constructor = storage[0]
    let callback = storage[1]
    for bindings in [
      f"let numeric: Int = ${callback}(7)\n  let text: Str = ${callback}(\"word\")",
      f"let text: Str = ${callback}(\"word\")\n  let numeric: Int = ${callback}(7)",
    ] {
      let checked = check_callable_source(ctx, f"""pure identity(value) { value }
pure inspect() -> Int {
  let boxed = ${constructor}
  ${bindings}
  numeric + text.count_chars()
}
""")?
      assert checked.status.exited_with(0), checked.stderr
      checked.stdout == ""
    }
  }
}
