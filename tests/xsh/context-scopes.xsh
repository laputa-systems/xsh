test test_cd_value_scope_consumes_tail_and_restores_context { |ctx|
  let root = test.temp_dir(ctx)?
  let original = fs.cwd()?
  let inside = cd (root) {
    fs.cwd()?
  }?
  assert inside == root
  assert fs.cwd()? == original
  let nested = cd (root) {
    Ok(7)
  }?
  assert nested == Ok(7)
  let stored = env ({XSH_SCOPE_NESTED_RESULT: "inner"}) {
    error.fail("stored data")
  }?
  assert stored is Err(_)
  let predicate = cd (root) {
    false
  }?
  assert ! predicate
}

test test_scope_command_capture_tails_keep_values_and_restore_context { |ctx|
  let output = test.expect(
    ctx,
    r"""
let original = fs.cwd()?
let revision: Str = cd (p".") { run.text sh -c "printf revision" ? }?
assert revision == "revision"
let inferred = cd (p".") { run.text sh -c "printf inferred" ? }?
assert inferred == "inferred"
let payload: Bytes = env ({XSH_CAPTURE_TAIL: "bytes"}) { run.bytes sh -c "printf bytes" ? }?
assert payload == b"bytes"
assert fs.cwd()? == original
let nested: Result[Str, ProcessError] = cd (p".") { run.text sh -c "printf nested" }?
assert nested? == "nested"
let capture = env ({XSH_CAPTURE_TAIL: "record"}) {
  run.capture --text sh -c "printf out; printf err >&2" ?
}?
assert capture.stdout == "out"
assert capture.stderr == "err"
let plain: Unit = cd (p".") { run sh -c "exit 0" }?
let best_effort: Unit = cd (p".") { run.status sh -c "exit 7" }?
let discarded: Unit = cd (p".") { run.text sh -c "printf discarded" ? }?
let observed: Any = discarded
assert observed is Unit
let _ = plain
let _ = best_effort
let failed = try { cd (p".") { run.text sh -c "exit 7" ? }? }
assert failed is Err(_)
assert fs.cwd()? == original
print "done"
""",
    status: 0,
  )?
  assert output.stdout == """done
"""
}

test test_env_value_scope_accepts_typed_overlays_and_restores {
  let original = env.get_or("XSH_VALUE_SCOPE", "absent")?
  let selected = env ({XSH_VALUE_SCOPE: "inner", XSH_SCOPE_NUMBER: 7}) {
    assert e"XSH_SCOPE_NUMBER"? == "7"
    e"XSH_VALUE_SCOPE"?
  }?
  assert selected == "inner"
  assert env.get_or("XSH_VALUE_SCOPE", "absent")? == original
  let overlay: Map[Str, Str] = {["XSH_VALUE_SCOPE"]: "map value"}
  assert env (overlay) {
    e"XSH_VALUE_SCOPE"?
  }? == "map value"
}

proc scope_body_failure(root: Path) [env, error] -> Result[Int] {
  let _ = cd (root) {
    error.fail("scope body failed")
    1
  }
  Ok(99)
}

test test_scope_body_propagation_reaches_outer_function_and_restores { |ctx|
  let root = test.temp_dir(ctx)?
  let original = fs.cwd()?
  assert scope_body_failure(root) is Err(_)
  assert fs.cwd()? == original
}

test test_scope_defers_run_before_environment_restoration {
  let original = env.get_or("XSH_VALUE_SCOPE", "absent")?
  var observed = ""
  let result = env ({XSH_VALUE_SCOPE: "deferred"}) {
    defer {
      observed = e"XSH_VALUE_SCOPE"?
    }
    false
  }?
  assert ! result
  assert observed == "deferred"
  assert env.get_or("XSH_VALUE_SCOPE", "absent")? == original
}

proc scope_lexical_return() [env, error] -> Int {
  let _ = env ({XSH_SCOPE_RETURN: "inner"}) {
    return 17
  }
  99
}

test test_scope_lexical_return_and_loop_transfers_restore {
  let original = env.get_or("XSH_SCOPE_RETURN", "absent")?
  assert scope_lexical_return() == 17
  assert env.get_or("XSH_SCOPE_RETURN", "absent")? == original
  var attempts = 0
  while attempts < 2 {
    attempts += 1
    let _ = env ({XSH_SCOPE_RETURN: "loop"}) {
      continue
    }
    test.fail("continue must leave the enclosing loop")
  }

  assert attempts == 2
  assert env.get_or("XSH_SCOPE_RETURN", "absent")? == original
  while true {
    let _ = env ({XSH_SCOPE_RETURN: "loop"}) {
      break
    }
    test.fail("break must leave the enclosing loop")
  }

  assert env.get_or("XSH_SCOPE_RETURN", "absent")? == original
}

test test_scope_entry_failure_is_data_and_skips_body { |ctx|
  let root = test.temp_dir(ctx)?
  let missing = fp"{root}/missing"
  var entered = false
  let failure = cd (missing) {
    entered = true
    7
  }
  assert failure is Err(_)
  assert ! entered
  let malformed: Map[Str, Str] = {["BAD=NAME"]: "value"}
  let invalid = env (malformed) {
    entered = true
    9
  }
  assert invalid is Err(_)
  assert ! entered
}

test test_nested_scopes_restore_to_the_immediate_parent {
  let original = env.get_or("XSH_SCOPE_NESTED", "absent")?
  let selected = env ({XSH_SCOPE_NESTED: "outer"}) {
    assert env ({XSH_SCOPE_NESTED: "inner"}) {
      e"XSH_SCOPE_NESTED"?
    }? == "inner"
    e"XSH_SCOPE_NESTED"?
  }?
  assert selected == "outer"
  assert env.get_or("XSH_SCOPE_NESTED", "absent")? == original
}

test test_scope_body_error_is_caught_only_by_the_outer_capture {
  let original = env.get_or("XSH_SCOPE_CAPTURE", "absent")?
  let failure = try {
    let _ = env ({XSH_SCOPE_CAPTURE: "inner"}) {
      error.fail("transparent")
      7
    }
    99
  }
  assert failure is Err(_)
  assert env.get_or("XSH_SCOPE_CAPTURE", "absent")? == original
}

test test_scope_input_and_scalar_fields_evaluate_once_in_order {
  var sequence = 0
  let value = env ({
    FIRST: {
      sequence = sequence * 10 + 1
      "first"
    },
    SECOND: {
      sequence = sequence * 10 + 2
      2
    },
  }) {
    sequence = sequence * 10 + 3
    e"SECOND"?
  }?
  assert sequence == 123
  assert value == "2"
}

test test_scope_rejects_null_overlay_values_and_escaping_producers { |ctx|
  let null_value = test.run_script(
    ctx,
    """let value = env ({X: null}) { 7 }
""",
  )?
  assert ! null_value.success
  assert "check.env-value" in null_value.stderr
  let integer_keys = test.run_script(
    ctx,
    """let overlay: Map[Int, Str] = {[7]: "value"}
let value = env (overlay) { 7 }
""",
  )?
  assert ! integer_keys.success
  assert "check.context-scope-input" in integer_keys.stderr
  let escaping = test.run_script(
    ctx,
    """stream rows() [] -> Stream[Int] { yield 1 }
let value = env ({X: "inner"}) { rows() }
""",
  )?
  assert ! escaping.success
  assert "check.context-scope-escape" in escaping.stderr
  let captured = test.run_script(
    ctx,
    """let value = cd (p".") { run.stream --text sh -c "printf live" ? }
""",
  )?
  assert ! captured.success
  assert "check.context-scope-escape" in captured.stderr
  let assigned = test.run_script(
    ctx,
    """stream rows() [] -> Stream[Int] { yield 1 }
var output: Any = null
let ignored = env ({X: "inner"}) { output = rows(); 7 }
""",
  )?
  assert ! assigned.success
  assert "check.context-scope-escape" in assigned.stderr
  let returned = test.run_script(
    ctx,
    """stream rows() [] -> Stream[Int] { yield 1 }
proc escaping() [env, error] -> Stream[Int] { return env ({X: "inner"}) { return rows() }? }
let _ = escaping()
""",
  )?
  assert ! returned.success
  assert "check.context-scope-escape" in returned.stderr
}

test test_suspended_producer_context_is_private_across_pulls_and_delegation { |ctx|
  let output = test.run_script(
    ctx,
    r"""stream context_rows() [env, error] -> Stream[Str] {
  let ignored = env ({XSH_SCOPE_PRODUCER: "producer"}) {
    yield env.get("XSH_SCOPE_PRODUCER")?
    yield env.get("XSH_SCOPE_PRODUCER")?
    7
  }
}
stream delegated_context_rows() [env, error] -> Stream[Str] {
  let ignored = env ({XSH_SCOPE_PRODUCER: "delegator"}) {
    yield @ context_rows()
    yield env.get("XSH_SCOPE_PRODUCER")?
    9
  }
}
env ({XSH_SCOPE_PRODUCER: "consumer"}) {
  for value in context_rows() { print $value ${env.get("XSH_SCOPE_PRODUCER")?} }
  for value in delegated_context_rows() { print $value ${env.get("XSH_SCOPE_PRODUCER")?} }
}?
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """producer consumer
producer consumer
producer consumer
producer consumer
delegator consumer
"""
}

test test_cancelled_producer_defers_see_its_context_before_restoration { |ctx|
  let output = test.run_script(
    ctx,
    r"""stream rows() [env, error] -> Stream[Str] {
  let ignored = env ({XSH_SCOPE_PRODUCER: "producer"}) {
    defer { print ${env.get("XSH_SCOPE_PRODUCER")?} }
    yield env.get("XSH_SCOPE_PRODUCER")?
    yield "unreachable"
    7
  }
}
env ({XSH_SCOPE_PRODUCER: "consumer"}) {
  for value in rows() {
    print $value ${env.get("XSH_SCOPE_PRODUCER")?}
    break
  }
  print ${env.get("XSH_SCOPE_PRODUCER")?}
}?
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """producer consumer
producer
consumer
"""
}

proc scope_tail_value() [env, error] -> Result[Int] {
  env ({XSH_SCOPE_TAIL: "inner"}) {
    17
  }
}

proc scope_statement_failure() [env, error] {
  env ({XSH_SCOPE_TAIL: "inner"}) {
    error.fail("statement failure")
  }
}

test test_scope_function_tails_consume_declared_values_and_statement_results_propagate {
  let original = env.get_or("XSH_SCOPE_TAIL", "absent")?
  assert scope_tail_value()? == 17
  assert scope_statement_failure() is Err(_)
  assert env.get_or("XSH_SCOPE_TAIL", "absent")? == original
  let assertion = try {
    env ({XSH_SCOPE_TAIL: "inner"}) {
      assert false
    }
    7
  }
  assert assertion is Err(_)
  assert env.get_or("XSH_SCOPE_TAIL", "absent")? == original
}

test test_scope_tail_inside_an_inferred_value_block_consumes_false_as_data {
  let nested = try {
    env ({XSH_SCOPE_VALUE: "inner"}) {
      false
    }?
  }
  assert nested == Ok(false)
  let value = {
    cd (p".") {
      false
    }?
  }
  assert ! value
}

test test_suspended_cwd_scope_is_private_and_cleanup_uses_its_directory { |ctx|
  let output = test.run_script(
    ctx,
    r"""stream paths() [fs, env, error] -> Stream[Path] {
  let ignored = cd (p"/") {
    defer { print ${fs.cwd()?} }
    yield fs.cwd()?
    yield fs.cwd()?
    7
  }
}
let original = fs.cwd()?
for value in paths() { print ${value} ${fs.cwd()? == original} }
print ${fs.cwd()? == original}
for value in paths() { print ${value} ${fs.cwd()? == original}; break }
print ${fs.cwd()? == original}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """/ true
/ true
/
true
/ true
/
true
"""
}

test test_scope_cleanup_failure_preserves_primary_error_and_restores { |ctx|
  let output = test.run_script(
    ctx,
    r"""env ({XSH_SCOPE_CLEANUP: "outer"}) {
  var observed = ""
  let failure = try {
    let ignored = env ({XSH_SCOPE_CLEANUP: "inner"}) {
      defer { observed = env.get("XSH_SCOPE_CLEANUP")?; error.fail("secondary cleanup")? }
      error.fail("primary body")?
      7
    }
    99
  }
  match failure {
    Err(error) => print ${error.message}
    Ok(_) => print "unexpected success"
  }
  print $observed ${env.get("XSH_SCOPE_CLEANUP")?}
}?
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """primary body
inner outer
"""
  assert "secondary cleanup" in output.stderr
}

test test_scope_rejects_producers_hidden_in_error_causes { |ctx|
  let declarations = r"""error Inner = Failed(resource: Stream[Int])
error Outer = Failed(message: Str)
stream rows() [] -> Stream[Int] { yield 1 }
"""
  for body in [
    r"""let escaped = env ({X: "inner"}) {
  let value: Result[Unit, Outer] = Err(Outer.Failed(message: "outer"), cause: Inner.Failed(resource: rows()))
  value
}
print "escaped"
""",
    r"""let escaped = env ({X: "inner"}) {
  let inner: Result[Unit, Outer] = Err(Outer.Failed(message: "middle"), cause: Inner.Failed(resource: rows()))
  let attached = match inner { Err(failure) => failure; _ => Outer.Failed(message: "unreachable") }
  let alias = attached
  let value: Result[Unit, Outer] = Err(Outer.Failed(message: "outer"), cause: alias)
  value
}
print "escaped"
""",
    r"""var escaped: Any = null
let ignored = env ({X: "inner"}) {
  let value: Result[Unit, Outer] = Err(Outer.Failed(message: "outer"), cause: Inner.Failed(resource: rows()))
  let attached = match value { Err(failure) => failure; _ => Outer.Failed(message: "unreachable") }
  escaped = attached
  7
}
print "escaped"
""",
    r"""stream translated() [env, error] -> Stream[Outer] {
  let ignored = env ({X: "inner"}) {
    let value: Result[Unit, Outer] = Err(Outer.Failed(message: "outer"), cause: Inner.Failed(resource: rows()))
    let attached = match value { Err(failure) => failure; _ => Outer.Failed(message: "unreachable") }
    yield attached
    7
  }
}
for item in translated() { print "escaped" }
""",
  ] {
    let output = test.expect(ctx, declarations + body, status: 3, stderr: ["context-scope-escape"])?
    assert output.stdout == ""
  }
}

test test_scope_rejects_dynamic_escapes_through_fields_and_nested_bodies { |ctx|
  # Any hides the live cause from the checker, so the runtime owns these.
  let declarations = r"""error Inner = Failed(resource: Stream[Int])
error Outer = Failed(message: Str)
stream rows() [] -> Stream[Int] { yield 1 }
type Holder = {item: Any}
var holder: Holder = {item: null}
var escaped: Any = null
let ignored = env ({X: "inner"}) {
  let value: Result[Unit, Outer] = Err(Outer.Failed(message: "outer"), cause: Inner.Failed(resource: rows()))
  let attached = match value { Err(failure) => failure; _ => Outer.Failed(message: "unreachable") }
"""
  for escape in [
    "holder.item = attached",
    "let seen = [1] |> tee { |n| escaped = attached }",
    "let seen = [1] |> tee { |n| holder.item = attached }",
    "let retried = retry [] { escaped = attached; true }",
    "let retried = retry [] { let n = { holder.item = attached; 1 }; true }",
    "let seen = [1] |> tee { |n| let retried = retry [] { escaped = attached; true } }",
  ] {
    let output = test.run_script(
      ctx,
      declarations + "  " + escape + """\n  7
}
print "escaped"
""",
    )?
    let {status, stderr, stdout} = output
    assert status == 3, f"{escape}: {stderr}"
    assert "context-scope-escape" in stderr, f"{escape}: {stderr}"
    assert stdout == "", escape
  }
}

test test_scope_nested_bodies_may_assign_live_values_to_scope_locals { |ctx|
  let output = test.run_script(
    ctx,
    r"""error Inner = Failed(resource: Stream[Int])
error Outer = Failed(message: Str)
stream rows() [] -> Stream[Int] { yield 1 }
type Holder = {item: Any}
var outer = 0
let count = env ({X: "inner"}) {
  let value: Result[Unit, Outer] = Err(Outer.Failed(message: "outer"), cause: Inner.Failed(resource: rows()))
  let attached = match value { Err(failure) => failure; _ => Outer.Failed(message: "unreachable") }
  var local: Any = null
  var holder: Holder = {item: null}
  let seen = [1, 2] |> tee { |n|
    local = attached
    holder.item = attached
    outer = outer + n
  }
  let retried = retry [] { local = attached; holder.item = attached; true }
  outer
}?
print f"{count} {outer}"
""",
  )?
  let {success, stderr, stdout} = output
  assert success, stderr
  assert stdout == """3 3
"""
}

test test_scope_rejects_producers_hidden_in_process_error_causes { |ctx|
  let output = test.expect(
    ctx,
    r"""error Inner = Failed(resource: Stream[Int])
stream rows() [] -> Stream[Int] { yield 1 }
let original: Result[Unit, ProcessError] = try { run sh -c "exit 7" }
match original {
  Err(failure) => {
    let escaped = env ({X: "inner"}) {
      let value: Result[Unit, ProcessError] = Err(failure, cause: Inner.Failed(resource: rows()))
      value
    }
    print "escaped"
  }
  _ => print "unexpected success"
}
""",
    status: 3,
    stderr: ["context-scope-escape"],
  )?
  assert output.stdout == ""
}

test test_scope_preserves_scalar_error_causes_as_data { |ctx|
  let output = test.expect(
    ctx,
    r"""error Outer = Failed(message: Str)
error Inner = Failed(message: Str)
let escaped = env ({X: "inner"}) {
  let value: Result[Unit, Outer] = Err(Outer.Failed(message: "outer"), cause: Inner.Failed(message: "inner"))
  value
}?
escaped?
""",
    status: 3,
    stderr: ["Outer.Failed", "Inner.Failed"],
  )?
  let cause_retained = "context-scope-escape" not in output.stderr
  let failure_details = output.stderr
  assert cause_retained, failure_details
}

test test_env_assignment_double_block_form_is_removed_before_execution { |ctx|
  let rejected = test.run_script(
    ctx,
    """print unreachable
env { XSH_REMOVED_SCOPE = "release" } {
  print body
}
""",
  )?
  assert ! rejected.success
  assert rejected.stdout == ""
  assert "parse.env-scope-migration" in rejected.stderr
}

test test_cd_scope_accepts_a_bare_path_before_its_block { |ctx|
  let output = test.expect(
    ctx,
    r"""cd / {
  run pwd
}
let cd = 6
let tmp = 3
print ${cd / tmp}
""",
    status: 0,
  )?
  assert output.stdout == "/\n2\n"
}
