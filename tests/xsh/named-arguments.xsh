pure named_argument_values(first: Int, second: Int, third: Int = 30) -> List[Int] {
  [first, second, third]
}

test test_named_argument_puns_use_lexical_values [error] {
  let first = 10
  let second = 20
  let third = 40
  test.eq(named_argument_values(first:, second:), [10, 20, 30])?
  test.eq(named_argument_values(1, second:, third:), [1, 20, 40])?
  test.eq(
    named_argument_values(
      first:,
      second: second + 1,
      third:,
    ),
    [10, 21, 40],
  )?
}

test test_named_argument_puns_preserve_source_order_and_effects [error] { |ctx|
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
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "first\nthird\n60\n")?
}

test test_named_argument_puns_keep_resolution_and_call_errors [error] { |ctx|
  let missing = test.run_script(
    ctx,
    """pure accept(value: Int) -> Int { value }
let config = {value: 42}
let result = accept(value:)
print $result
""",
  )?
  test.ok(! missing.success, missing.stderr)?
  test.contains(missing.stderr, "check.unresolved-name")?
  test.contains(missing.stderr, "accept(value:)")?

  let duplicate = test.run_script(
    ctx,
    """pure accept(value: Int) -> Int { value }
let value = 42
let result = accept(value:, value: 2)
print $result
""",
  )?
  test.ok(! duplicate.success, duplicate.stderr)?
  test.contains(duplicate.stderr, "check.named-arg")?
  test.contains(duplicate.stderr, "parameter `value` supplied more than once")?

  let wrong_type = test.run_script(
    ctx,
    """pure accept(value: Int) -> Int { value }
let value = "text"
let result = accept(value:)
print $result
""",
  )?
  test.ok(! wrong_type.success, wrong_type.stderr)?
  test.contains(wrong_type.stderr, "Int")?
  test.contains(wrong_type.stderr, "Str")?
}

test test_named_argument_puns_apply_to_module_and_method_calls [fs, error] { |ctx|
  let name = "punned-temp.xsh"
  let contents = b"punning"
  let candidate = test.temp_file(ctx, name:, contents:)?
  test.eq(candidate.read_bytes()?, contents)?
  let offset = 1
  let length = 3
  test.eq(b"abcde".slice(offset:, length:), b"bcd")?
}

test test_named_argument_pun_tooling_preserves_behavior_and_is_idempotent [fs, process, error] { |ctx|
  let source = p"tests/fixtures/syntax/valid/named-argument-pun-explicit.xsh".read_text()?
  let before = test.run_script(ctx, source)?
  test.ok(before.success, before.stderr)?
  let candidate = test.temp_file(ctx, name: "named-pun-fix.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(first.status.exited_with(0), first.stderr)?
  let fixed = candidate.read_text()?
  test.contains(fixed, "accept(value:)")?
  test.contains(fixed, "# Preserve this comment.")?
  let after = test.run_script(ctx, fixed)?
  test.ok(after.success, after.stderr)?
  test.eq(after.stdout, before.stdout)?
  let second = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(second.status.exited_with(1), second.stderr)?
  test.contains(second.stderr, "lint.prefer-named-argument-pun")?
  test.eq(candidate.read_text()?, fixed)?
  let formatted = run.capture --text "xsht" fmt --check $candidate ?
  test.ok(formatted.status.exited_with(0), formatted.stderr)?
}

test test_named_argument_pun_fixes_shared_import_once [fs, process, error] { |ctx|
  let root = test.temp_dir(ctx, name: "named-pun-shared")?
  let helper = fp"${root}/helper.xsh"
  helper.write_atomic(p"tests/fixtures/syntax/valid/named-argument-pun-module.xsh".read_text()?)?
  let first_entry = fp"${root}/first.xsh"
  let second_entry = fp"${root}/second.xsh"
  first_entry.write_atomic(r"""use helper
print ${helper.relay(1)}
""")?
  second_entry.write_atomic(r"""use helper
print ${helper.relay(2)}
""")?
  let applied = run.capture --text "xsht" lint --fix $root ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = helper.read_text()?
  test.contains(fixed, "accept(value:)")?
  let repeated = run.capture --text "xsht" lint --fix $root ?
  test.ok(repeated.status.exited_with(0), repeated.stderr)?
  test.eq(helper.read_text()?, fixed)?
  let checked = run.capture --text "xsht" check $root ?
  test.ok(checked.status.exited_with(0), checked.stderr)?
}
