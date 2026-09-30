test test_assertion_forms_share_nominal_capture_inference [error] { |ctx|
  let output = test.run_script(ctx, r"""pure checked(failure: AssertionError) -> AssertionError { failure }
pure captured() -> Result[Unit, AssertionError] {
  try { assert false, "private capture" }
}
let bare = try { false; let _ = 0 }
let explicit = try { assert false, "explicit context" }
let chain = try { 3 < 2 < 1; let _ = 0 }
for result in [bare, explicit, chain, captured()] {
  match result {
    Err(failure) => {
      let nominal = checked(failure)
      print $nominal.message
    }
    Ok(_) => print "unexpected"
  }
}
""")?
  assert output.success, output.stderr
  "false" in output.stdout
  "explicit context" in output.stdout
  "3 < 2" in output.stdout
  "private capture" in output.stdout
  assert "unexpected" not in output.stdout, output.stdout
}

test test_assertion_nominal_filter_runs_attempt_cleanup [error] { |ctx|
  let output = test.run_script(ctx, r"""var attempts = 0
proc attempt() -> Int { attempts += 1; attempts }
proc cleanup() [io] -> Unit { print "cleaned" }
let result: Result[Unit, AssertionError] = retry [0ms] on (AssertionError) {
  defer cleanup()
  assert attempt() == 2, "retry assertion"
}
match result { Ok(_) => print "done"; Err(_) => print "unexpected" }
print $attempts
""")?
  assert output.success, output.stderr
  output.stdout == "cleaned\ncleaned\ndone\n2\n"
}

test test_assertion_message_failure_keeps_its_nominal_type [error] { |ctx|
  let output = test.run_script(ctx, r"""error MessageError = Failed(message: Str)
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
  Err(AssertionError.Failed {message}) => print f"unexpected assertion: $message"
  _ => print "unexpected"
}
""")?
  assert output.success, output.stderr
  output.stdout == "message\ncleaned\npassed\nmessage failure\n"
}

test test_membership_assertions_preserve_typed_map_key_domains [error] { |ctx|
  let output = test.run_script(ctx, r"""let integers: Map[Int, Str] = {[1]: "one"}
let flags: Map[Bool, Int] = {[true]: 1}
let delays: Map[Duration, Int] = {[3ms]: 1}
let paths: Map[Path, Int] = {[p"src"]: 1}
let unsigned: Map[UInt, Str] = {[1]: "one"}
1 in integers
assert true in flags, "Bool key"
3ms in delays
assert p"src" in paths, "Path key"
1 in unsigned
let fields = {present: null}
let erased: Record = fields
"present" in fields
"present" in erased
"absent" not in erased
print "checked"
""")?
  assert output.success, output.stderr
  output.stdout == "checked\n"
  for statement in ["\"bad\" in values", "assert \"bad\" in values, \"key\"", "values.has(\"bad\")"] {
    let invalid = test.run_script(ctx, "let values: Map[Int, Str] = {[1]: \"one\"}\n" + statement + "\n")?
    assert invalid.status == 2, invalid.stderr
    "check.type-mismatch" in invalid.stderr
    "Int" in invalid.stderr
  }
  let removed = test.run_script(ctx, "let values: Map[Int, Str] = {[1]: \"one\"}\nvalues.has(1)\n")?
  assert removed.status == 2, removed.stderr
  "check.removed-membership" in removed.stderr
  assert "check.type-mismatch" not in removed.stderr, removed.stderr
}

test test_membership_assertions_accept_checked_module_exports [fs, error] { |ctx|
  fp"${ctx.temp_root}/membership_merge.xsh".write("##! Provides a checked export.\n## A public field.\nexport let present = 1\n")?
  let output = test.run_script(ctx, r"""use membership_merge
"present" in membership_merge.keys()
assert "absent" not in membership_merge.keys(), "module export absence"
print "checked"
""")?
  assert output.success, output.stderr
  output.stdout == "checked\n"
}
