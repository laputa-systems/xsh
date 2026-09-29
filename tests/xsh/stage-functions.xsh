test test_stage_functions_use_one_item_calls_and_per_call_defaults [error] { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "0\ncall\ncall\n11\n12\n1\n4\ntrue\ntrue\n")?
}

test test_stage_functions_supply_independent_aggregate_defaults [error] { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "1\n1\n")?
}

test test_stage_functions_cover_keys_sinks_named_configuration_and_results [error] { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "direction\n1\n3\n2\n2\nseen:1\nseen:2\ntrue\nseen:3\ntrue\ntrue\n")?
}

test test_stage_functions_short_circuit_and_cancel_child_cleanup [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc cleanup() [] { print "cleanup" }
stream numbers() [] -> Stream[Int] {
  defer cleanup()
  print "pull:1"
  yield 1
  print "pull:2"
  yield 2
  print "pull:3"
  yield 3
}
proc predicate(item: Int) [] -> Bool { print f"test:${item}"; return item == 2 }
proc main() [] {
  print ${numbers() |> any(predicate)}
  print ${numbers() |> all(predicate)}
}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull:1\ntest:1\npull:2\ntest:2\ncleanup\ntrue\npull:1\ntest:1\ncleanup\nfalse\n")?
}

test test_stage_functions_for_break_matches_explicit_wrapper_pull_order [error] { |ctx|
  for body in ["map(add)", "map { |item| add(item) }"] {
    let output = test.run_script(ctx, r"""
proc cleanup() [] { print "cleanup" }
stream numbers() [] -> Stream[Int] { defer cleanup(); print "pull:1"; yield 1; print "pull:2"; yield 2 }
proc add(item: Int, amount: Int = 10) [] -> Int { print "call"; return item + amount }
proc main() [] {
  for value in (numbers() |> """ + body + r""") {
    print $value
    break
  }
}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "pull:1\ncall\n11\ncleanup\n")?
  }
}

test test_stage_functions_side_effect_failure_and_late_source_failure [error] { |ctx|
  for stage in ["each", "tee"] {
    let output = test.run_script(ctx, r"""
proc cleanup() [] { print "cleanup" }
stream numbers() [] -> Stream[Int] { defer cleanup(); yield 1; yield 2; print "unreached"; yield 3 }
proc observe(item: Int) [] -> Result[Unit] {
  print f"seen:${item}"
  if item == 2 { return error.fail("sink failed") }
  return Ok()
}
proc main() [error] { let _ = numbers() |> """ + stage + "(observe) }\n")?
    test.ok(! output.success, output.stderr)?
    test.eq(output.stdout, "seen:1\nseen:2\ncleanup\n")?
    test.contains(output.stderr, "sink failed", output.stderr)?
  }
  let late = test.run_script(ctx, r"""
proc cleanup() [] { print "cleanup" }
stream numbers() [error] -> Stream[Int] { defer cleanup(); yield 1; let _ = "late failure".parse_int()? }
proc observe(item: Int) [] -> Int { print f"seen:${item}"; return item }
proc main() [error] { let _ = numbers() |> map(observe) }
""")?
  test.ok(! late.success, late.stderr)?
  test.eq(late.stdout, "seen:1\ncleanup\n")?
}

test test_stage_functions_reject_erasure_shadowing_partial_methods_and_bad_contracts [error] { |ctx|
  for source in [
    "pure f(item: Int) -> Int { item }\nproc main() [] { let f: Pure = f; let _ = [1] |> map(f) }",
    "pure f(item: Int) -> Int { item }\nproc apply(f: Pure) [] { let _ = [1] |> map(f) }",
    "proc apply(f: Any) [] { let _ = [1] |> map(f) }",
    "pure f(item: Int) -> Int { item }\nproc factory() [] -> Pure { return f }\nlet _ = [1] |> map(factory())",
    "let text = \"value\"\nlet _ = [\"a\"] |> map(text.lower)",
    "pure f(item: Int, required: Int) -> Int { item + required }\nlet _ = [1] |> map(f)",
    "pure f(item: Str) -> Str { item }\nlet _ = [1] |> map(f)",
    "pure f(item: Int) -> Int { item }\nlet _ = [1] |> where(f)",
    "pure f(item: Int) -> Bool { true }\nlet _ = [1] |> map(f, block: f)",
    "pure f(item: Int) -> Int { item }\nlet _ = [1] |> map(f) { |item| item }",
    "pure f(item: Int) -> Int { item }\nlet _ = [1] |> par-map(f)",
    "let _ = [1] |> map(_)",
    "proc f(item: Int) [env] -> Int { let _ = env.get(\"HOME\"); return item }\nproc main() [] { let _ = [1] |> map(f) }",
  ] {
    let output = test.run_script(ctx, source + "\n")?
    test.ok(! output.success, source)?
    test.ok(output.stderr != "", source)?
  }
}

test test_stage_functions_keep_qualified_import_identity_and_defaults [fs, error] { |ctx|
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "5\n[a\n")?
}

test test_stage_functions_select_standard_overloads_from_the_item_type [fs, error] { |ctx|
  let file = test.temp_file(ctx, name: "digest-input", contents: b"abc")?
  let output = test.run_script(ctx, r"""
use hash
proc main(...argv: List[Str]) [fs, error] {
  let byte_digests = [b"abc"] |> map(hash.md5)
  let file_digests: List[Result[Digest]] = [Path(argv[0])] |> map(hash.md5)
  print byte_digests[0].hex()
  let digest = file_digests[0]?
  print digest.hex()
}
""", [file.display()])?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "900150983cd24fb0d6963f7d28e17f72\n900150983cd24fb0d6963f7d28e17f72\n")?
  let ambiguous = test.run_script(ctx, "use hash\nproc apply(items: List[Any]) [fs] { let _ = items |> map(hash.md5) }\n")?
  test.ok(! ambiguous.success, ambiguous.stderr)?
  test.contains(ambiguous.stderr, "check.stream-callable-signature", ambiguous.stderr)?
}

test test_stage_functions_tooling_fixes_only_transparent_wrappers [fs, process, error] { |ctx|
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
  test.contains(diagnosed.stderr, "lint.stage-callable", diagnosed.stderr)?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = candidate.read_text()?
  test.contains(fixed, "|> map(increment)", fixed)?
  test.contains(fixed, "sort-by(desc: true, block: increment)", fixed)?
  test.contains(fixed, "increment(item, 2)", fixed)?
  test.contains(fixed, "result(item)?", fixed)?
  test.contains(fixed, "# Keep the reason visible.", fixed)?
  let output = test.run_script(ctx, fixed)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "2\n")?
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  test.eq(candidate.read_text()?, fixed)?
  let broken = source + "missing_name()\n"
  let invalid = test.temp_file(ctx, name: "stage-function-invalid.xsh", contents: bytes.from_text(broken))?
  let refused = run.capture --text "xsht" lint --fix $invalid ?
  test.ok(! refused.status.exited_with(0), refused.stderr)?
  test.contains(refused.stderr, "missing_name", refused.stderr)?
  test.contains(invalid.read_text()?, "|> map {", refused.stderr)?
}
