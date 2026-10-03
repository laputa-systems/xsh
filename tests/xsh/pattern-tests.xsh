test test_pattern_predicates_inspect_results_and_nested_payloads {
  let successful = Ok(7)
  let failed = Err("expected")
  let yes = successful is Ok(_)
  let no = failed is Ok(_)
  assert yes == true
  assert no == false
  assert successful is Ok(7) == true
  assert successful is Ok(8) == false
  let unit = Ok()
  assert unit is Ok == true
  assert failed is Err(_) == true
  assert successful == Ok(7)
  assert failed is Err(_) == true
  let nested = Ok({child: {answer: 42}})
  assert nested is Ok({child: {answer: 42}}) == true
  assert nested is Ok({child: {answer: 0}}) == false
  assert 42 is 42 == true
  assert 42 is 43 == false
  assert ! (42 is 43) == true
  assert b"payload" is b"payload" == true
  assert 3.5 is 3.5 == true
  let optional: Int? = null
  assert optional is null == true
  assert ((42 is 42 and 43 is 43) or 44 is 0) == true
  let filtered = [value for value in [Ok(1), Ok(2)] if value is Ok(2)]
  assert filtered == [Ok(2)]
}

pure pattern_test_dynamic_value() -> Any {
  "hello"
}

test test_pattern_predicates_narrow_stable_dynamic_bindings {
  let value = pattern_test_dynamic_value()
  let is_string = value is Str
  assert is_string == true
  if value is Str {
    assert value.upper() == "HELLO"
  } else {
    test.fail("expected string")?
  }
}

test test_pattern_predicates_reject_bindings_and_alternation { |ctx|
  for fixture in [
    {
      source: """let outcome: Result[Int] = Ok(1)
let matches = outcome is Ok(payload)
""",
      diagnostic: "check.pattern-test-binding",
    },
    {
      source: """let value = {answer: 42}
let matches = value is {answer}
""",
      diagnostic: "check.pattern-test-binding",
    },
    {
      source: """let value = 1
let matches = value is 1 | 2
""",
      diagnostic: "parse.pattern-test-alternation",
    },
    {
      source: """let value: Any = 1
let matches = value is not Int
""",
      diagnostic: "parse.expected-pattern",
    },
    {
      source: """let value = 1
let matches = value is unknown_name
""",
      diagnostic: "check.unknown-type",
    },
    {
      source: """let value = 1
let matches = value is Int
""",
      diagnostic: "check.pattern-type",
    },
    {
      source: """let value = Ok(1)
let matches = value is Ok
""",
      diagnostic: "check.pattern-arity",
    },
    {
      source: """type NotFound = Int
pure value() -> Any { 1 }
let matches = value() is NotFound
""",
      diagnostic: "check.pattern-test-ambiguous",
    },
  ] {
    let output = test.run_script(ctx, fixture.source)?
    assert output.success == false
    assert fixture.diagnostic in output.stderr
  }
}

enum PredicateChoice { EmptyChoice, PayloadChoice(Int, Str) }

enum OtherPredicateChoice { OtherChoice, AnotherOtherChoice }

error PredicateError = Missing(message: Str) : NotFound | Broken(message: Str) : InvalidData

pure pattern_test_tag_value() -> Any {
  PayloadChoice(7, "payload")
}

test test_pattern_predicates_keep_nominal_tags_and_error_facets {
  assert EmptyChoice is EmptyChoice == true
  assert PayloadChoice(7, "payload") is PayloadChoice(7, "payload") == true
  assert PayloadChoice(7, "payload") is PayloadChoice(8, _) == false
  let tag_value = pattern_test_tag_value()
  assert tag_value is PredicateChoice == true
  assert tag_value is OtherPredicateChoice == false
  let missing: PredicateError = PredicateError.Missing(message: "missing")
  assert missing is PredicateError.Missing == true
  assert missing is PredicateError.Broken == false
  assert missing is PredicateError.Missing {message: "missing"} == true
  assert missing is PredicateError.Missing {message: "other"} == false
  assert missing is NotFound == true
  assert missing is InvalidData == false
  let failure = Err(missing)
  assert failure is Err(PredicateError.Missing {message: "missing"}) == true
  assert failure is Err(is NotFound) == true
}

test test_pattern_predicates_evaluate_subject_once { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc subject() -> Result[Int] {
  print "subject"
  return Ok(7)
}
let selected = subject() is Ok(7)
print ${selected}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """subject
true
"""
}

test test_pattern_predicate_statements_assert_and_values_do_not_propagate { |ctx|
  let output = test.run_script(
    ctx,
    r"""let outcome = Err("failed")
let tested = outcome is Ok(_)
print ${tested}
assert outcome is Err(_)
assert outcome is Ok(_)
print "unreachable"
""",
  )?
  assert ! output.success, output.stderr
  assert output.stdout == """false
"""
  assert "AssertionError.Failed" in output.stderr
}

test test_pattern_predicate_lint_fixes_converge_and_formatter_retains_syntax { |ctx|
  let source = r"""let value = Ok(7)
let selected = match value { Ok(_) => true, Err(_) => false }
print ${selected}
"""
  let candidate = test.temp_file(ctx, name: "pattern-test-lint.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --fix $candidate ?
  {
    let assertion_condition = fixed.status.exited_with(0)
    let assertion_message = fixed.stderr
    assert assertion_condition, assertion_message
  }
  let first = candidate.read_text()?
  assert " is Ok(_)" in first
  let second = run.capture --text "xsht" lint --fix $candidate ?
  {
    let assertion_condition = second.status.exited_with(0)
    let assertion_message = second.stderr
    assert assertion_condition, assertion_message
  }
  assert candidate.read_text()? == first
  let formatted = run.capture --text "xsht" fmt $candidate ?
  {
    let assertion_condition = formatted.status.exited_with(0)
    let assertion_message = formatted.stderr
    assert assertion_condition, assertion_message
  }
  assert " is Ok(_)" in candidate.read_text()?
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  {
    let assertion_condition = stable.status.exited_with(0)
    let assertion_message = stable.stderr
    assert assertion_condition, assertion_message
  }
  let output = test.run_script(ctx, candidate.read_text()?)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
"""
}

test test_pattern_predicate_lint_preserves_comments_and_bindings { |ctx|
  for source in [
    """let value = Ok(7)
let selected = match value {
  # Preserve this explanation.
  Ok(_) => true,
  _ => false
}
print done
""",
    """let value = Ok(7)
let selected = match value { Ok(payload) => true, _ => false }
print done
""",
  ] {
    let candidate = test.temp_file(ctx, name: "pattern-test-no-fix.xsh", contents: bytes.from_text(source))?
    let fixed = run.capture --text "xsht" lint --fix $candidate ?
    assert candidate.read_text()? == source
    assert "lint.boolean-pattern-test" in fixed.stderr
  }
}

test test_pattern_predicates_resolve_qualified_constructor_type_and_facet_names { |ctx|
  let root = test.temp_dir(ctx, name: "predicate-module")?
  fp"${root}/predicate.xsh".write("""
##! Pattern predicate fixture module.
## Choice constructors.
export enum Choice { Ready, Payload(Int) }
## Fixture errors.
export error Failure = Missing(detail: Str) : NotFound
## A dynamic choice.
export pure dynamic_choice() -> Any { return Ready }
""")?
  let output = test.run_script(
    ctx,
    r"""
use predicate as p
let ready = p.Ready
let ready_matches = ready is p.Ready
if ready is p.Ready { test.eq(ready_matches, true)? }
let payload_matches = p.Payload(7) is p.Payload(7)
let nested = Ok(p.Ready)
let nested_matches = nested is Ok(p.Ready)
let dynamic = p.dynamic_choice()
let type_matches = dynamic is p.Choice
let missing = p.Failure.Missing(detail: "missing")
let variant_matches = missing is p.Failure.Missing
let facet_matches = missing is p.NotFound
print $ready_matches $payload_matches $type_matches $variant_matches $facet_matches $nested_matches
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true true true true true true
"""
}

pure pattern_test_dynamic_list() -> Any {
  [1, 2]
}

test test_pattern_predicates_resolve_parameterized_types_and_multiline_tests { |ctx|
  let value = pattern_test_dynamic_list()
  let matched = value is List[Int]
  assert matched == true
  assert value is List[Str] == false
  if value is List[Int] {
    assert value[0] == 1
  }

  let output = test.run_script(
    ctx,
    r"""pure subject() -> Any { [1, 2] }
let value = subject()
let matched = value is
  List[Int]
print ${matched}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
"""
}

test test_pattern_predicate_formatter_preserves_parameterized_types { |ctx|
  let source = r"""pure subject() -> Any { [1, 2] }
let selected = subject() is List[Int]
print ${selected}
"""
  let candidate = test.temp_file(ctx, name: "parameterized-predicate.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  {
    let assertion_condition = formatted.status.exited_with(0)
    let assertion_message = formatted.stderr
    assert assertion_condition, assertion_message
  }
  assert " is List[Int]" in candidate.read_text()?
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  {
    let assertion_condition = stable.status.exited_with(0)
    let assertion_message = stable.stderr
    assert assertion_condition, assertion_message
  }
  let output = test.run_script(ctx, candidate.read_text()?)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
"""
}

test test_pattern_predicates_leave_control_body_braces {
  let missing: PredicateError = PredicateError.Missing(message: "missing")
  var branches = 0
  if missing is PredicateError.Missing {
    branches += 1
  }

  if missing is PredicateError.Missing {message: "missing"} {
    branches += 1
  }

  if missing is PredicateError.Missing {} else {
    test.fail("matching empty branch was skipped")?
  }

  while missing is PredicateError.Broken {
    test.fail("nonmatching loop was entered")?
  }

  let matched = if missing is PredicateError.Missing { true } else { false }
  assert matched == true
  assert branches == 2
}

test test_pattern_predicates_keep_following_type_pattern_match_arms { |ctx|
  let output = test.run_script(
    ctx,
    r"""error ArmError = Missing(message: Str) : NotFound
pure describe(value: Error) -> Str {
  match value {
    is PermissionDenied => return "denied"
    is NotFound => return "missing"
    _ => return "other"
  }
}
pure describe_expression(value: Error) -> Str {
  let result = match value {
    is PermissionDenied => "denied"
    is NotFound => "missing"
    _ => "other"
  }
  result
}
let missing = ArmError.Missing(message: "missing")
print (describe(missing)) (describe_expression(missing))
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """missing missing
"""
}

test test_pattern_predicate_facet_narrowing_retains_nominal_fallback_errors { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error BuildError = Missing(message: Str) : NotFound
pure nominal_message(failure: BuildError) -> Str { failure.message }
let outcome: Result[Str, BuildError] = Err(BuildError.Missing(message: "absent"))
let recovered = outcome ?? { |failure|
  if failure is NotFound { nominal_message(failure) + failure.message } else { "other" }
}
print $recovered
let failure: BuildError = BuildError.Missing(message: "conditional")
if let is NotFound = failure { print ${nominal_message(failure)} }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """absentabsent
conditional
"""
}

test test_pattern_predicate_dynamic_facets_retain_the_error_api { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error BuildError = Missing(message: Str) : NotFound
pure dynamic_failure() -> Any { BuildError.Missing(message: "absent") }
pure error_message(failure: Error) -> Str { failure.message }
let failure = dynamic_failure()
if failure is NotFound {
  let optional: NotFound? = failure
  print ${failure.message} ${optional?.message ?? "missing"} ${error_message(failure)}
}
if let is NotFound = dynamic_failure() { print matched }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """absent absent absent
matched
"""
}

test test_capitalized_unknown_pattern_names_are_rejected { |ctx|
  let rejected = test.run_script(
    ctx,
    """match p"missing.log".read_text() {
  Ok(_) => print "ok"
  Err(NotFound) => print "caught"
}
""",
  )?
  assert ! rejected.success
  assert "check.pattern-capitalized-binding" in rejected.stderr
  assert "is NotFound" in rejected.stderr
}

test test_field_path_line_followed_by_not_in_or_is_is_an_expression {
  let r = {a: {b: 1}}
  let xs = [2, 3]
  let absent = {
    r.a.b not in xs
  }
  let typed = {
    r.a.b is 1
  }
  assert absent
  assert typed
}

pure sign_name(n: Int) -> Str {
  match n {
    -1 => "minus one"
    0 => "zero"
    _ => "other"
  }
}

test test_negative_number_literal_patterns {
  assert sign_name(-1) == "minus one"
  assert sign_name(1) == "other"
  let offset = -2.5
  assert offset is -2.5
  let label = match -3 {
    -2 | -3 => "small",
    _ => "other",
  }
  assert label == "small"
}

pure arm_capture_then_let(r: Int) -> Int {
  let a = match r {
    e => e + 1,
  }
  let e = 7
  let b = match e {
    7 => 10,
    _ => 20,
  }
  a + b + e
}

test test_match_arm_captures_end_with_their_arm { |ctx|
  assert arm_capture_then_let(1) == 19

  # An arm capture may shadow a binding of the same scope, which resolves
  # again after the arm (lint.shadowing flags the style, not the meaning).
  let shadowed = test.run_script(
    ctx,
    """pure shadows(r: Int) -> Int {
  let e = 100
  let a = match r {
    e => e + 1
  }
  a + e
}
print (shadows(1))
""",
  )?
  assert shadowed.success, shadowed.stderr
  assert shadowed.stdout == """102
"""
}

test test_error_variant_patterns_match_errors_raised_in_another_module { |ctx|
  let root = test.temp_dir(ctx, name: "error-identity-module")?
  fp"${root}/raiser.xsh".write(r"""
##! Error identity fixture module.
## Fixture errors.
export error Failure = Missing(detail: Str) | Busy
## Fails with a declared family.
export pure fail_typed() -> Result[Int, Failure] { return Err(Failure.Missing(detail: "typed")) }
## Fails through the broad Error carrier.
export pure fail_broad() -> Result[Int] { return Err(Failure.Missing(detail: "broad")) }
## Matches inside the module.
export pure describe(result: Result[Int, Failure]) -> Str {
  match result {
    Err(Failure.Missing {detail}) => f"missing ${detail}"
    _ => "other"
  }
}
""")?
  let output = test.run_script(
    ctx,
    r"""
use raiser as r
match r.fail_typed() {
  Ok(_) => print "ok"
  Err(r.Failure.Missing {detail}) => print $detail
  Err(r.Failure.Busy) => print "busy"
}
match r.fail_broad() {
  Err(r.Failure.Missing {detail}) => print $detail
  _ => print "other"
}
let local: Result[Int, r.Failure] = Err(r.Failure.Missing(detail: "local"))
print (r.fail_typed() is Err(r.Failure.Missing)) (r.describe(local))
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert output.success, output.stderr
  assert output.stdout == """typed
broad
true missing local
"""
}
