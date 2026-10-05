test test_assert_failure_stops_script { |ctx|
  let output = test.expect(
    ctx,
    """assert 1 == 2
print "unreachable"
""",
    status: 3,
  )?
  assert output.stdout == ""
  assert "1 == 2" in output.stderr, output.stderr
  assert "AssertionError" in output.stderr, output.stderr
}

test test_boolean_values_and_explicit_discards_remain_values { |ctx|
  let output = test.expect(
    ctx,
    """
pure predicate() -> Bool { false }
pure dynamic() -> Any { false }
proc wrapped() -> Result[Bool] { false }
proc wrapped_any() -> Result[Any] { false }
let _ = predicate()
let value = predicate()
let retried = retry [] { false }?
print f"{value} {dynamic().require(Bool)?} {wrapped()?} {wrapped_any()?.require(Bool)?} {retried}"
""",
    status: 0,
  )?
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
  let output = test.expect(
    ctx,
    """
proc check() { assert 2 > 3 }
check()?
print "unreachable"
""",
    status: 3,
  )?
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
  let output = test.expect(
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
    status: 0,
  )?
  assert output.stdout == """done
"""
}

test test_assertion_nominal_error_handlers_and_retry { |ctx|
  let output = test.expect(
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
    status: 0,
    stdout: ["false"],
  )?
  assert "unexpected" not in output.stdout, output.stdout
}

test test_assertion_defers_preserve_primary_failure { |ctx|
  let output = test.expect(
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
    status: 3,
  )?
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
    test.expect(ctx, source, status: 2)?
  }
}

test test_membership_domains_views_and_source_order { |ctx|
  let output = test.expect(
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
    status: 0,
  )?
  assert output.stdout == """done
"""
}

test test_boolean_literals_names_and_statement_match_tails { |ctx|
  let passed = test.expect(
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
    status: 0,
  )?
  assert passed.stdout == """done
"""
  let failed = test.expect(
    ctx,
    """let condition = false
assert condition
print "unreachable"
""",
    status: 3,
  )?
  assert failed.stdout == ""
  let literal = test.expect(
    ctx,
    """assert false
print "unreachable"
""",
    status: 3,
  )?
  assert literal.stdout == ""
  test.expect(
    ctx,
    """proc check() {
  match 1 {
    1 => true
    _ => false
  }
}
check()?
""",
    status: 2,
    stderr: ["check.bool-statement"],
  )?
}

test test_assertion_aliases_dynamic_boundaries_and_integer_status { |ctx|
  let output = test.expect(
    ctx,
    """
type Predicate = Bool
type PredicateResult = Result[Bool]
pure value() -> Predicate { false }
proc wrapped() -> PredicateResult { false }
let _ = value()
assert wrapped()? == false
pure checked() -> Result[Int, AssertionError] { assert true; 7 }
assert checked()? == 7
let status = run.status false
let _ = status
print "done"
""",
    status: 0,
  )?
  assert output.stdout == """done
"""
  let dynamic = test.expect(
    ctx,
    """let dynamic: Any = false
(dynamic)
""",
    status: 2,
    stderr: ["check.ignored-result"],
  )?
  assert "check.bool-statement" not in dynamic.stderr, dynamic.stderr
  test.expect(
    ctx,
    """7
""",
    status: 7,
  )?
}

test test_retained_helpers_share_core_nominal_failure { |ctx|
  let output = test.expect(
    ctx,
    """
for failure in [test.ok(false, message: "custom"), test.eq(1, 2), test.ne(1, 1)] {
  match failure {
    Err(AssertionError.Failed {message: message}) => print $message
    Ok(_) => print "unexpected"
  }
}
""",
    status: 0,
    stdout: ["custom"],
  )?
  assert "unexpected" not in output.stdout, output.stdout
}

test test_assertion_diagnostics_include_values_and_only_evaluated_operands { |ctx|
  test.expect(
    ctx,
    """let actual = [1, 2]
assert actual == [1, 3]
""",
    status: 3,
    stderr: ["left: [1, 2]", "right: [1, 3]"],
  )?
  test.expect(
    ctx,
    """let small = 2
assert small > 5
""",
    status: 3,
    stderr: ["left: 2", "right: 5", ":2:8"],
  )?
  test.expect(
    ctx,
    """assert "missing" in {present: null}
""",
    status: 3,
    stderr: ["missing", "present: null"],
  )?
  let compound = test.expect(
    ctx,
    """proc skipped() -> Result[Bool] { print "skipped"; true }
assert false and skipped()?
""",
    status: 3,
  )?
  assert compound.stdout == ""
  test.expect(
    ctx,
    """let actual = "old\\nline\\n"
assert actual == "new\\nline\\n"
""",
    status: 3,
    stderr: ["diff:", "-old", "+new"],
  )?
}

test test_assertion_diagnostics_bound_record_field_names { |ctx|
  var field = ""
  repeat 1000 times {
    field = field + "abcdefghij"
  }

  let source = f"""
    let actual = {{{field}: null}}
    assert \"missing\" in actual

    """
  let output = test.expect(ctx, source, status: 3, stderr: ["abcdefghij"])?
  assert output.stderr.byte_len() < 4096
}

test test_assertion_attempt_local_effects_and_unwrapped_exists { |ctx|
  let output = test.expect(
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
    status: 0,
    stdout: ["false"],
  )?
  assert "unexpected" not in output.stdout, output.stdout
}

test test_retained_assertion_named_arguments_keep_source_evaluation_order { |ctx|
  test.expect(
    ctx,
    """
var order = 0
proc left() -> Result[Int] { order = order * 10 + 1; 1 }
proc right() -> Result[Int] { order = order * 10 + 2; 1 }
test.eq(right: right()?, left: left()?)?
assert order == 21
""",
    status: 0,
  )?
}

test test_user_fields_and_functions_named_membership_aliases_remain_usable { |ctx|
  let helper = fp"{ctx.temp_root}/custom.xsh"
  helper.write("""
##! Caller-owned functions.
## Caller-owned containment function.
export pure contains(value: Str) -> Bool { value == "present" }
## Caller-owned presence function.
export pure has(value: Str) -> Bool { value == "present" }
""")
  let output = test.expect(
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
    status: 0,
  )?
  assert output.stdout == """done
"""
}

pure assertion_negated(flag: Bool) -> Bool {
  ! flag
}

test test_boolean_value_contexts_do_not_assert { |ctx|
  let output = test.expect(
    ctx,
    """stream flags() [] -> Stream[Bool] {
  yield false
  yield 1 > 2
}
let yielded = flags() |> collect()
print $yielded.len()
""",
    status: 0,
  )?
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
  for source in [
    """let failed = run.status false
(failed)
print "done"
""",
    """let absent: Bool? = false
(absent)
print "done"
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, source
    assert output.stdout == ""
    assert "check.ignored-result" in output.stderr, output.stderr
    assert "check.bool-statement" not in output.stderr, output.stderr
  }

  test.expect(
    ctx,
    """proc check() -> Result[Unit] { let code = 7; (code) }
check()?
""",
    status: 2,
    stderr: ["check.type-mismatch"],
  )?
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
    test.expect(ctx, statement + "\n", status: 2, stderr: ["check.removed-membership"])?
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
    test.expect(ctx, "let _ = " + case.expression + "\n", status: 2, stderr: [case.code])?
  }
}

test test_imported_module_boolean_statement_is_rejected { |ctx|
  fp"{ctx.temp_root}/statement_module.xsh".write("""##! Runs a statement.
## A public field.
export let present = 1
true
""")
  let output = test.expect(
    ctx,
    """use statement_module
print "unreachable"
""",
    status: 2,
  )?
  assert output.stdout == ""
  assert "check.module-top-level" in output.stderr
}
