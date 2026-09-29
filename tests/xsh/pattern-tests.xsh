test test_pattern_predicates_inspect_results_and_nested_payloads [error] {
  let successful: Result[Int] = Ok(7)
  let failed: Result[Int, Str] = Err("expected")
  let yes = successful is Ok(_)
  let no = failed is Ok(_)
  test.eq(yes, true)?
  test.eq(no, false)?
  test.eq(successful is Ok(7), true)?
  test.eq(successful is Ok(8), false)?
  let unit = Ok()
  test.eq(unit is Ok, true)?
  test.eq(failed is Err(_), true)?
  test.eq(successful, Ok(7))?
  test.eq(failed is Err(_), true)?
  let nested = Ok({child: {answer: 42}})
  test.eq(nested is Ok({child: {answer: 42}}), true)?
  test.eq(nested is Ok({child: {answer: 0}}), false)?
  test.eq(42 is 42, true)?
  test.eq(42 is 43, false)?
  test.eq(!(42 is 43), true)?
  test.eq(b"payload" is b"payload", true)?
  test.eq(3.5 is 3.5, true)?
  let optional: Int? = null
  test.eq(optional is null, true)?
  test.eq((42 is 42) and (43 is 43) or (44 is 0), true)?
  let filtered = [value for value in [Ok(1), Ok(2)] if value is Ok(2)]
  test.eq(filtered, [Ok(2)])?
}

pure pattern_test_dynamic_value() -> Any { "hello" }

test test_pattern_predicates_narrow_stable_dynamic_bindings [error] {
  let value = pattern_test_dynamic_value()
  let is_string = value is Str
  test.eq(is_string, true)?
  if value is Str {
    test.eq(value.upper(), "HELLO")?
  } else {
    test.fail("expected string")?
  }
}

test test_pattern_predicates_reject_bindings_and_alternation [error] { |ctx|
  for fixture in [
    {source: "let outcome: Result[Int] = Ok(1)\nlet matches = outcome is Ok(payload)\n", diagnostic: "check.pattern-test-binding"},
    {source: "let value = {answer: 42}\nlet matches = value is {answer}\n", diagnostic: "check.pattern-test-binding"},
    {source: "let value = 1\nlet matches = value is 1 | 2\n", diagnostic: "parse.pattern-test-alternation"},
    {source: "let value = 1\nlet matches = value is unknown_name\n", diagnostic: "check.unknown-type"},
    {source: "let value = 1\nlet matches = value is Int\n", diagnostic: "check.pattern-type"},
    {source: "let value = Ok(1)\nlet matches = value is Ok\n", diagnostic: "check.pattern-arity"},
    {source: "type NotFound = Int\npure value() -> Any { 1 }\nlet matches = value() is NotFound\n", diagnostic: "check.pattern-test-ambiguous"},
  ] {
    let output = test.run_script(ctx, fixture.source)?
    test.eq(output.success, false)?
    test.contains(output.stderr, fixture.diagnostic)?
  }
}

enum PredicateChoice { EmptyChoice, PayloadChoice(Int, Str) }
enum OtherPredicateChoice { OtherChoice, AnotherOtherChoice }
error PredicateError = Missing(message: Str) : NotFound | Broken(message: Str) : InvalidData

pure pattern_test_tag_value() -> Any { PayloadChoice(7, "payload") }

test test_pattern_predicates_keep_nominal_tags_and_error_facets [error] {
  test.eq(EmptyChoice is EmptyChoice, true)?
  test.eq(PayloadChoice(7, "payload") is PayloadChoice(7, "payload"), true)?
  test.eq(PayloadChoice(7, "payload") is PayloadChoice(8, _), false)?
  let tag_value = pattern_test_tag_value()
  test.eq(tag_value is PredicateChoice, true)?
  test.eq(tag_value is OtherPredicateChoice, false)?
  let missing: PredicateError = PredicateError.Missing(message: "missing")
  test.eq(missing is PredicateError.Missing, true)?
  test.eq(missing is PredicateError.Broken, false)?
  test.eq(missing is PredicateError.Missing {message: "missing"}, true)?
  test.eq(missing is PredicateError.Missing {message: "other"}, false)?
  test.eq(missing is NotFound, true)?
  test.eq(missing is InvalidData, false)?
  let failure: Result[Int, PredicateError] = Err(missing)
  test.eq(failure is Err(PredicateError.Missing {message: "missing"}), true)?
  test.eq(failure is Err(is NotFound), true)?
}

test test_pattern_predicates_evaluate_subject_once [error] { |ctx|
  let output = test.run_script(ctx, r"""proc subject() -> Result[Int] {
  print "subject"
  return Ok(7)
}
let selected = subject() is Ok(7)
print ${selected}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "subject\ntrue\n")?
}

test test_pattern_predicate_lint_fixes_converge_and_formatter_retains_syntax [fs, process, error] { |ctx|
  let source = r"""let value = Ok(7)
let selected = match value { Ok(_) => true, Err(_) => false }
print ${selected}
"""
  let candidate = test.temp_file(ctx, name: "pattern-test-lint.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(fixed.status.exited_with(0), fixed.stderr)?
  let first = candidate.read_text()?
  test.contains(first, " is Ok(_)")?
  let second = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(second.status.exited_with(0), second.stderr)?
  test.eq(candidate.read_text()?, first)?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  test.ok(formatted.status.exited_with(0), formatted.stderr)?
  test.contains(candidate.read_text()?, " is Ok(_)")?
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  test.ok(stable.status.exited_with(0), stable.stderr)?
  let output = test.run_script(ctx, candidate.read_text()?)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\n")?
}

test test_pattern_predicate_lint_preserves_comments_and_bindings [fs, process, error] { |ctx|
  for source in [
    "let value = Ok(7)\nlet selected = match value {\n  # Preserve this explanation.\n  Ok(_) => true,\n  _ => false\n}\nprint done\n",
    "let value = Ok(7)\nlet selected = match value { Ok(payload) => true, _ => false }\nprint done\n",
  ] {
    let candidate = test.temp_file(ctx, name: "pattern-test-no-fix.xsh", contents: bytes.from_text(source))?
    let fixed = run.capture --text "xsht" lint --fix $candidate ?
    test.eq(candidate.read_text()?, source)?
    test.contains(fixed.stderr, "lint.boolean-pattern-test")?
  }
}

test test_pattern_predicates_resolve_qualified_constructor_type_and_facet_names [fs, error] { |ctx|
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
  let output = test.run_script(ctx, r"""
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
""", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true true true true true true\n")?
}


pure pattern_test_dynamic_list() -> Any { [1, 2] }

test test_pattern_predicates_resolve_parameterized_types_and_multiline_tests [error] { |ctx|
  let value = pattern_test_dynamic_list()
  let matched = value is List[Int]
  test.eq(matched, true)?
  test.eq(value is List[Str], false)?
  if value is List[Int] {
    test.eq(value[0], 1)?
  }
  let output = test.run_script(ctx, r"""pure subject() -> Any { [1, 2] }
let value = subject()
let matched = value
  is List[Int]
print ${matched}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\n")?
}

test test_pattern_predicate_formatter_preserves_parameterized_types [fs, process, error] { |ctx|
  let source = r"""pure subject() -> Any { [1, 2] }
let selected = subject() is List[Int]
print ${selected}
"""
  let candidate = test.temp_file(ctx, name: "parameterized-predicate.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  test.ok(formatted.status.exited_with(0), formatted.stderr)?
  test.contains(candidate.read_text()?, " is List[Int]")?
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  test.ok(stable.status.exited_with(0), stable.stderr)?
  let output = test.run_script(ctx, candidate.read_text()?)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\n")?
}

test test_pattern_predicates_leave_control_body_braces [error] {
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
  test.eq(matched, true)?
  test.eq(branches, 2)?
}

test test_pattern_predicates_keep_following_type_pattern_match_arms [error] { |ctx|
  let output = test.run_script(ctx, r"""error ArmError = Missing(message: Str) : NotFound
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "missing missing\n")?
}

test test_pattern_predicate_facet_narrowing_retains_nominal_fallback_errors [error] { |ctx|
  let output = test.run_script(ctx, r"""
error BuildError = Missing(message: Str) : NotFound
pure nominal_message(failure: BuildError) -> Str { failure.message }
let outcome: Result[Str, BuildError] = Err(BuildError.Missing(message: "absent"))
let recovered = outcome ?? { |failure|
  if failure is NotFound { nominal_message(failure) + failure.message } else { "other" }
}
print $recovered
let failure: BuildError = BuildError.Missing(message: "conditional")
if let is NotFound = failure { print ${nominal_message(failure)} }
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "absentabsent\nconditional\n")?
}

test test_pattern_predicate_dynamic_facets_retain_the_error_api [error] { |ctx|
  let output = test.run_script(ctx, r"""
error BuildError = Missing(message: Str) : NotFound
pure dynamic_failure() -> Any { BuildError.Missing(message: "absent") }
pure error_message(failure: Error) -> Str { failure.message }
let failure = dynamic_failure()
if failure is NotFound {
  let optional: NotFound? = failure
  print ${failure.message} ${optional?.message ?? "missing"} ${error_message(failure)}
}
if let is NotFound = dynamic_failure() { print matched }
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "absent absent absent\nmatched\n")?
}
