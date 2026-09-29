error PatternLoopError = Done(detail: Str) : NotFound
type PatternBranchTag = SelectedBranch(Int) | OtherBranch

pure pattern_loop_subject(index: Int) -> Result[Int] {
  if index < 3 { Ok(index) } else { Err(PatternLoopError.Done(detail: "done")) }
}

pure pattern_conditional_label(outcome: Result[Int]) -> Str {
  if let Ok(value) = outcome {
    let label = f"${value}"
    label
  } else {
    "missing"
  }
}

proc test_pattern_conditionals_bind_immutable_branch_payloads() [error] {
  let outcome: Result[Int] = Ok(7)
  let value = "outer"
  if let Ok(value) = outcome {
    test.eq(value, 7)?
  } else {
    test.eq(value, "outer")?
  }
  test.eq(value, "outer")?
  if let SelectedBranch(value) = SelectedBranch(7) {
    test.eq(value, 7)?
  }
  test.eq(value, "outer")?
  if let Err(_) = outcome {
    test.fail("unexpected error branch")?
  } else if let Ok(value) = outcome {
    test.eq(value, 7)?
  } else {
    test.fail("expected matching branch")?
  }
  if let Ok(outcome) = outcome {
    test.eq(outcome, 7)?
  }
  test.eq(outcome, Ok(7))?
}

proc test_pattern_conditionals_produce_values_and_keep_literal_results() [error] {
  let outcome: Result[Int] = Ok(9)
  let selected = if let Ok(value) = outcome { value + 1 } else { 0 }
  test.eq(selected, 10)?
  let accepted = if let Ok(value) = outcome { value < 0 } else { true }
  test.eq(accepted, false)?
  test.eq(pattern_conditional_label(outcome), "9")?
  let missing: Result[Int] = Err(PatternLoopError.Done(detail: "done"))
  test.eq(pattern_conditional_label(missing), "missing")?
  if let Err(error) = missing {
    test.eq(error is PatternLoopError.Done, true)?
  } else {
    test.fail("Result was implicitly unwrapped")?
  }
}

proc test_pattern_loops_reevaluate_after_continue_and_keep_lexical_targets() [error] {
  var index = 0
  var total = 0
  while let Ok(value) = pattern_loop_subject(index) {
    index += 1
    if value == 1 { continue }
    total += value
  }
  test.eq(index, 3)?
  test.eq(total, 2)?
  while let Ok(value) = pattern_loop_subject(0) {
    test.eq(value, 0)?
    break
  }
}

proc test_pattern_conditionals_reject_irrefutable_and_leaking_captures(ctx: TestContext) [error] {
  for source in [
    "if let value = 1 { print $value }\n",
    "while let _ = 1 { break }\n",
    "if let [..tail] = [1] { print $tail }\n",
    "if let Ok(value) = Ok(1) { value = 2 }\n",
    "if let Ok(value) = Ok(1) {} else { print $value }\n",
    "if let Ok(value) = Ok(1) {}\nprint $value\n",
    "let value = if let Ok(payload) = Ok(1) { payload }\n",
    "let value = if let Ok(payload) = Ok(1) { payload } else { \"bad\" }\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.success, false, source)?
  }
}

pure pattern_condition_dynamic() -> Any { "hello" }

proc test_pattern_conditionals_reuse_nested_record_type_and_facet_patterns() [error] {
  let failure: PatternLoopError = PatternLoopError.Done(detail: "done")
  if let is NotFound = failure {
    test.eq(failure is NotFound, true)?
  } else {
    test.fail("facet pattern did not match")?
  }
  let outcome = Ok({message: "ready", code: 7})
  if let Ok({message: label, code: 7}) = outcome {
    test.eq(label, "ready")?
  } else {
    test.fail("nested pattern did not match")?
  }
  let label = "outer"
  if let Ok({message: label, code: 8}) = outcome {
    test.fail("partial nested pattern selected a branch")?
  } else {
    test.eq(label, "outer")?
  }
  test.eq(label, "outer")?
  let value = pattern_condition_dynamic()
  if let text is Str = value {
    test.eq(text.upper(), "HELLO")?
  }
  if let _ is Str = value {
    test.eq(value.upper(), "HELLO")?
  }
}

proc test_pattern_conditionals_evaluate_once_and_cleanup_loop_defers(ctx: TestContext) [error] {
  let output = test.run_script(ctx, r"""
error Finish = Done(detail: Str)
proc subject(index: Int) -> Result[Int] {
  print $index
  if index < 2 { return Ok(index) }
  return Err(Finish.Done(detail: "done"))
}
proc cleanup(value: Int) { print cleanup $value }
var index = 0
if let Ok(value) = subject(5) { print unexpected $value } else { print mismatch }
while let Ok(value) = subject(index) {
  defer cleanup(value)
  index += 1
  continue
}
print finished
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "5\nmismatch\n0\ncleanup 0\n1\ncleanup 1\n2\nfinished\n")?
}

proc test_pattern_conditionals_propagate_explicit_subject_errors(ctx: TestContext) [error] {
  let output = test.run_script(ctx, r"""
error Finish = Done(detail: Str)
proc subject() -> Result[Int] { return Err(Finish.Done(detail: "subject failed")) }
if let 7 = subject()? { print selected } else { print unexpected }
""")?
  test.eq(output.success, false)?
  test.eq(output.stdout, "")?
  test.contains(output.stderr, "Finish.Done")?
  test.contains(output.stderr, "result.propagate")?
}

proc test_pattern_conditional_lint_and_formatter_fixes_are_stable(ctx: TestContext) [fs, process, error] {
  for source in [
    "let outcome = Ok(7)\nmatch outcome { Ok(value) => { print $value }, Err(_) => { print missing } }\n",
    "let outcome = Ok(7)\nlet selected = match outcome { Ok(value) => value + 1, Err(_) => 0 }\nprint $selected\n",
  ] {
    let candidate = test.temp_file(ctx, name: "pattern-conditional-fix.xsh", contents: bytes.from_text(source))?
    let fixed = run.capture --text "xsht" lint --fix $candidate ?
    test.ok(fixed.status.exited_with(0), fixed.stderr)?
    let first = candidate.read_text()?
    test.contains(first, "if let Ok(value)")?
    test.contains(first, "else")?
    let stable = run.capture --text "xsht" lint --fix $candidate ?
    test.ok(stable.status.exited_with(0), stable.stderr)?
    test.eq(candidate.read_text()?, first)?
    let formatted = run.capture --text "xsht" fmt $candidate ?
    test.ok(formatted.status.exited_with(0), formatted.stderr)?
    let checked = run.capture --text "xsht" fmt --check $candidate ?
    test.ok(checked.status.exited_with(0), checked.stderr)?
    let output = test.run_script(ctx, candidate.read_text()?)?
    test.ok(output.success, output.stderr)?
  }
}

proc test_pattern_conditional_lint_retains_comments_guards_and_error_bindings(ctx: TestContext) [fs, process, error] {
  for source in [
    "let outcome = Ok(7)\nmatch outcome {\n  # selected payload\n  Ok(value) => { print $value }, Err(_) => {}\n}\n",
    "let outcome = Ok(7)\nmatch outcome { Ok(value) if value > 0 => { print $value }, _ => {} }\n",
    "let outcome = Ok(7)\nmatch outcome { Ok(value) => { print $value }, Err(error) => { print $error } }\n",
  ] {
    let candidate = test.temp_file(ctx, name: "pattern-conditional-no-fix.xsh", contents: bytes.from_text(source))?
    let ignored = run.capture --text "xsht" lint --fix $candidate ?
    test.eq(candidate.read_text()?, source)?
  }
}

proc test_pattern_condition_formatter_retains_loop_and_multiline_syntax(ctx: TestContext) [fs, process, error] {
  let source = r"""let outcome = Ok(7)
while let
  Ok(value)
  = outcome {
  print $value
  break
}
"""
  let candidate = test.temp_file(ctx, name: "pattern-loop-format.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  test.ok(formatted.status.exited_with(0), formatted.stderr)?
  let first = candidate.read_text()?
  test.contains(first, "while let Ok(value) = outcome")?
  let checked = run.capture --text "xsht" fmt --check $candidate ?
  test.ok(checked.status.exited_with(0), checked.stderr)?
  let output = test.run_script(ctx, first)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7\n")?
}

proc test_pattern_conditionals_match_lists_and_bind_typed_remainders() [error] {
  let argv = ["build", "native", "debug"]
  let target = "outer"
  if let ["build", target, ..tail] = argv {
    test.eq(target.upper(), "NATIVE")?
    test.eq(tail, ["debug"])?
  } else {
    test.fail("list branch did not match")?
  }
  test.eq(target, "outer")?
  var remaining = [1, 2, 3]
  var total = 0
  while let [head, ..tail] = remaining {
    total += head
    remaining = tail
  }
  test.eq(total, 6)?
  test.eq(remaining, [])?
}

proc pattern_conditional_open_root(root_path: Path) [fs, error] -> Result[FsRoot] {
  return if let Ok(root) = fs.open_root(root_path) { root } else { fs.open_root(root_path)? }
}

proc test_pattern_conditionals_preserve_escaping_owned_resources(ctx: TestContext) [fs, error] {
  let root_path = test.temp_dir(ctx, name: "pattern-root")?
  let root = pattern_conditional_open_root(root_path)?
  fs.root_write(root, p"value", "retained")?
  test.eq(fs.root_read_text(root, p"value")?, "retained")?
  fs.close_root(root)?
}

type PatternSiblingValue = SiblingWord(Str) | SiblingNumber(Int)

pure pattern_sibling_label(subject: PatternSiblingValue) -> Str {
  match subject {
    SiblingWord(value) => value
    SiblingNumber(value) => f"${value}"
  }
}

proc test_pattern_conditionals_preserve_sibling_match_capture_reuse() [error] {
  test.eq(pattern_sibling_label(SiblingWord("word")), "word")?
  test.eq(pattern_sibling_label(SiblingNumber(7)), "7")?
}
