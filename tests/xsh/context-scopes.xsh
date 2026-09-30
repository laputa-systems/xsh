test test_cd_value_scope_consumes_tail_and_restores_context [fs, env, error] { |ctx|
  let root = test.temp_dir(ctx)?
  let original = fs.cwd()?
  let inside = cd (root) { fs.cwd()? }?
  inside == root
  fs.cwd()? == original
  let nested = cd (root) { Ok(7) }?
  nested == Ok(7)
  let stored = env ({XSH_SCOPE_NESTED_RESULT: "inner"}) { error.fail("stored data") }?
  stored is Err(_)
  let predicate = cd (root) { false }?
  ! predicate
}

test test_env_value_scope_accepts_typed_overlays_and_restores [env, error] {
  let original = env.get_or("XSH_VALUE_SCOPE", "absent")?
  let selected = env ({XSH_VALUE_SCOPE: "inner", XSH_SCOPE_NUMBER: 7}) {
    env.get("XSH_SCOPE_NUMBER")? == "7"
    env.get("XSH_VALUE_SCOPE")?
  }?
  selected == "inner"
  env.get_or("XSH_VALUE_SCOPE", "absent")? == original
  let overlay: Map[Str, Str] = {["XSH_VALUE_SCOPE"]: "map value"}
  (env (overlay) { env.get("XSH_VALUE_SCOPE")? }?) == "map value"
}

proc scope_body_failure(root: Path) [env, error] -> Result[Int] {
  let _ = cd (root) {
    error.fail("scope body failed")?
    1
  }
  Ok(99)
}

test test_scope_body_propagation_reaches_outer_function_and_restores [fs, env, error] { |ctx|
  let root = test.temp_dir(ctx)?
  let original = fs.cwd()?
  scope_body_failure(root) is Err(_)
  fs.cwd()? == original
}

test test_scope_defers_run_before_environment_restoration [env, error] {
  let original = env.get_or("XSH_VALUE_SCOPE", "absent")?
  var observed = ""
  let result = env ({XSH_VALUE_SCOPE: "deferred"}) {
    defer { observed = env.get("XSH_VALUE_SCOPE")? }
    false
  }?
  ! result
  observed == "deferred"
  env.get_or("XSH_VALUE_SCOPE", "absent")? == original
}

proc scope_lexical_return() [env, error] -> Int {
  let _ = env ({XSH_SCOPE_RETURN: "inner"}) { return 17 }
  99
}

test test_scope_lexical_return_and_loop_transfers_restore [env, error] {
  let original = env.get_or("XSH_SCOPE_RETURN", "absent")?
  scope_lexical_return() == 17
  env.get_or("XSH_SCOPE_RETURN", "absent")? == original
  var attempts = 0
  while attempts < 2 {
    attempts += 1
    let _ = env ({XSH_SCOPE_RETURN: "loop"}) { continue }
    test.fail("continue must leave the enclosing loop")?
  }
  attempts == 2
  env.get_or("XSH_SCOPE_RETURN", "absent")? == original
  while true {
    let _ = env ({XSH_SCOPE_RETURN: "loop"}) { break }
    test.fail("break must leave the enclosing loop")?
  }
  env.get_or("XSH_SCOPE_RETURN", "absent")? == original
}

test test_scope_entry_failure_is_data_and_skips_body [fs, env, error] { |ctx|
  let root = test.temp_dir(ctx)?
  let missing = fp"${root}/missing"
  var entered = false
  let failure = cd (missing) { entered = true; 7 }
  failure is Err(_)
  ! entered
  let malformed: Map[Str, Str] = {["BAD=NAME"]: "value"}
  let invalid = env (malformed) { entered = true; 9 }
  invalid is Err(_)
  ! entered
}

test test_nested_scopes_restore_to_the_immediate_parent [env, error] {
  let original = env.get_or("XSH_SCOPE_NESTED", "absent")?
  let selected = env ({XSH_SCOPE_NESTED: "outer"}) {
    (env ({XSH_SCOPE_NESTED: "inner"}) { env.get("XSH_SCOPE_NESTED")? }?) == "inner"
    env.get("XSH_SCOPE_NESTED")?
  }?
  selected == "outer"
  env.get_or("XSH_SCOPE_NESTED", "absent")? == original
}

test test_scope_body_error_is_caught_only_by_the_outer_capture [env, error] {
  let original = env.get_or("XSH_SCOPE_CAPTURE", "absent")?
  let failure = try {
    let _ = env ({XSH_SCOPE_CAPTURE: "inner"}) { error.fail("transparent")?; 7 }
    99
  }
  failure is Err(_)
  env.get_or("XSH_SCOPE_CAPTURE", "absent")? == original
}

test test_scope_input_and_scalar_fields_evaluate_once_in_order [env, error] {
  var sequence = 0
  let value = env ({FIRST: { sequence = sequence * 10 + 1; "first" }, SECOND: { sequence = sequence * 10 + 2; 2 }}) {
    sequence = sequence * 10 + 3
    env.get("SECOND")?
  }?
  sequence == 123
  value == "2"
}

test test_scope_rejects_null_overlay_values_and_escaping_producers [error] { |ctx|
  let null_value = test.run_script(ctx, "let value = env ({X: null}) { 7 }\n")?
  ! null_value.success
  "check.env-value" in null_value.stderr
  let integer_keys = test.run_script(ctx, "let overlay: Map[Int, Str] = {[7]: \"value\"}\nlet value = env (overlay) { 7 }\n")?
  ! integer_keys.success
  "check.context-scope-input" in integer_keys.stderr
  let escaping = test.run_script(ctx, "stream rows() [] -> Stream[Int] { yield 1 }\nlet value = env ({X: \"inner\"}) { rows() }\n")?
  ! escaping.success
  "check.context-scope-escape" in escaping.stderr
  let assigned = test.run_script(ctx, "stream rows() [] -> Stream[Int] { yield 1 }\nvar output: Any = null\nlet ignored = env ({X: \"inner\"}) { output = rows(); 7 }\n")?
  ! assigned.success
  "check.context-scope-escape" in assigned.stderr
  let returned = test.run_script(ctx, "stream rows() [] -> Stream[Int] { yield 1 }\nproc escaping() [env, error] -> Stream[Int] { return env ({X: \"inner\"}) { return rows() }? }\nescaping()\n")?
  ! returned.success
  "check.context-scope-escape" in returned.stderr
}

test test_suspended_producer_context_is_private_across_pulls_and_delegation [error] { |ctx|
  let output = test.run_script(ctx, r"""stream context_rows() [env, error] -> Stream[Str] {
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
""")?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  output.stdout == "producer consumer\nproducer consumer\nproducer consumer\nproducer consumer\ndelegator consumer\n"
}

test test_cancelled_producer_defers_see_its_context_before_restoration [error] { |ctx|
  let output = test.run_script(ctx, r"""stream rows() [env, error] -> Stream[Str] {
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
""")?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  output.stdout == "producer consumer\nproducer\nconsumer\n"
}

proc scope_tail_value() [env, error] -> Result[Int] {
  env ({XSH_SCOPE_TAIL: "inner"}) { 17 }
}

proc scope_statement_failure() [env, error] {
  env ({XSH_SCOPE_TAIL: "inner"}) { error.fail("statement failure") }
}

test test_scope_function_tails_consume_declared_values_and_statement_results_propagate [env, error] {
  let original = env.get_or("XSH_SCOPE_TAIL", "absent")?
  scope_tail_value()? == 17
  scope_statement_failure() is Err(_)
  env.get_or("XSH_SCOPE_TAIL", "absent")? == original
  let assertion = try { env ({XSH_SCOPE_TAIL: "inner"}) { false }?; 7 }
  assertion is Err(_)
  env.get_or("XSH_SCOPE_TAIL", "absent")? == original
}

test test_scope_tail_inside_an_inferred_value_block_consumes_false_as_data [env, error] {
  let nested = try { env ({XSH_SCOPE_VALUE: "inner"}) { false }? }
  nested == Ok(false)
  let value = { cd (p".") { false }? }
  ! value
}

test test_suspended_cwd_scope_is_private_and_cleanup_uses_its_directory [error] { |ctx|
  let output = test.run_script(ctx, r"""stream paths() [fs, env, error] -> Stream[Path] {
  let ignored = cd (p"/") {
    defer { print ${fs.cwd()?.display()} }
    yield fs.cwd()?
    yield fs.cwd()?
    7
  }
}
let original = fs.cwd()?
for value in paths() { print ${value.display()} ${fs.cwd()? == original} }
print ${fs.cwd()? == original}
for value in paths() { print ${value.display()} ${fs.cwd()? == original}; break }
print ${fs.cwd()? == original}
""")?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  output.stdout == "/ true\n/ true\n/\ntrue\n/ true\n/\ntrue\n"
}

test test_scope_cleanup_failure_preserves_primary_error_and_restores [env, error] { |ctx|
  let output = test.run_script(ctx, r"""env ({XSH_SCOPE_CLEANUP: "outer"}) {
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
""")?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  output.stdout == "primary body\ninner outer\n"
  "secondary cleanup" in output.stderr
}

test test_scope_rejects_producers_hidden_in_error_causes [error] { |ctx|
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
    let output = test.run_script(ctx, declarations + body)?
    output.status == 3
    "context-scope-escape" in output.stderr
    output.stdout == ""
  }
}

test test_scope_rejects_producers_hidden_in_process_error_causes [error] { |ctx|
  let output = test.run_script(ctx, r"""error Inner = Failed(resource: Stream[Int])
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
""")?
  output.status == 3
  "context-scope-escape" in output.stderr
  output.stdout == ""
}

test test_scope_preserves_scalar_error_causes_as_data [error] { |ctx|
  let output = test.run_script(ctx, r"""error Outer = Failed(message: Str)
error Inner = Failed(message: Str)
let escaped = env ({X: "inner"}) {
  let value: Result[Unit, Outer] = Err(Outer.Failed(message: "outer"), cause: Inner.Failed(message: "inner"))
  value
}?
escaped?
""")?
  output.status == 3
  "Outer.Failed" in output.stderr
  "Inner.Failed" in output.stderr
  let cause_retained = "context-scope-escape" not in output.stderr
  let failure_details = output.stderr
  assert cause_retained, failure_details
}
