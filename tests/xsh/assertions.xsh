test test_assert_failure_stops_script { |ctx|
  let output = test.run_script(
    ctx,
    """assert 1 == 2
print "unreachable"
""",
  )?
  assert output.status == 3
  assert output.stdout == ""
  assert "1 == 2" in output.stderr, output.stderr
  assert "AssertionError" in output.stderr, output.stderr
}

test test_boolean_values_and_explicit_discards_remain_values { |ctx|
  let output = test.run_script(
    ctx,
    """
pure predicate() -> Bool { false }
pure dynamic() -> Any { false }
proc wrapped() -> Result[Bool] { false }
proc wrapped_any() -> Result[Any] { false }
let _ = predicate()
let value = predicate()
let retried = retry [] { false }?
print f"{value} {dynamic()} {wrapped()?} {wrapped_any()?} {retried}"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """false false false false false
"""
}

test test_bool_statements_and_unit_tails_are_rejected { |ctx|
  for source in [
    """1 == 2
print "unreachable"
""",
    """proc check() { 2 > 3 }
check()?
""",
    """proc check() -> Result[Unit] { let flag = true; flag }
check()?
""",
    """proc check() { 2 > 3; print "after" }
check()?
""",
    """for item in [1] { item == 1 }
""",
    """[1, 2] |> each { |item| item > 0 }
""",
    """let flag = true
flag
""",
    """true
""",
    """proc check() { defer { 1 == 1 } }
check()?
""",
    """pure check() -> Result[Int, AssertionError] { true; 7 }
let _ = check()
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, source
    assert output.stdout == ""
    assert "check.bool-statement" in output.stderr, output.stderr
    assert "use `assert <expr>` to assert it, or `let _ = <expr>` to discard it" in output.stderr, output.stderr
  }
}

test test_unit_tail_and_non_tail_asserts_fail { |ctx|
  let output = test.run_script(
    ctx,
    """
proc check() { assert 2 > 3 }
check()?
print "unreachable"
""",
  )?
  assert output.status == 3
  assert output.stdout == ""
}

test test_membership_checks_map_keys_and_record_fields {
  let empty: Map[Int?] = {}
  let mapping = empty.set("present", null)
  assert "present" in mapping
  assert "absent" not in mapping
  let fields = {present: null}
  assert "present" in fields
  assert "absent" not in fields
  let words: Map[Str] = {key: "value"}
  assert "value" not in words
  assert "value" not in {key: "value"}
}

test test_assertion_single_evaluation_short_circuit_and_value_predicates { |ctx|
  let output = test.run_script(
    ctx,
    """
var calls = 0
proc tick() -> Result[Int] { calls += 1; calls }
assert tick()? == 1
assert true or tick()? == 99
assert ! (false and tick()? == 99)
assert calls == 1
let values = [1, 2, 3] |> where { false } |> collect()
assert values == []
let no = [1, 2] |> any { false }
let yes = [1, 2] |> all { true }
let not_all = [1, 2] |> all { false }
assert no == false
assert yes == true
assert not_all == false
print "done"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
}

test test_assertion_nominal_error_handlers_and_retry { |ctx|
  let output = test.run_script(
    ctx,
    """
pure fail() -> Result[Unit, AssertionError] { assert false }
match fail() {
  Err(AssertionError.Failed {message: message}) => print $message
  Ok(_) => print "unexpected"
}
var attempts = 0
proc tick() -> Result[Int] { attempts += 1; attempts }
let _ = retry [0ms, 0ms] {
  assert tick()? == 3
  let _ = attempts
}?
assert attempts == 3
""",
  )?
  assert output.success, output.stderr
  assert "false" in output.stdout, output.stdout
  assert "unexpected" not in output.stdout, output.stdout
}

test test_assertion_defers_preserve_primary_failure { |ctx|
  let output = test.run_script(
    ctx,
    """
proc cleanup(value: Str) { print $value }
proc broken_cleanup() { error.fail("cleanup failed")? }
proc fail() {
  defer cleanup("first")?
  defer broken_cleanup()?
  defer cleanup("last")?
  assert 8 < 3
}
fail()?
print "unreachable"
""",
  )?
  assert output.status == 3
  assert output.stdout == """last
first
"""
  assert "AssertionError.Failed" in output.stderr, output.stderr
  assert "8 < 3" in output.stderr, output.stderr
}

test test_assertion_contexts_require_result_effect_and_compatible_error { |ctx|
  for source in [
    """pure fail() -> Unit { assert false }
""",
    """proc fail() [] { assert false }
""",
    """proc fail() -> Result[Unit, ProcessError] { assert false }
""",
    """pure fail() -> Int { false }
""",
    """p"missing".exists()
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, output.stderr
  }
}

test test_membership_domains_views_and_source_order { |ctx|
  let output = test.run_script(
    ctx,
    """
var order = ""
proc needle() -> Result[Str] { order = order + "n"; "é" }
proc container() -> Result[Str] { order = order + "c"; "café" }
assert needle()? in container()?
assert order == "nc"
assert "fé" in "café".byte_slice(2)
assert "" in ""
assert b"bc" in b"abcd".slice(1, length: 2)
assert b"" in b""
assert b"" in b"abc"
assert "" in "abc"
assert 2 in [1, 2, 3]
assert 4 not in [1, 2, 3]
assert p"foo" in p"foobar"
assert "bar" in p"foobar"
assert p"/usr/lib" in p"/usr/lib64/tool"
assert "é" not in "cafe"
print "done"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
}

test test_boolean_literals_names_and_statement_match_tails { |ctx|
  let passed = test.run_script(
    ctx,
    """
assert true
let condition = true
assert condition
proc check() {
  match "bad".parse_int() {
    Ok(_) => assert true
    Err(_) => { assert true }
  }
}
check()?
print "done"
""",
  )?
  assert passed.success, passed.stderr
  assert passed.stdout == """done
"""
  let failed = test.run_script(
    ctx,
    """let condition = false
assert condition
print "unreachable"
""",
  )?
  assert failed.status == 3
  assert failed.stdout == ""
  let literal = test.run_script(
    ctx,
    """assert false
print "unreachable"
""",
  )?
  assert literal.status == 3
  assert literal.stdout == ""
  let arms = test.run_script(
    ctx,
    """proc check() {
  match 1 {
    1 => true
    _ => false
  }
}
check()?
""",
  )?
  assert arms.status == 2
  assert "check.bool-statement" in arms.stderr, arms.stderr
}

test test_assertion_aliases_dynamic_boundaries_and_integer_status { |ctx|
  let output = test.run_script(
    ctx,
    """
type Predicate = Bool
type PredicateResult = Result[Bool]
pure value() -> Predicate { false }
proc wrapped() -> PredicateResult { false }
let dynamic: Any = false
(dynamic)
let _ = value()
assert wrapped()? == false
pure checked() -> Result[Int, AssertionError] { assert true; 7 }
assert checked()? == 7
let status = run.status false
let _ = status
print "done"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
  let integer = test.run_script(
    ctx,
    """7
""",
  )?
  assert integer.status == 7
}

test test_retained_helpers_share_core_nominal_failure { |ctx|
  let output = test.run_script(
    ctx,
    """
for failure in [test.ok(false, message: "custom"), test.eq(1, 2), test.ne(1, 1)] {
  match failure {
    Err(AssertionError.Failed {message: message}) => print $message
    Ok(_) => print "unexpected"
  }
}
""",
  )?
  assert output.success, output.stderr
  assert "custom" in output.stdout, output.stdout
  assert "unexpected" not in output.stdout, output.stdout
}

test test_assertion_diagnostics_include_values_and_only_evaluated_operands { |ctx|
  let equality = test.run_script(
    ctx,
    """let actual = [1, 2]
assert actual == [1, 3]
""",
  )?
  assert equality.status == 3
  assert "left: [1, 2]" in equality.stderr, equality.stderr
  assert "right: [1, 3]" in equality.stderr, equality.stderr
  let ordering = test.run_script(
    ctx,
    """let small = 2
assert small > 5
""",
  )?
  assert ordering.status == 3
  assert "left: 2" in ordering.stderr, ordering.stderr
  assert "right: 5" in ordering.stderr, ordering.stderr
  assert ":2:8" in ordering.stderr, ordering.stderr
  let membership = test.run_script(
    ctx,
    """assert "missing" in {present: null}
""",
  )?
  assert membership.status == 3
  assert "missing" in membership.stderr, membership.stderr
  assert "present: null" in membership.stderr, membership.stderr
  let compound = test.run_script(
    ctx,
    """proc skipped() -> Result[Bool] { print "skipped"; true }
assert false and skipped()?
""",
  )?
  assert compound.status == 3
  assert compound.stdout == ""
  let difference = test.run_script(
    ctx,
    """let actual = "old\\nline\\n"
assert actual == "new\\nline\\n"
""",
  )?
  assert difference.status == 3
  assert "diff:" in difference.stderr, difference.stderr
  assert "-old" in difference.stderr, difference.stderr
  assert "+new" in difference.stderr, difference.stderr
}

test test_assertion_diagnostics_bound_record_field_names { |ctx|
  var field = ""
  for _ in range(1000) {
    field = field + "abcdefghij"
  }

  let source = f"""
    let actual = {{{field}: null}}
    assert \"missing\" in actual

    """
  let output = test.run_script(ctx, source)?
  assert output.status == 3
  assert "abcdefghij" in output.stderr, output.stderr
  assert output.stderr.byte_len() < 4096
}

test test_assertion_attempt_local_effects_and_unwrapped_exists { |ctx|
  let output = test.run_script(
    ctx,
    """
proc local() [] {
  let attempt = retry [] { assert false; let _ = 0 }
  match attempt {
    Err(AssertionError.Failed {message: message}) => print $message
    Ok(_) => print "unexpected"
  }
}
local()?
assert p".".exists()?
""",
  )?
  assert output.success, output.stderr
  assert "false" in output.stdout, output.stdout
  assert "unexpected" not in output.stdout, output.stdout
}

test test_retained_assertion_named_arguments_keep_source_evaluation_order { |ctx|
  let output = test.run_script(
    ctx,
    """
var order = 0
proc left() -> Result[Int] { order = order * 10 + 1; 1 }
proc right() -> Result[Int] { order = order * 10 + 2; 1 }
test.eq(right: right()?, left: left()?)?
assert order == 21
""",
  )?
  assert output.success, output.stderr
}

test test_user_fields_and_functions_named_membership_aliases_remain_usable { |ctx|
  let helper = fp"{ctx.temp_root}/custom.xsh"
  helper.write("""
##! Caller-owned functions.
## Caller-owned containment function.
export pure contains(value: Str) -> Bool { value == "present" }
## Caller-owned presence function.
export pure has(value: Str) -> Bool { value == "present" }
""")?
  let output = test.run_script(
    ctx,
    """
use custom
pure field_predicate(value: Str) -> Bool { value == "present" }
let fields = {contains: field_predicate, has: field_predicate}
assert fields.contains.call("present") == true
assert fields.has.call("present") == true
let erased: Any = fields
assert erased.contains("present").require(Bool)? == true
assert erased.has("present").require(Bool)? == true
assert custom.contains("present")
assert custom.has("present")
print "done"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
}

pure assertion_negated(flag: Bool) -> Bool {
  ! flag
}

test test_boolean_value_contexts_do_not_assert { |ctx|
  let output = test.run_script(
    ctx,
    """stream flags() [] -> Stream[Bool] {
  yield false
  yield 1 > 2
}
let yielded = flags() |> collect()
print $yielded.len()
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """2
"""
  assert assertion_negated(false)
  var flag = true
  flag = 2 < 1
  assert ! flag
  let picked = if flag { true } else { false }
  assert ! picked
  var reached = false
  if 3 < 2 {
    reached = true
  }

  assert ! reached
}

test test_status_and_optional_statements_have_no_truthiness { |ctx|
  let output = test.run_script(
    ctx,
    """let failed = run.status false
(failed)
let absent: Bool? = false
(absent)
print "done"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
  let integer = test.run_script(
    ctx,
    """proc check() -> Result[Unit] { let code = 7; (code) }
check()?
""",
  )?
  assert integer.status == 2, integer.stderr
  assert "check.type-mismatch" in integer.stderr
}

test test_removed_membership_apis_and_unsupported_domains_are_rejected { |ctx|
  for statement in [
    """let text = "abc"
let _ = text.contains("a")""",
    """let data = b"abc"
let _ = data.contains(b"a")""",
    """let items = [1, 2]
let _ = items.contains(1)""",
    """let counts: Map[Int] = {one: 1}
let _ = counts.has("one")""",
    """let fields = {one: 1}
let _ = fields.has("one")""",
    "test.contains(\"abc\", \"a\")?",
    "test.not_contains(\"abc\", \"z\")?",
  ] {
    let removed = test.run_script(ctx, statement + "\n")?
    assert removed.status == 2, removed.stderr
    assert "check.removed-membership" in removed.stderr, removed.stderr
  }

  for case in [
    {
      expression: "97 in b\"abc\"",
      code: "check.type-mismatch",
    },
    {
      expression: "\"a\" in b\"abc\"",
      code: "check.type-mismatch",
    },
    {
      expression: "\"a\" in rx\"a\"",
      code: "check.membership-type",
    },
    {
      expression: "1 in range(3)",
      code: "check.membership-type",
    },
  ] {
    let unsupported = test.run_script(ctx, "let _ = " + case.expression + "\n")?
    assert unsupported.status == 2, unsupported.stderr
    assert case.code in unsupported.stderr, unsupported.stderr
  }
}

test test_imported_module_boolean_statement_is_rejected { |ctx|
  fp"{ctx.temp_root}/statement_module.xsh".write("""##! Runs a statement.
## A public field.
export let present = 1
true
""")?
  let output = test.run_script(
    ctx,
    """use statement_module
print "unreachable"
""",
  )?
  assert output.status == 2, output.stderr
  assert output.stdout == ""
  assert "check.module-top-level" in output.stderr
}
