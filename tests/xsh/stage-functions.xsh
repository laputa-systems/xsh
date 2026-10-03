test test_stage_functions_use_one_item_calls_and_per_call_defaults { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc add(item: Int, amount: Int = 10) [] -> Int { print "call"; return item + amount }
pure positive(item: Int) -> Bool { item > 0 }
pure duplicate(item: Int) -> List[Int] { [item, item] }
proc main() [] {
  let empty = [] |> map(add)
  print empty.len()
  let values = [1, 2] |> map(add)
  print values[0]
  print values[1]
  print ${([-1, 0, 1] |> where(positive)).len()}
  print ${([1, 2] |> flat-map(duplicate)).len()}
  print ${([0, 1] |> any(positive))}
  print ${([1, 2] |> all(positive))}
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """0
call
call
11
12
1
4
true
true
"""
}

test test_stage_functions_supply_independent_aggregate_defaults { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc size(item: Int, values: List[Int] = []) [] -> Int {
  var copy = values
  copy = copy.push(item)
  return copy.len()
}
proc main() [] {
  let sizes = [1, 2] |> map(size)
  print sizes[0]
  print sizes[1]
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """1
1
"""
}

test test_stage_functions_cover_keys_sinks_named_configuration_and_results { |ctx|
  let output = test.run_script(
    ctx,
    r"""
pure key(item: Int) -> Int { 0 - item }
pure bucket(item: Int) -> Str { if item > 1 { "large" } else { "small" } }
proc observe(item: Int) [] { print f"seen:${item}" }
proc direction() [] -> Bool { print "direction"; return true }
pure result(item: Int) -> Result[Int] { Ok(item) }
proc main() [] {
  print ${([1, 3, 2] |> sort-by(block: key, desc: direction()))[0]}
  let options = {desc: false}
  print ${([1, 3, 2] |> sort-by(key, ...options))[0]}
  print ${([1, 1, 2] |> unique-by(key)).len()}
  let grouped = [1, 2, 3] |> group-by(bucket)
  print grouped.len()
  print ${([1, 2] |> tee(observe) |> map(block: result))[0] is Ok(_)}
  let _ = [3] |> each(observe)
  let patterns = ["a", "["] |> map(regex.compile)
  print ${patterns[0] is Ok(_)}
  print ${patterns[1] is Err(_)}
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """direction
1
3
2
2
seen:1
seen:2
true
seen:3
true
true
"""
}

test test_stage_functions_short_circuit_and_cancel_child_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc cleanup() [] { print "cleanup" }
stream numbers() [error] -> Stream[Int] {
  defer cleanup()
  print "pull:1"
  yield 1
  print "pull:2"
  yield 2
  print "pull:3"
  yield 3
}
proc predicate(item: Int) [] -> Bool { print f"test:${item}"; return item == 2 }
proc main() [error] {
  print ${numbers() |> any(predicate)}
  print ${numbers() |> all(predicate)}
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pull:1
test:1
pull:2
test:2
cleanup
true
pull:1
test:1
cleanup
false
"""
}

test test_stage_functions_for_break_matches_explicit_wrapper_pull_order { |ctx|
  for body in ["map(add)", "map { |item| add(item) }"] {
    let output = test.run_script(
      ctx,
      r"""
proc cleanup() [] { print "cleanup" }
stream numbers() [error] -> Stream[Int] { defer cleanup(); print "pull:1"; yield 1; print "pull:2"; yield 2 }
proc add(item: Int, amount: Int = 10) [] -> Int { print "call"; return item + amount }
proc main() [error] {
  for value in (numbers() |> """ + body + r""") {
    print $value
    break
  }
}
""",
    )?
    {
      let {success: assertion_condition, stderr: assertion_message, ..} = output
      assert assertion_condition, assertion_message
    }
    assert output.stdout == """pull:1
call
11
cleanup
"""
  }
}

test test_stage_functions_side_effect_failure_and_late_source_failure { |ctx|
  for stage in ["each", "tee"] {
    let output = test.run_script(
      ctx,
      r"""
proc cleanup() [] { print "cleanup" }
stream numbers() [error] -> Stream[Int] { defer cleanup(); yield 1; yield 2; print "unreached"; yield 3 }
proc observe(item: Int) [] -> Result[Unit] {
  print f"seen:${item}"
  if item == 2 { return error.fail("sink failed") }
  return Ok()
}
proc main() [error] { let _ = numbers() |> """ + stage + """(observe) }
""",
    )?
    {
      let assertion_condition = ! output.success
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
    assert output.stdout == """seen:1
seen:2
cleanup
"""
    {
      let assertion_condition = "sink failed" in output.stderr
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }

  let late = test.run_script(
    ctx,
    r"""
proc cleanup() [] { print "cleanup" }
stream numbers() [error] -> Stream[Int] { defer cleanup(); yield 1; let _ = "late failure".parse_int()? }
proc observe(item: Int) [] -> Int { print f"seen:${item}"; return item }
proc main() [error] { let _ = numbers() |> map(observe) }
""",
  )?
  {
    let assertion_condition = ! late.success
    let assertion_message = late.stderr
    assert assertion_condition, assertion_message
  }
  assert late.stdout == """seen:1
cleanup
"""
}

test test_stage_functions_reject_erasure_shadowing_partial_methods_and_bad_contracts { |ctx|
  for source in [
    """pure f(item: Int) -> Int { item }
proc main() [] { let f: Pure = f; let _ = [1] |> map(f) }""",
    """pure f(item: Int) -> Int { item }
proc apply(f: Pure) [] { let _ = [1] |> map(f) }""",
    "proc apply(f: Any) [] { let _ = [1] |> map(f) }",
    """pure f(item: Int) -> Int { item }
proc factory() [] -> Pure { return f }
let _ = [1] |> map(factory())""",
    """let text = "value"
let _ = ["a"] |> map(text.lower)""",
    """pure f(item: Int, required: Int) -> Int { item + required }
let _ = [1] |> map(f)""",
    """pure f(item: Str) -> Str { item }
let _ = [1] |> map(f)""",
    """pure f(item: Int) -> Int { item }
let _ = [1] |> where(f)""",
    """pure f(item: Int) -> Bool { true }
let _ = [1] |> map(f, block: f)""",
    """pure f(item: Int) -> Int { item }
let _ = [1] |> map(f) { |item| item }""",
    """pure f(item: Int) -> Int { item }
let _ = [1] |> par-map(f)""",
    "let _ = [1] |> map(_)",
    """proc f(item: Int) [env] -> Int { let _ = env.get("HOME"); return item }
proc main() [] { let _ = [1] |> map(f) }""",
  ] {
    let output = test.run_script(ctx, source + "\n")?
    {
      let assertion_condition = ! output.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = output.stderr != ""
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
  }
}

test test_stage_functions_keep_qualified_import_identity_and_defaults { |ctx|
  let directory = test.temp_dir(ctx, name: "stage-call-import")?
  fp"${directory}/helpers.xsh".write(r"""##! Named stage functions.
## Add a default amount to the item.
export pure add(item: Int, amount: Int = 4) -> Int { item + amount }
## A default amount is applied to strings too.
export pure surround(item: Str, prefix: Str = "[") -> Str { prefix + item }
""")?
  let script = fp"${directory}/main.xsh"
  script.write(r"""use helpers as helpers
let values = [1, 2] |> map(helpers.add)
print values[0]
print ${(["a"] |> map(helpers.surround))[0]}
""")?
  let output = test.run_script(ctx, script.read_text()?, [], {XSH_MODULE_PATH: directory.display()})?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """5
[a
"""
}

test test_stage_functions_select_standard_overloads_from_the_item_type { |ctx|
  let file = test.temp_file(ctx, name: "digest-input", contents: b"abc")?
  let output = test.run_script(
    ctx,
    r"""
use hash
proc main(...argv: List[Str]) [fs, error] {
  let byte_digests = [b"abc"] |> map(hash.md5)
  let file_digests: List[Result[Digest]] = [Path(argv[0])] |> map(hash.md5)
  print byte_digests[0].hex()
  let digest = file_digests[0]?
  print digest.hex()
}
""",
    [file.display()],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """900150983cd24fb0d6963f7d28e17f72
900150983cd24fb0d6963f7d28e17f72
"""
  let ambiguous = test.run_script(
    ctx,
    """use hash
proc apply(items: List[Any]) [fs] { let _ = items |> map(hash.md5) }
""",
  )?
  {
    let assertion_condition = ! ambiguous.success
    let assertion_message = ambiguous.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "check.stream-callable-signature" in ambiguous.stderr
    let assertion_message = ambiguous.stderr
    assert assertion_condition, assertion_message
  }
}

test test_stage_functions_tooling_fixes_only_transparent_wrappers { |ctx|
  let source = r"""pure increment(item: Int, amount: Int = 1) -> Int { item + amount }
pure result(item: Int) -> Result[Int] { Ok(item) }
proc main() [error] {
  let values = [1, 2] |> map { |item| increment(item) }
  print values[0]
  let _ = [1] |> sort-by(desc: true) { |item| increment(item) }
  let _ = [1] |> map { |item| increment(item, 2) }
  let _ = [1] |> map { |item| result(item)? }
  let _ = [1] |> map { |item| # Keep the reason visible.
    increment(item)
  }
}
"""
  let candidate = test.temp_file(ctx, name: "stage-function-fix.xsh", contents: bytes.from_text(source))?
  let diagnosed = run.capture --text "xsht" lint $candidate ?
  {
    let assertion_condition = "lint.stage-callable" in diagnosed.stderr
    let assertion_message = diagnosed.stderr
    assert assertion_condition, assertion_message
  }
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  {
    let assertion_condition = applied.status.exited_with(0)
    let assertion_message = applied.stderr
    assert assertion_condition, assertion_message
  }
  let fixed = candidate.read_text()?
  {
    let assertion_condition = "|> map(increment)" in fixed
    let assertion_message = fixed
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "sort-by(desc: true, block: increment)" in fixed
    let assertion_message = fixed
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "increment(item, 2)" in fixed
    let assertion_message = fixed
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "result(item)?" in fixed
    let assertion_message = fixed
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "# Keep the reason visible." in fixed
    let assertion_message = fixed
    assert assertion_condition, assertion_message
  }
  let output = test.run_script(ctx, fixed)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """2
"""
  let _ = run.capture --text "xsht" lint --fix $candidate ?
  assert candidate.read_text()? == fixed
  let broken = source + """missing_name()
"""
  let invalid = test.temp_file(ctx, name: "stage-function-invalid.xsh", contents: bytes.from_text(broken))?
  let refused = run.capture --text "xsht" lint --fix $invalid ?
  {
    let assertion_condition = ! refused.status.exited_with(0)
    let assertion_message = refused.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "missing_name" in refused.stderr
    let assertion_message = refused.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "|> map {" in invalid.read_text()?
    let assertion_message = refused.stderr
    assert assertion_condition, assertion_message
  }
}
