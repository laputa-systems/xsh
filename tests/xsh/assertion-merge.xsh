test test_assertion_forms_share_nominal_capture_inference { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure checked(failure: AssertionError) -> AssertionError { failure }
pure captured() -> Result[Unit, AssertionError] {
  try { assert false, "private capture" }
}
let bare = try { assert false; let _ = 0 }
let explicit = try { assert false, "explicit context" }
let chain = try { assert 3 < 2 < 1; let _ = 0 }
for result in [bare, explicit, chain, captured()] {
  match result {
    Err(failure) => {
      let nominal = checked(failure)
      print $nominal.message
    }
    Ok(_) => print "unexpected"
  }
}
""",
  )?
  assert output.success, output.stderr
  assert "false" in output.stdout
  assert "explicit context" in output.stdout
  assert "3 < 2" in output.stdout
  assert "private capture" in output.stdout
  assert "unexpected" not in output.stdout, output.stdout
}

test test_assertion_nominal_filter_runs_attempt_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""var attempts = 0
proc attempt() -> Int { attempts += 1; attempts }
proc cleanup() [io] -> Unit { print "cleaned" }
let result: Result[Unit, AssertionError] = retry [0ms] on (AssertionError) {
  defer cleanup()
  assert attempt() == 2, "retry assertion"
}
match result { Ok(_) => print "done"; Err(_) => print "unexpected" }
print $attempts
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """cleaned
cleaned
done
2
"""
}

test test_assertion_message_failure_keeps_its_nominal_type { |ctx|
  let output = test.run_script(
    ctx,
    r"""error MessageError = Failed(message: Str)
proc message() [error, io] -> Result[Str, MessageError] {
  print "message"
  Err(MessageError.Failed(message: "message failure"))
}
proc cleanup() [io] { print "cleaned" }
let success: Result[Unit, Error] = try { assert true, message()? }
let failure: Result[Unit, Error] = try {
  defer cleanup()
  assert false, message()?
}
match success { Ok(_) => print "passed"; Err(_) => print "unexpected" }
match failure {
  Err(MessageError.Failed {message}) => print $message
  Err(AssertionError.Failed {message}) => print f"unexpected assertion: {message}"
  _ => print "unexpected"
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """message
cleaned
passed
message failure
"""
}

test test_membership_assertions_preserve_typed_map_key_domains { |ctx|
  let output = test.run_script(
    ctx,
    r"""let integers: Map[Int, Str] = {[1]: "one"}
let flags: Map[Bool, Int] = {[true]: 1}
let delays: Map[Duration, Int] = {[3ms]: 1}
let paths: Map[Path, Int] = {[p"src"]: 1}
let unsigned: Map[UInt, Str] = {[1]: "one"}
assert 1 in integers
assert true in flags, "Bool key"
assert 3ms in delays
assert p"src" in paths, "Path key"
assert 1 in unsigned
let fields = {present: null}
let erased: Record = fields
assert "present" in fields
assert "present" in erased
assert "absent" not in erased
print "checked"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """checked
"""
  for statement in ["let _ = \"bad\" in values", "assert \"bad\" in values, \"key\"", "values.has(\"bad\")"] {
    let invalid = test.run_script(
      ctx,
      """let values: Map[Int, Str] = {[1]: "one"}
""" + statement + "\n",
    )?
    assert invalid.status == 2, invalid.stderr
    assert "check.type-mismatch" in invalid.stderr
    assert "Int" in invalid.stderr
  }

  let removed = test.run_script(
    ctx,
    """let values: Map[Int, Str] = {[1]: "one"}
values.has(1)
""",
  )?
  assert removed.status == 2, removed.stderr
  assert "check.removed-membership" in removed.stderr
  assert "check.type-mismatch" not in removed.stderr, removed.stderr
}

test test_membership_assertions_accept_checked_module_exports { |ctx|
  fp"{ctx.temp_root}/membership_merge.xsh".write("""##! Provides a checked export.
## A public field.
export let present = 1
""")?
  let output = test.run_script(
    ctx,
    r"""use membership_merge
assert "present" in membership_merge.keys()
assert "absent" not in membership_merge.keys(), "module export absence"
print "checked"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """checked
"""
}
