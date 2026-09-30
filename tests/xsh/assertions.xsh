test test_boolean_statement_failure_stops_script [error] { |ctx|
  let output = test.run_script(ctx, "1 == 2\nprint \"unreachable\"\n")?
  output.status == 3
  output.stdout == ""
  assert "1 == 2" in output.stderr, output.stderr
  assert "AssertionError" in output.stderr, output.stderr
}

test test_boolean_values_and_explicit_discards_remain_values [error] { |ctx|
  let output = test.run_script(ctx, """
pure predicate() -> Bool { false }
pure dynamic() -> Any { false }
proc wrapped() -> Result[Bool] { false }
proc wrapped_any() -> Result[Any] { false }
let _ = predicate()
let value = predicate()
let retried = retry [] { false }?
print f"\${value} \${dynamic()} \${wrapped()?} \${wrapped_any()?} \${retried}"
""")?
  assert output.success, output.stderr
  output.stdout == "false false false false false\n"
}

test test_unit_tail_and_non_tail_booleans_assert [error] { |ctx|
  let output = test.run_script(ctx, """
proc check() { 2 > 3 }
check()?
print "unreachable"
""")?
  output.status == 3
  output.stdout == ""
}

test test_membership_checks_map_keys_and_record_fields [error] {
  let empty: Map[Int?] = {}
  let mapping = empty.set("present", null)
  ("present" in mapping)
  ("absent" not in mapping)
  let fields = {present: null}
  ("present" in fields)
  ("absent" not in fields)
}

test test_assertion_single_evaluation_short_circuit_and_value_predicates [error] { |ctx|
  let output = test.run_script(ctx, """
var calls = 0
proc tick() -> Result[Int] { calls += 1; calls }
tick()? == 1
true or tick()? == 99
! (false and tick()? == 99)
calls == 1
let values = [1, 2, 3] |> where { false } |> collect()
values == []
let no = [1, 2] |> any { false }
let yes = [1, 2] |> all { true }
let not_all = [1, 2] |> all { false }
no == false
yes == true
not_all == false
print "done"
""")?
  assert output.success, output.stderr
  output.stdout == "done\n"
}

test test_assertion_nominal_error_handlers_and_retry [error] { |ctx|
  let output = test.run_script(ctx, """
pure fail() -> Result[Unit, AssertionError] { false }
match fail() {
  Err(AssertionError.Failed {message: message}) => print $message
  Ok(_) => print "unexpected"
}
var attempts = 0
proc tick() -> Result[Int] { attempts += 1; attempts }
let _ = retry [0ms, 0ms] {
  tick()? == 3
  let _ = attempts
}?
attempts == 3
""")?
  assert output.success, output.stderr
  assert "false" in output.stdout, output.stdout
  assert "unexpected" not in output.stdout, output.stdout
}

test test_assertion_defers_preserve_primary_failure [error] { |ctx|
  let output = test.run_script(ctx, """
proc cleanup(value: Str) { print $value }
proc broken_cleanup() { error.fail("cleanup failed")? }
proc fail() {
  defer cleanup("first")?
  defer broken_cleanup()?
  defer cleanup("last")?
  8 < 3
}
fail()?
print "unreachable"
""")?
  output.status == 3
  output.stdout == "last\nfirst\n"
  assert "AssertionError.Failed" in output.stderr, output.stderr
  assert "8 < 3" in output.stderr, output.stderr
}

test test_assertion_contexts_require_result_effect_and_compatible_error [error] { |ctx|
  for source in [
    "pure fail() -> Unit { false }\n",
    "proc fail() [] { false }\n",
    "proc fail() -> Result[Unit, ProcessError] { false }\n",
    "pure fail() -> Int { false }\n",
    "p\"missing\".exists()\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, output.stderr
  }
}

test test_membership_domains_views_and_source_order [error] { |ctx|
  let output = test.run_script(ctx, """
var order = ""
proc needle() -> Result[Str] { order = order + "n"; "é" }
proc container() -> Result[Str] { order = order + "c"; "café" }
needle()? in container()?
order == "nc"
"fé" in "café".byte_slice(2)
"" in ""
b"bc" in b"abcd".slice(1, length: 2)
b"" in b""
2 in [1, 2, 3]
4 not in [1, 2, 3]
p"foo" in p"foobar"
"bar" in p"foobar"
"é" not in "cafe"
print "done"
""")?
  assert output.success, output.stderr
  output.stdout == "done\n"
}

test test_boolean_literals_names_and_statement_match_tails [error] { |ctx|
  let passed = test.run_script(ctx, """
true
let condition = true
condition
proc check() {
  match "bad".parse_int() {
    Ok(_) => true
    Err(_) => true
  }
}
check()?
print "done"
""")?
  assert passed.success, passed.stderr
  passed.stdout == "done\n"
  let failed = test.run_script(ctx, "let condition = false\ncondition\nprint \"unreachable\"\n")?
  failed.status == 3
  failed.stdout == ""
  let literal = test.run_script(ctx, "false\nprint \"unreachable\"\n")?
  literal.status == 3
  literal.stdout == ""
}

test test_assertion_aliases_dynamic_boundaries_and_integer_status [error] { |ctx|
  let output = test.run_script(ctx, """
type Predicate = Bool
type PredicateResult = Result[Bool]
pure value() -> Predicate { false }
proc wrapped() -> PredicateResult { false }
let dynamic: Any = false
(dynamic)
let _ = value()
wrapped()? == false
pure checked() -> Result[Int, AssertionError] { true; 7 }
checked()? == 7
let status = run.status false
let _ = status
print "done"
""")?
  assert output.success, output.stderr
  output.stdout == "done\n"
  let integer = test.run_script(ctx, "7\n")?
  integer.status == 7
}

test test_retained_helpers_share_core_nominal_failure [error] { |ctx|
  let output = test.run_script(ctx, """
for failure in [test.ok(false, message: "custom"), test.eq(1, 2), test.ne(1, 1)] {
  match failure {
    Err(AssertionError.Failed {message: message}) => print $message
    Ok(_) => print "unexpected"
  }
}
""")?
  assert output.success, output.stderr
  assert "custom" in output.stdout, output.stdout
  assert "unexpected" not in output.stdout, output.stdout
}

test test_assertion_diagnostics_include_values_and_only_evaluated_operands [error] { |ctx|
  let equality = test.run_script(ctx, "let actual = [1, 2]\nactual == [1, 3]\n")?
  equality.status == 3
  assert "left: [1, 2]" in equality.stderr, equality.stderr
  assert "right: [1, 3]" in equality.stderr, equality.stderr
  let membership = test.run_script(ctx, "\"missing\" in {present: null}\n")?
  membership.status == 3
  assert "missing" in membership.stderr, membership.stderr
  assert "present: null" in membership.stderr, membership.stderr
  let compound = test.run_script(ctx, "proc skipped() -> Result[Bool] { print \"skipped\"; true }\nfalse and skipped()?\n")?
  compound.status == 3
  compound.stdout == ""
  let difference = test.run_script(ctx, "let actual = \"old\\nline\\n\"\nactual == \"new\\nline\\n\"\n")?
  difference.status == 3
  assert "diff:" in difference.stderr, difference.stderr
  assert "-old" in difference.stderr, difference.stderr
  assert "+new" in difference.stderr, difference.stderr
}

test test_assertion_diagnostics_bound_record_field_names [error] { |ctx|
  var field = ""
  for _ in range(1000) { field = field + "abcdefghij" }
  let output = test.run_script(ctx, f"let actual = {${field}: null}\n\"missing\" in actual\n")?
  output.status == 3
  assert "abcdefghij" in output.stderr, output.stderr
  output.stderr.byte_len() < 4096
}

test test_assertion_attempt_local_effects_and_unwrapped_exists [error] { |ctx|
  let output = test.run_script(ctx, """
proc local() [] {
  let attempt = retry [] { false; let _ = 0 }
  match attempt {
    Err(AssertionError.Failed {message: message}) => print $message
    Ok(_) => print "unexpected"
  }
}
local()?
p".".exists()?
""")?
  assert output.success, output.stderr
  assert "false" in output.stdout, output.stdout
  assert "unexpected" not in output.stdout, output.stdout
}

test test_retained_assertion_named_arguments_keep_source_evaluation_order [error] { |ctx|
  let output = test.run_script(ctx, """
var order = 0
proc left() -> Result[Int] { order = order * 10 + 1; 1 }
proc right() -> Result[Int] { order = order * 10 + 2; 1 }
test.eq(right: right()?, left: left()?)?
order == 21
""")?
  assert output.success, output.stderr
}

test test_user_fields_and_functions_named_membership_aliases_remain_usable [fs, error] { |ctx|
  let helper = fp"${ctx.temp_root}/custom.xsh"
  helper.write("""
##! Caller-owned functions.
## Caller-owned containment function.
export pure contains(value: Str) -> Bool { value == "present" }
## Caller-owned presence function.
export pure has(value: Str) -> Bool { value == "present" }
""")?
  let output = test.run_script(ctx, """
use custom
pure field_predicate(value: Str) -> Bool { value == "present" }
let fields = {contains: field_predicate, has: field_predicate}
fields.contains.call("present") == true
fields.has.call("present") == true
custom.contains("present")
custom.has("present")
print "done"
""")?
  assert output.success, output.stderr
  output.stdout == "done\n"
}
