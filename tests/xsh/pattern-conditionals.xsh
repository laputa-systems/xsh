error PatternLoopError = Done(detail: Str) : NotFound

pure pattern_loop_subject(index: Int) -> Result[Int] {
  if index < 3 {
    Ok(index)
  } else {
    Err(PatternLoopError.Done(detail: "done"))
  }
}

pure pattern_conditional_label(outcome: Result[Int]) -> Str {
  if let Ok(value) = outcome {
    let label = f"${value}"
    label
  } else {
    "missing"
  }
}

test test_pattern_conditionals_bind_immutable_branch_payloads { |ctx|
  let output = test.run_script(
    ctx,
    r"""enum PatternBranchTag { SelectedBranch(Int), OtherBranch }
proc witness() [error] {
  let outcome: Result[Int] = Ok(7)
  let value = "outer"
  if let Ok(value) = outcome {
    assert value == 7
  } else {
    assert value == "outer"
  }
  assert value == "outer"
  if let SelectedBranch(value) = SelectedBranch(7) {
    assert value == 7
  }
  assert value == "outer"
  if let Err(_) = outcome {
    test.fail("unexpected error branch")?
  } else if let Ok(value) = outcome {
    assert value == 7
  } else {
    test.fail("expected matching branch")?
  }
  if let Ok(outcome) = outcome {
    assert outcome == 7
  }
  assert outcome == Ok(7)
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_pattern_conditionals_produce_values_and_keep_literal_results {
  let outcome = Ok(9)
  let selected = if let Ok(value) = outcome { value + 1 } else { 0 }
  assert selected == 10
  let accepted = if let Ok(value) = outcome { value < 0 } else { true }
  assert accepted == false
  assert pattern_conditional_label(outcome) == "9"
  let missing: Result[Int] = Err(PatternLoopError.Done(detail: "done"))
  assert pattern_conditional_label(missing) == "missing"
  if let Err(error) = missing {
    assert error is PatternLoopError.Done == true
  } else {
    test.fail("Result was implicitly unwrapped")?
  }
}

test test_pattern_loops_reevaluate_after_continue_and_keep_lexical_targets {
  var index = 0
  var total = 0
  while let Ok(value) = pattern_loop_subject(index) {
    index += 1
    continue when value == 1
    total += value
  }

  assert index == 3
  assert total == 2
  while let Ok(value) = pattern_loop_subject(0) {
    assert value == 0
    break
  }
}

test test_pattern_conditionals_reject_irrefutable_and_leaking_captures { |ctx|
  for source in [
    """if let value = 1 { print $value }
""",
    """while let _ = 1 { break }
""",
    """if let [..tail] = [1] { print $tail }
""",
    """if let Ok(value) = Ok(1) { value = 2 }
""",
    """if let Ok(value) = Ok(1) {} else { print $value }
""",
    """if let Ok(value) = Ok(1) {}
print $value
""",
    """let value = if let Ok(payload) = Ok(1) { payload }
""",
    """let value = if let Ok(payload) = Ok(1) { payload } else { "bad" }
""",
  ] {
    let output = test.run_script(ctx, source)?
    {
      let assertion_actual = output.success
      let assertion_expected = false
      let assertion_message = source
      assert assertion_actual == assertion_expected, assertion_message
    }
  }
}

test test_pattern_conditionals_reuse_nested_record_type_and_facet_patterns { |ctx|
  let output = test.run_script(
    ctx,
    r"""error PatternLoopError = Done(detail: Str) : NotFound
pure pattern_condition_dynamic() -> Any { "hello" }
proc witness() [error] {
  let failure: PatternLoopError = PatternLoopError.Done(detail: "done")
  if let is NotFound = failure {
    assert failure is NotFound == true
  } else {
    test.fail("facet pattern did not match")?
  }
  let outcome = Ok({message: "ready", code: 7})
  if let Ok({message: label, code: 7}) = outcome {
    assert label == "ready"
  } else {
    test.fail("nested pattern did not match")?
  }
  let label = "outer"
  if let Ok({message: label, code: 8}) = outcome {
    test.fail("partial nested pattern selected a branch")?
  } else {
    assert label == "outer"
  }
  assert label == "outer"
  let value = pattern_condition_dynamic()
  if let text is Str = value {
    assert text.upper() == "HELLO"
  }
  if let _ is Str = value {
    assert value.upper() == "HELLO"
  }
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_pattern_conditionals_evaluate_once_and_cleanup_loop_defers { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """5
mismatch
0
cleanup 0
1
cleanup 1
2
finished
"""
}

test test_pattern_conditionals_propagate_explicit_subject_errors { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error Finish = Done(detail: Str)
proc subject() -> Result[Int] { return Err(Finish.Done(detail: "subject failed")) }
if let 7 = subject()? { print selected } else { print unexpected }
""",
  )?
  assert output.success == false
  assert output.stdout == ""
  assert "Finish.Done" in output.stderr
  assert "result.propagate" in output.stderr
}

test test_pattern_conditional_lint_and_formatter_fixes_are_stable { |ctx|
  for source in [
    """let outcome = Ok(7)
match outcome { Ok(value) => { print $value }, Err(_) => { print missing } }
""",
    """let outcome = Ok(7)
let selected = match outcome { Ok(value) => value + 1, Err(_) => 0 }
print $selected
""",
  ] {
    let candidate = test.temp_file(ctx, name: "pattern-conditional-fix.xsh", contents: bytes.from_text(source))?
    let fixed = run.capture --text "xsht" lint --fix $candidate ?
    {
      let assertion_condition = fixed.status.exited_with(0)
      let assertion_message = fixed.stderr
      assert assertion_condition, assertion_message
    }
    let first = candidate.read_text()?
    assert "if let Ok(value)" in first
    assert "else" in first
    let stable = run.capture --text "xsht" lint --fix $candidate ?
    {
      let assertion_condition = stable.status.exited_with(0)
      let assertion_message = stable.stderr
      assert assertion_condition, assertion_message
    }
    assert candidate.read_text()? == first
    let formatted = run.capture --text "xsht" fmt $candidate ?
    {
      let assertion_condition = formatted.status.exited_with(0)
      let assertion_message = formatted.stderr
      assert assertion_condition, assertion_message
    }
    let checked = run.capture --text "xsht" fmt --check $candidate ?
    {
      let assertion_condition = checked.status.exited_with(0)
      let assertion_message = checked.stderr
      assert assertion_condition, assertion_message
    }
    let output = test.run_script(ctx, candidate.read_text()?)?
    {
      let {success: assertion_condition, stderr: assertion_message, ..} = output
      assert assertion_condition, assertion_message
    }
  }
}

test test_pattern_conditional_lint_retains_comments_guards_and_error_bindings { |ctx|
  for source in [
    """let outcome = Ok(7)
match outcome {
  # selected payload
  Ok(value) => { print $value }, Err(_) => {}
}
""",
    """let outcome = Ok(7)
match outcome { Ok(value) if value > 0 => { print $value }, _ => {} }
""",
    """let outcome = Ok(7)
match outcome { Ok(value) => { print $value }, Err(error) => { print $error } }
""",
  ] {
    let candidate = test.temp_file(ctx, name: "pattern-conditional-no-fix.xsh", contents: bytes.from_text(source))?
    let _ = run.capture --text "xsht" lint --fix $candidate ?
    assert candidate.read_text()? == source
  }
}

test test_pattern_condition_formatter_retains_loop_and_multiline_syntax { |ctx|
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
  {
    let assertion_condition = formatted.status.exited_with(0)
    let assertion_message = formatted.stderr
    assert assertion_condition, assertion_message
  }
  let first = candidate.read_text()?
  assert "while let Ok(value) = outcome" in first
  let checked = run.capture --text "xsht" fmt --check $candidate ?
  {
    let assertion_condition = checked.status.exited_with(0)
    let assertion_message = checked.stderr
    assert assertion_condition, assertion_message
  }
  let output = test.run_script(ctx, first)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """7
"""
}

test test_pattern_conditionals_match_lists_and_bind_typed_remainders { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc witness() [error] {
  let argv = ["build", "native", "debug"]
  let target = "outer"
  if let ["build", target, ..tail] = argv {
    assert target.upper() == "NATIVE"
    assert tail == ["debug"]
  } else {
    test.fail("list branch did not match")?
  }
  assert target == "outer"
  var remaining = [1, 2, 3]
  var total = 0
  while let [head, ..tail] = remaining {
    total += head
    remaining = tail
  }
  assert total == 6
  assert remaining == []
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

proc pattern_conditional_open_root(root_path: Path) [fs, error] -> Result[FsRoot] {
  if let Ok(root) = fs.open_root(root_path) {
    root
  } else {
    fs.open_root(root_path)?
  }
}

test test_pattern_conditionals_preserve_escaping_owned_resources { |ctx|
  let root_path = test.temp_dir(ctx, name: "pattern-root")?
  let root = pattern_conditional_open_root(root_path)?
  root.write(p"value", "retained")?
  assert root.read_text(p"value")? == "retained"
  root.close()?
}

enum PatternSiblingValue { SiblingWord(Str), SiblingNumber(Int) }

pure pattern_sibling_label(subject: PatternSiblingValue) -> Str {
  match subject {
    SiblingWord(value) => value
    SiblingNumber(value) => f"${value}"
  }
}

test test_pattern_conditionals_preserve_sibling_match_capture_reuse {
  assert pattern_sibling_label(SiblingWord("word")) == "word"
  assert pattern_sibling_label(SiblingNumber(7)) == "7"
}

test test_pattern_conditionals_yield_from_stream_producers { |ctx|
  let output = test.run_script(
    ctx,
    r"""stream pairs(lines: List[Str]) -> Stream[Str] {
  for line in lines {
    if let [key, value] = line.split("=") {
      yield f"${key}:${value}"
    } else if let [single] = line.split("=") {
      if single == "stop" { break }
      yield f"${single}:-"
    }
  }
  var remaining = [1, 2]
  while let [head, ..rest] = remaining {
    remaining = rest
    yield f"rest:${head}"
  }
}
let all = pairs(["a=1", "b", "c=3"]) |> collect
let stopped = pairs(["a=1", "stop", "c=3"]) |> collect
let first = pairs(["a=1", "b=2"]) |> take(1)
print all.join(" ")
print stopped.join(" ")
print first.join(" ")
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """a:1 b:- c:3 rest:1 rest:2
a:1 rest:1 rest:2
a:1
"""
}
