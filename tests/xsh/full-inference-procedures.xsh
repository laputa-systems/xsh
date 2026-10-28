proc full_inference_procedure_modules(ctx: TestContext) [fs, error] -> Result[Path] {
  let modules = test.temp_dir(ctx, name: "full-inference-procedure-modules")?
  let fixture = fp"{ctx.file.parent()}/../fixtures/modules/full-inference-procedures/wrappers.xsh"
  fp"{modules}/wrappers.xsh".write(fixture.read_text()?)?
  modules
}

test test_full_inference_null_data_returns_keep_exact_shapes { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure only_null() { null }
pure null_data() { {raw: null, items: [null], outcome: Ok(null)} }
proc null_task() { null }
proc data_task() { null_data() }
pure optional(flag: Bool) { if flag { null } else { "present" } }
let exact: Null = only_null()
let absent: Str? = only_null()
let data = null_data()
let task_value: Null = null_task()?
let task_data = data_task()?
assert exact == null
assert absent == null
assert task_value == null
assert data.raw == null
assert data.items == [null]
assert data.outcome == Ok(null)
assert task_data == data
assert optional(true) == null
assert optional(false) == "present"
print "null data preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "null data preserved\n"
  test.expect(
    ctx,
    r"""type External = {raw: Str?}
pure only_null() { null }
let decoded: External = only_null()
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_private_and_imported_generic_procs_preserve_result_values { |ctx|
  let modules = full_inference_procedure_modules(ctx)?
  let output = test.run_xsh(
    ctx,
    r"""use wrappers as imported
proc private_wrap(value) { value }
let private_integer = private_wrap(42)?
let private_text = private_wrap("private")?
let imported_integer = imported.wrap(7)?
let imported_text = imported.wrap("imported")?
let nested: Result[Int] = imported.wrap(Ok(7))?
let error_payload: Result[Int, Str] = imported.wrap(Err("kept"))?
assert private_integer == 42
assert private_text == "private"
assert imported_integer == 7
assert imported_text == "imported"
assert nested == Ok(7)
assert error_payload == Err("kept")
print $private_integer
print $private_text
print $imported_integer
print $imported_text
print "nested result preserved"
print "error payload preserved"
""",
    env: {XSH_MODULE_PATH: modules},
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\nprivate\n7\nimported\nnested result preserved\nerror payload preserved\n"
}

test test_full_inference_finite_io_callback_passes_through_generic_proc { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc report(value: Str) [io] -> Str {
  print $value
  return value
}
proc apply(callback, value) [io] {
  callback(value)
}
let result = apply(report, "ground helper")?
assert result == "ground helper"
print $result
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "ground helper\nground helper\n"
}

test test_full_inference_explicit_empty_effect_bound_rejects_io_callback { |ctx|
  test.expect(
    ctx,
    r"""proc report(value: Str) [io] -> Str {
  print $value
  return value
}
proc deny(callback, value) [] {
  callback(value)
}
let result = deny(report, "forbidden")?
""",
    status: 2,
    stderr: ["[check.effect-violation]"],
  )?
}

test test_full_inference_proc_mutable_capture_remains_monomorphic { |ctx|
  test.expect(
    ctx,
    r"""var captured = [1]
proc remember(value) {
  captured = captured.push(value)
  value
}
let integer = remember(42)?
let text = remember("demo")?
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_proc_immutable_prefix_preserves_the_tail_type { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc copy(value) [] {
  let copied = value
  copied
}
proc copy_integer(value: Int) [] -> Result[Int] {
  let copied = value
  copied
}
let inferred_integer = copy(42)?
let grounded_integer = copy_integer(42)?
let inferred_text = copy("demo")?
assert inferred_integer == 42
assert grounded_integer == 42
assert inferred_text == "demo"
print $inferred_integer
print $grounded_integer
print $inferred_text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n42\ndemo\n"
}

test test_full_inference_proc_branches_keep_one_int_or_str_result { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc mark(label: Str) [io] -> Bool {
  print $label
  return true
}
proc choose(flag, left, right) [io] {
  if flag {
    let selected = mark("left branch")
    left
  } else {
    let selected = mark("right branch")
    right
  }
}
proc choose_integer(flag: Bool, left: Int, right: Int) [io] -> Result[Int] {
  if flag {
    let selected = mark("left branch")
    left
  } else {
    let selected = mark("right branch")
    right
  }
}
proc choose_text(flag: Bool, left: Str, right: Str) [io] -> Result[Str] {
  if flag {
    let selected = mark("left branch")
    left
  } else {
    let selected = mark("right branch")
    right
  }
}
let inferred_integer = choose(true, 42, 7)?
let grounded_integer = choose_integer(true, 42, 7)?
let inferred_text = choose(false, "left", "right")?
let grounded_text = choose_text(false, "left", "right")?
assert inferred_integer == 42
assert grounded_integer == 42
assert inferred_text == "right"
assert grounded_text == "right"
print $inferred_integer
print $grounded_integer
print $inferred_text
print $grounded_text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "left branch\nleft branch\nright branch\nright branch\n42\n42\nright\nright\n"
}

test test_full_inference_generic_statement_only_proc_returns_unit { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc announce(value) [io] {
  print $value
}
proc announce_integer(value: Int) [io] -> Result[Unit] {
  print $value
}
proc announce_text(value: Str) [io] -> Result[Unit] {
  print $value
}
let _ = announce(42)?
let _ = announce("demo")?
let _ = announce_integer(42)?
let _ = announce_text("demo")?
print "complete"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n42\ndemo\ncomplete\n"
}

test test_full_inference_inferred_effect_callback_passes_through_an_inferred_proc { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc greet() { print "hello" }
proc relay(callback) { callback() }
proc run_relay() { relay(greet)? }
proc nested() { run_relay()? }
nested()?
let kept = run_relay
print "done"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "hello\ndone\n"
}

test test_full_inference_inferred_callback_effects_reach_the_caller_bound { |ctx|
  test.expect(
    ctx,
    r"""proc stamp() { let moment = time.now(); print $moment }
proc relay(callback) { callback() }
proc bounded() [error] { relay(stamp)? }
bounded()?
""",
    status: 2,
    stderr: ["[check.effect-violation]", "`time`"],
  )?
  test.expect(
    ctx,
    r"""proc stamp() { let moment = time.now(); print $moment }
proc relay(callback) { callback() }
proc inferred() { relay(stamp)? }
proc bounded() [error] { inferred()? }
bounded()?
""",
    status: 2,
    stderr: ["[check.effect-violation]", "`time`"],
  )?
}

test test_full_inference_statement_callback_calls_propagate_through_a_generic_proc { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc show(value) { print $value }
proc twice(callback, value) { callback(value); callback(value) }
proc each_item(items, callback) { for item in items { callback(item) } }
proc guarded(callback, flag) { if flag { callback(1) }; 7 }
twice(show, 3)?
twice(show, "text")?
each_item([1, 2], show)?
let kept = guarded(show, true)?
assert kept == 7
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "3\n3\ntext\ntext\n1\n2\n1\n"
}

test test_full_inference_statement_callback_failure_joins_the_proc_error { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc fail(value) -> Result[Unit, Str] { return Err("failed: " + value) }
proc attempt(callback, value) { callback(value); 1 }
match attempt(fail, "now") {
  Err(message) => { print $message }
  Ok(_) => { print "unexpected" }
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "failed: now\n"
}

test test_full_inference_statement_callback_keeps_discard_and_effect_rules { |ctx|
  test.expect(
    ctx,
    "proc relay(callback) { callback(); 1 }\nproc value() { 5 }\nrelay(value)?\n",
    status: 2,
    stderr: ["[check.callable-mismatch]", "Unit"],
  )?
  test.expect(
    ctx,
    "proc relay(callback) [] { callback(); 1 }\n",
    status: 2,
    stderr: ["[check.effect-violation]", "`error`"],
  )?
  test.expect(
    ctx,
    "proc stamp() { let moment = time.now(); print $moment }\nproc relay(callback) { callback(); 1 }\nproc bounded() [error] { relay(stamp)? }\n",
    status: 2,
    stderr: ["[check.effect-violation]", "`time`"],
  )?
}

test test_full_inference_pure_statement_callback_requires_a_unit_result { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure ignore(value) -> Unit { let _ = value }
pure sweep(items, callback) { for item in items { callback(item) }; items.len() }
pure pair(callback, value) { callback(value); callback(value); 2 }
assert sweep([1, 2, 3], ignore) == 3
assert pair(ignore, "text") == 2
print "swept"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "swept\n"
  test.expect(
    ctx,
    "pure sweep(items, callback) { for item in items { callback(item) }; 1 }\npure keep(value) { value }\nlet rejected = sweep([1], keep)\n",
    status: 2,
    stderr: ["[check.callable-mismatch]", "Unit"],
  )?
}

test test_full_inference_authored_ok_and_err_join_in_a_generic_proc { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc branch(flag, value) { if flag { Ok(value) } else { Err("declined") } }
proc early(flag, value) {
  if flag { return Err("early") }
  return Ok(value)
}
proc counted(flag, value) {
  if flag { return Ok(value) }
  Err(7)
}
match branch(true, 1) { Ok(value) => { print $value }
  Err(error) => { print $error.message } }
match branch(false, "text") { Ok(value) => { print $value }
  Err(error) => { print $error.message } }
match early(true, 1.5) { Ok(value) => { print $value }
  Err(error) => { print $error.message } }
match early(false, 1.5) { Ok(value) => { print $value }
  Err(error) => { print $error.message } }
match counted(false, "kept") { Ok(value) => { print $value }
  Err(error) => { print $error.message } }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "1\ndeclined\nearly\n1.5\n7\n"
  test.expect(
    ctx,
    "proc branch(flag, value) -> Result[Int, Error] { if flag { Ok(1) } else { Err(\"declined\") } }\n",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_only_proc_completions_convert_error_payloads { |ctx|
  for source in [
    "pure branch(flag, value: Int) { if flag { Ok(value) } else { Err(\"declined\") } }\nlet outcome = branch(false, 1)\n",
    "pure branch(flag, value) { if flag { Ok(value) } else { Err(\"declined\") } }\nlet outcome = branch(false, 1)\n",
    "pure branch(flag, value: Int) { if flag { Ok(value) } else { Err(7) } }\nlet outcome = branch(false, 1)\n",
  ] {
    test.expect(ctx, source, status: 2, stderr: ["[check.type-mismatch]"])?
  }
  test.expect(
    ctx,
    "proc local(flag) { let outcome = if flag { Ok(1) } else { Err(\"declined\") }\n print \"unreachable\" }\nlocal(true)?\n",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
  let output = test.run_xsh(
    ctx,
    r"""error Failure = Lost(message: Str)
pure sorted(flag, value: Int) { if flag { Ok(value) } else { Err(Failure.Lost("lost")) } }
proc converted(flag, value) { if flag { Ok(value) } else { Err("declined") } }
match sorted(false, 1) { Ok(value) => { print $value }
  Err(error) => { print $error.message } }
match converted(false, 1) { Ok(value) => { print $value }
  Err(error) => { print $error.message } }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "lost\ndeclined\n"
}

test test_full_inference_ground_proc_returns_a_generic_callee_application_result { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc relay(callback) {
  let value = callback("input")?
  value
}
proc echo(text: Str) -> Result[Str] { text }
proc ground() { relay(echo) }
let result = ground()?
assert result == "input"
print $result
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "input\n"
}

test test_full_inference_pure_effect_rejection_reports_one_diagnostic { |ctx|
  let cases = [
    {name: "inferred proc", source: "proc log(value: Str) { print $value }\npure relay(value: Str) { log(value) }\n"},
    {name: "inferred proc from a generic caller", source: "proc log(value: Str) { print $value }\npure relay(value) { log(value) }\n"},
    {name: "generic inferred proc", source: "proc log(value) { print $value }\npure relay(value: Str) { log(value) }\n"},
    {name: "inferred stream", source: "stream numbers(items: List[Int]) { for item in items { yield item } }\npure relay(items: List[Int]) { numbers(items) }\n"},
    {name: "alias of a proc", source: "proc log(value: Str) { print $value }\nlet alias = log\npure relay(value: Str) { alias(value) }\n"},
  ]
  for {name, source} in cases {
    let output = test.run_xsh(ctx, source)?
    assert output.success == false, name
    assert "err[check.pure-effect]" in output.stderr, f"{name}: {output.stderr}"
    assert output.stderr.split("err[").len() == 2, f"{name}: {output.stderr}"
  }
}

test test_full_inference_generic_proc_may_exit_the_script { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc require_name(kind, name: Str) [io, process] {
  if name == "" {
    eprint f"missing name for {kind}"
    exit 2
  }
  print f"{kind}: {name}"
}
proc finish(kind) [io, process] {
  print f"finishing {kind}"
  exit 3
}
require_name("user", "root")
require_name(7, "ok")
finish("last")
print "not reached"
""",
  )?
  assert output.status == 3, output.stderr
  assert output.stdout == "user: root\n7: ok\nfinishing last\n", output.stdout
}

test test_full_inference_generic_proc_exit_reports_its_status_from_a_branch { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc require_name(kind, name: Str) [io, process] {
  if name == "" {
    eprint f"missing name for {kind}"
    exit 2
  }
  print f"{kind}: {name}"
}
require_name("user", "root")
require_name(7, "")
print "not reached"
""",
  )?
  assert output.status == 2, output.stderr
  assert output.stdout == "user: root\n", output.stdout
  assert "missing name for 7" in output.stderr, output.stderr
}

test test_full_inference_generic_proc_exits_from_a_branch_or_match_arm_tail { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc report(kind, name: Str) [io, process] {
  if name == "" { exit 4 } else { print f"{kind}: {name}" }
}
proc load(kind, name: Str) [fs, io, process] {
  match fp"{name}".read_text() {
    Ok(text) => { print f"{kind}: loaded" }
    Err(failure) => { exit 5 }
  }
}
report("first", "ready")
load("second", "/etc/hostname")
load("third", "/nonexistent-xsh-path")
print "not reached"
""",
  )?
  assert output.status == 5, output.stderr
  assert output.stdout == "first: ready\nsecond: loaded\n", output.stdout
}

test test_full_inference_proc_ending_in_a_returning_loop_has_no_unit_completion { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc spin(limit: Int) {
  var count = 0
  loop {
    count += 1
    if count > limit { return count }
  }
}
proc scan(limit: Int) {
  var at = 0
  while true {
    at += 2
    if at > limit { return at }
  }
}
let first = spin(3)?
let second = scan(5)?
print $first
print $second
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "4\n6\n"
}

test test_full_inference_proc_with_a_breaking_loop_still_completes_with_unit { |ctx|
  test.expect(
    ctx,
    r"""proc spin(limit: Int) {
  var count = 0
  loop {
    count += 1
    if count > limit { return count }
    break when count > 10
  }
}
let first = spin(3)?
""",
    status: 2,
    stderr: ["[check.type-mismatch]", "completions of proc `spin`"],
  )?
}
