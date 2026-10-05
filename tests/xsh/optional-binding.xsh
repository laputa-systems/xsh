type Executor = {name: Str, slots: Int}

type Context = {executor: Executor?, retries: Int?}

error LookupError = Missing(key: Str) : NotFound

pure find(names: List[Str], wanted: Str) -> Str? {
  for name in names {
    return name when name == wanted
  }

  null
}

pure lookup(table: Map[Str, Int], key: Str) -> Result[Int?, LookupError] {
  if key == "" {
    Err(.Missing(key:))
  } else if key in table {
    Ok(table[key])
  } else {
    Ok(null)
  }
}

pure parsed(text: Str?) -> Result[Int, LookupError]? {
  guard text != null else {
    return null
  }

  let outcome: Result[Int, LookupError] = if text == "12" {
    Ok(12)
  } else {
    Err(.Missing(key: text))
  }
  outcome
}

pure executor_label(context: Context) -> Str {
  guard let executor = context.executor else {
    return "no executor"
  }

  # The optional subject is narrowed as well as the new binding.
  let again: Executor = context.executor
  f"{executor.name}:{again.slots}"
}

test test_guard_let_binds_a_present_optional_and_exits_on_null {
  assert executor_label({executor: {name: "local", slots: 4}, retries: null}) == "local:4"
  assert executor_label({executor: null, retries: 2}) == "no executor"
}

test test_guard_let_optional_destructures_and_accepts_an_annotation {
  let context: Context = Context(executor: {name: "remote", slots: 8}, retries: null)
  guard let {name, slots} = context.executor else {
    return test.fail("expected an executor")
  }

  assert name == "remote"
  assert slots == 8
  guard let retries: Int = context.retries else {
    return
  }

  test.fail(f"unexpected retries {retries}")
}

test test_guard_let_optional_in_a_loop_may_continue_or_break {
  let names = ["a", "b", "c"]
  var found = []
  for wanted in ["c", "x", "a", "stop", "b"] {
    break when wanted == "stop"
    guard let name = find(names, wanted) else {
      continue
    }

    found += [name]
  }

  assert found == ["c", "a"]
}

test test_optional_binding_evaluates_its_subject_once { |ctx|
  let output = test.expect(
    ctx,
    """proc next(value: Int?) -> Int? {
  print "next"
  value
}

proc guarded(value: Int?) {
  guard let found = next(value) else {
    print "guard: null"
    return
  }
  print f"guard: {found}"
}

guarded(1)
guarded(null)
if let found = next(2) {
  print f"if: {found}"
}
if let found = next(null) {
  print f"if: {found}"
} else {
  print "if: null"
}
""",
    status: 0,
  )?
  assert output.stdout == "next\nguard: 1\nnext\nguard: null\nnext\nif: 2\nnext\nif: null\n"
}

# The outermost type selects the form. `Result[T?]` is a Result binding whose
# target stays optional: `Ok(null)` binds `null`, and only `Err` fails.
test test_guard_let_over_a_result_of_an_optional_is_a_result_binding {
  let table: Map[Str, Int] = {a: 1}
  guard let present = lookup(table, "a") else { |failure|
    return test.fail(f"unexpected {failure.message}")
  }

  assert present == 1
  guard let absent = lookup(table, "zz") else { |failure|
    return test.fail(f"unexpected {failure.message}")
  }

  assert absent == null
  var failed = ""
  for key in [""] {
    guard let _ = lookup(table, key) else { |failure|
      failed = failure.message
      continue
    }

    test.fail("expected an error")
  }

  assert failed != ""
}

# `Result[T]?` is an optional binding: only `null` fails, and the target is
# the Result itself, still to be unwrapped.
test test_guard_let_over_an_optional_result_binds_the_result {
  guard let outcome = parsed("12") else {
    return test.fail("expected a result")
  }

  assert outcome == Ok(12)
  guard let missing = parsed(null) else {
    return
  }

  assert missing is Ok(_)
  test.fail("expected no result")
}

test test_if_let_and_while_let_bind_a_present_optional {
  let context: Context = Context(executor: null, retries: 3)
  let retries = if let count = context.retries { count + 1 } else { 0 }
  assert retries == 4
  var label = "unset"
  if let executor = context.executor {
    label = executor.name
  } else if let count = context.retries {
    label = f"retries {count}"
  } else {
    label = "nothing"
  }

  assert label == "retries 3"

  let queue: List[Int?] = [3, 4, null, 5]
  var index = 0
  var total = 0
  while let item = queue[index] {
    total += item
    index += 1
  }

  assert total == 7
  assert index == 2

  # A discard tests presence, and the branch may use the narrowed subject.
  if let _ = context.retries {
    assert context.retries + 1 == 4
  } else {
    test.fail("expected retries")
  }
}

# A pattern that can fail keeps its meaning over an optional subject.
test test_refutable_patterns_over_an_optional_are_unchanged {
  let maybe: Int? = 3
  let none: Int? = null
  var seen = []
  for value in [maybe, none] {
    if let 3 = value {
      seen += ["three"]
    } else {
      seen += ["other"]
    }
  }

  assert seen == ["three", "other"]
}

test test_optional_guard_let_rejects_a_failure_parameter { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "optional-guard-param.xsh",
    contents: bytes.from_text("""proc label(name: Str?) -> Str {
  guard let found = name else { |failure|
    return "none"
  }
  found
}
print \${label(null)}
"""),
  )?
  let checked = run.capture --text --accept=[0, 1, 2] "xsht" check $candidate
  let report = checked.stdout + checked.stderr
  assert ! checked.status.ok, report
  assert "check.block-params" in report, report
  assert "an optional `guard let` has no error to bind" in report, report
  assert ":2:34" in report, report
  assert report.split("err[").len() == 2, report
}

test test_optional_guard_let_must_leave_the_continuation { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "optional-guard-fallthrough.xsh",
    contents: bytes.from_text("""proc label(name: Str?) -> Str {
  guard let found = name else {
    print "none"
  }
  found
}
print \${label(null)}
"""),
  )?
  let checked = run.capture --text --accept=[0, 1, 2] "xsht" check $candidate
  let report = checked.stdout + checked.stderr
  assert ! checked.status.ok, report
  assert "check.guard-fallthrough" in report, report
}

test test_conditional_binding_still_rejects_other_subjects { |ctx|
  let guard_source = test.temp_file(
    ctx,
    name: "guard-plain.xsh",
    contents: bytes.from_text("""let count = 3
guard let value = count else {
  exit 2
}
print \$value
"""),
  )?
  let guarded = run.capture --text --accept=[0, 1, 2] "xsht" check $guard_source
  let guard_report = guarded.stdout + guarded.stderr
  assert "check.guard-binding" in guard_report, guard_report
  assert "must produce a Result or an optional value" in guard_report, guard_report

  let if_source = test.temp_file(
    ctx,
    name: "if-let-plain.xsh",
    contents: bytes.from_text("""let count = 3
if let value = count {
  print \$value
}
"""),
  )?
  let conditional = run.capture --text --accept=[0, 1, 2] "xsht" check $if_source
  let if_report = conditional.stdout + conditional.stderr
  assert "check.irrefutable-pattern-condition" in if_report, if_report
}

test test_lint_rewrites_a_null_test_and_restating_binding_as_guard_let { |ctx|
  let source = """enum Mode { Fast, Slow }

type Executor = {name: Str, slots: Int}

type Context = {executor: Executor?, retries: Int?, label: Str?, mode: Mode?}

pure describe(context: Context) -> Str {
  guard context.executor != null else {
    # nothing to run on
    return "none"
  }
  let executor: Executor = context.executor
  return "zero" when context.retries == null
  let retries = context.retries
  guard context.mode != null else {
    return "modeless"
  }
  let mode: Mode = context.mode
  let speed = match mode {
    Fast => "fast",
    Slow => "slow",
  }
  for attempt in range(retries) {
    continue unless context.label != null
    let label = context.label
    return f"{label}{attempt} {speed}"
  }

  f"{executor.name}/{retries} {speed}"
}

pure widened(context: Context) -> Any {
  guard context.retries != null else {
    return null
  }
  let retries: Any = context.retries
  retries
}

print \${describe({executor: {name: "a", slots: 1}, retries: 2, label: "x", mode: Fast})}
print \${describe({executor: {name: "a", slots: 1}, retries: 2, label: null, mode: Slow})}
print \${describe({executor: {name: "a", slots: 1}, retries: null, label: "x", mode: Fast})}
print \${describe({executor: null, retries: 2, label: "x", mode: Fast})}
print \${describe({executor: {name: "a", slots: 1}, retries: 2, label: "x", mode: null})}
print \${widened({executor: null, retries: 2, label: null, mode: null}) is Int}
"""
  let before = test.expect(ctx, source, status: 0)?
  assert before.stdout == "x0 fast\na/2 slow\nzero\nnone\nmodeless\ntrue\n"
  let candidate = test.temp_file(ctx, name: "null-tests.xsh", contents: bytes.from_text(source))?

  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-optional-binding $candidate
  let report = linted.stdout + linted.stderr
  assert report.split("warn[lint.prefer-optional-binding]").len() == 6, report

  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-optional-binding $candidate
  assert fixed.status.exited_with(0), fixed.stdout + fixed.stderr
  let rewritten = candidate.read_text()?

  # A guard keeps its block, comments included; an annotation that only
  # restates the narrowed type goes, and one the checker cannot equate stays.
  assert """  guard let executor: Executor = context.executor else {
    # nothing to run on
    return "none"
  }
  guard let retries = context.retries else {
    return "zero"
  }
  guard let mode = context.mode else {
    return "modeless"
  }
  let speed""" in rewritten, rewritten
  assert """    guard let label = context.label else {
      continue
    }
    return f"{label}{attempt} {speed}\"""" in rewritten, rewritten
  assert """  guard let retries: Any = context.retries else {
    return null
  }
  retries""" in rewritten, rewritten
  assert "!= null" not in rewritten, rewritten
  assert "== null" not in rewritten, rewritten

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
}

# Each shape below differs from a null test plus restating binding in a way
# that makes the one-statement rewrite wrong or unproven.
test test_lint_leaves_other_null_tests_alone { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "other-null-tests.xsh",
    contents: bytes.from_text("""type Context = {retries: Int?, limit: Int?, first: Int?, second: Int?, count: Int?}

proc next(context: Context) -> Int? {
  context.retries
}

proc untouched(context: Context) -> Int {
  # The binding asks for the optional itself.
  guard context.retries != null else {
    return 0
  }
  let kept: Int? = context.retries

  # A statement runs between the test and the binding.
  guard context.limit != null else {
    return 1
  }
  print "checked"
  let limit = context.limit

  # The binding names a different optional.
  return 2 when context.first == null
  let other = context.second

  # A call may answer differently the second time.
  return 3 when next(context) == null
  let again = next(context)

  # A mutable binding is not what `guard let` declares.
  return 4 when context.count == null
  var count = context.count
  count += (kept ?? 0) + limit + (other ?? 0) + (again ?? 0)
  count
}

print \${untouched({retries: 1, limit: 2, first: 3, second: 4, count: 5})}
"""),
  )?
  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-optional-binding $candidate
  let report = linted.stdout + linted.stderr
  assert linted.status.exited_with(0), report
  assert "lint.prefer-optional-binding" not in report, report
}

test test_lint_withholds_the_fix_when_a_comment_would_be_lost { |ctx|
  let source = """type Context = {retries: Int?}

pure attempts(context: Context) -> Int {
  return 0 when context.retries == null # nothing configured
  let retries = context.retries
  retries
}

print \${attempts({retries: 2})}
"""
  let candidate = test.temp_file(ctx, name: "commented-null-test.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text --accept=[0, 1] "xsht" lint --fix --only lint.prefer-optional-binding $candidate
  let report = fixed.stdout + fixed.stderr
  assert "lint.prefer-optional-binding" in report, report
  assert "needs a manual rewrite" in report, report
  assert candidate.read_text()? == source
}
