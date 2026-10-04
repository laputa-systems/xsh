pure named_argument_values(first: Int, second: Int, third = 30) -> List[Int] {
  [first, second, third]
}

test test_named_argument_puns_use_lexical_values {
  let first = 10
  let second = 20
  let third = 40
  assert named_argument_values(first:, second:) == [10, 20, 30]
  assert named_argument_values(1, second:, third:) == [1, 20, 40]
  assert named_argument_values(
    first:,
    second: second + 1,
    third:,
  ) == [10, 21, 40]
}

test test_named_argument_puns_preserve_source_order_and_effects { |ctx|
  let executed = test.run_script(
    ctx,
    """proc marked(label: Str, value: Int) -> Int {
  print $label
  return value
}

pure combine(first: Int, second: Int, third: Int) -> Int {
  first + second + third
}

let second = 20
let result = combine(first: marked("first", 10), second:, third: marked("third", 30))
print $result
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """first
third
60
"""
}

test test_named_argument_puns_keep_resolution_and_call_errors { |ctx|
  let missing = test.run_script(
    ctx,
    """pure accept(value: Int) -> Int { value }
let config = {value: 42}
let result = accept(value:)
print $result
""",
  )?
  {
    let assertion_condition = ! missing.success
    let assertion_message = missing.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.unresolved-name" in missing.stderr
  assert "accept(value:)" in missing.stderr

  let duplicate = test.run_script(
    ctx,
    """pure accept(value: Int) -> Int { value }
let value = 42
let result = accept(value:, value: 2)
print $result
""",
  )?
  {
    let assertion_condition = ! duplicate.success
    let assertion_message = duplicate.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.named-arg" in duplicate.stderr
  assert "parameter `value` supplied more than once" in duplicate.stderr

  let wrong_type = test.run_script(
    ctx,
    """pure accept(value: Int) -> Int { value }
let value = "text"
let result = accept(value:)
print $result
""",
  )?
  {
    let assertion_condition = ! wrong_type.success
    let assertion_message = wrong_type.stderr
    assert assertion_condition, assertion_message
  }
  assert "Int" in wrong_type.stderr
  assert "Str" in wrong_type.stderr
}

test test_named_argument_puns_apply_to_module_and_method_calls { |ctx|
  let name = "punned-temp.xsh"
  let contents = b"punning"
  let candidate = test.temp_file(ctx, name:, contents:)?
  assert candidate.read_bytes()? == contents
  let method_call = test.run_script(
    ctx,
    r"""let offset = 1
let length = 3
assert b"abcde".slice(offset:, length:) == b"bcd"
""",
  )?
  let {success: method_success, stderr: method_message, ..} = method_call
  assert method_success, method_message
}

test test_named_argument_pun_tooling_preserves_behavior_and_is_idempotent { |ctx|
  let source = p"tests/fixtures/syntax/valid/named-argument-pun-explicit.xsh".read_text()?
  let before = test.run_script(ctx, source)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = before
    assert assertion_condition, assertion_message
  }
  let candidate = test.temp_file(ctx, name: "named-pun-fix.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --fix $candidate ?
  {
    let assertion_condition = first.status.exited_with(0)
    let assertion_message = first.stderr
    assert assertion_condition, assertion_message
  }
  let fixed = candidate.read_text()?
  assert "accept(value:)" in fixed
  assert "# Preserve this comment." in fixed
  let after = test.run_script(ctx, fixed)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = after
    assert assertion_condition, assertion_message
  }
  assert after.stdout == before.stdout
  let second = run.capture --text "xsht" lint --fix $candidate ?
  {
    let assertion_condition = second.status.exited_with(1)
    let assertion_message = second.stderr
    assert assertion_condition, assertion_message
  }
  assert "lint.prefer-named-argument-pun" in second.stderr
  assert candidate.read_text()? == fixed
  let formatted = run.capture --text "xsht" fmt --check $candidate ?
  {
    let assertion_condition = formatted.status.exited_with(0)
    let assertion_message = formatted.stderr
    assert assertion_condition, assertion_message
  }
}

test test_named_argument_pun_fixes_shared_import_once { |ctx|
  let root = test.temp_dir(ctx, name: "named-pun-shared")?
  let helper = fp"{root}/helper.xsh"
  helper.write_atomic(p"tests/fixtures/syntax/valid/named-argument-pun-module.xsh".read_text()?)?
  let first_entry = fp"{root}/first.xsh"
  let second_entry = fp"{root}/second.xsh"
  first_entry.write_atomic(r"""use helper
print ${helper.relay(1)}
""")?
  second_entry.write_atomic(r"""use helper
print ${helper.relay(2)}
""")?
  let applied = run.capture --text "xsht" lint --fix $root ?
  {
    let assertion_condition = applied.status.exited_with(0)
    let assertion_message = applied.stderr
    assert assertion_condition, assertion_message
  }
  let fixed = helper.read_text()?
  assert "accept(value:)" in fixed
  let repeated = run.capture --text "xsht" lint --fix $root ?
  {
    let assertion_condition = repeated.status.exited_with(0)
    let assertion_message = repeated.stderr
    assert assertion_condition, assertion_message
  }
  assert helper.read_text()? == fixed
  let checked = run.capture --text "xsht" check $root ?
  {
    let assertion_condition = checked.status.exited_with(0)
    let assertion_message = checked.stderr
    assert assertion_condition, assertion_message
  }
}
