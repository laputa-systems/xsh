error OptionalPostfixError = Failed(message: Str) : InvalidData

type OptionalPostfixServer = {host: Str?}
type OptionalPostfixConfig = {server: OptionalPostfixServer?}

proc test_optional_method_skips_arguments_and_preserves_fallback() [error] {
  let absent: Str? = null
  test.eq(absent?.trim() ?? "default", "default")?
  test.eq(absent?.replace("x", "y") ?? "default", "default")?
  let present: Str? = "  label  "
  test.eq(present?.trim() ?? "default", "label")?
}

proc test_optional_index_and_slice() [error] {
  let absent: List[Int]? = null
  test.eq(absent?[0] ?? -1, -1)?
  test.eq(absent?[0..2] ?? [], [])?
  let present: List[Int]? = [1, 2, 3]
  test.eq(present?[1] ?? -1, 2)?
  test.eq(present?[1..] ?? [], [2, 3])?
  let text: Str? = "αβγ"
  test.eq(text?[1..2] ?? "", "β")?
}

proc test_optional_postfix_evaluation_order(ctx: TestContext) [fs, error] {
  let output = test.run_script(ctx, """
proc absent() [io] -> Str? { print "receiver"; return null }
proc present() [io] -> Str? { print "present"; return "x" }
proc argument() [io] -> Str { print "argument"; return "x" }
proc bound(value: Int) [io] -> Int { print $value; return value }
proc values() [io] -> List[Int]? { print "list"; return [1, 2, 3] }
print (absent()?.replace(from: argument(), to: argument()) ?? "absent")
print (present()?.replace(from: argument(), to: argument()) ?? "absent")
let empty: List[Int]? = null
print (empty?[bound(0)] ?? -1)
let skipped = (empty?[bound(0)..bound(2)] ?? []).len()
print $skipped
let reached = (values()?[bound(1)..bound(3)] ?? []).len()
print $reached
""", [], {}, b"", "optional-order.xsh")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, """receiver
absent
present
argument
argument
x
-1
0
list
1
3
2
""")?
}

proc test_optional_result_layers_and_outer_propagation() [error] {
  let present: Str? = "42"
  let absent: Str? = null
  test.eq((present?.parse_int() ?? Ok(0))?, 42)?
  test.eq((absent?.parse_int() ?? Ok(0))?, 0)?
  let bad: Str? = "bad"
  test.error_kind(bad?.parse_int() ?? Ok(0), "parse-int")?
  let wrapped: Result[List[Int]] = Ok([3, 4, 5])
  test.eq(wrapped?[1], 4)?
  test.eq(wrapped?[1..], [4, 5])?
  let nested: Result[Str?] = Ok(" label ")
  test.eq(nested? ?.trim() ?? "default", "label")?
}

proc test_optional_non_null_errors_and_explicit_hops(ctx: TestContext) [fs, error] {
  let empty = test.run_script(ctx, "let values: List[Int]? = []\nlet item = values?[0] ?? 0\nprint $item\n", [], {}, b"", "optional-empty.xsh")?
  test.ok(!empty.success)?
  test.ok(empty.stderr.contains("index"), empty.stderr)?
  let any = test.run_script(ctx, "let value: Any = null\nprint value?.trim()\n", [], {}, b"", "optional-any.xsh")?
  test.ok(!any.success)?
  test.ok(any.stderr.contains("check.null-safe-field"), any.stderr)?
  let mixed = test.run_script(ctx, "let value: Str? = null\nlet wrapped = value?.parse_int()\nprint wrapped?\n", [], {}, b"", "optional-mixed.xsh")?
  test.ok(!mixed.success)?
  test.ok(mixed.stderr.contains("check.try-result"), mixed.stderr)?
}

proc test_result_postfix_propagation_skips_index_on_failure(ctx: TestContext) [fs, error] {
  let output = test.run_script(ctx, """
error InputError = Failed(message: Str) : InvalidData
proc argument() [io] -> Int { print "index"; return 0 }
proc read() [io, error] -> Result[Int] {
  let input: Result[List[Int]] = Err(InputError.Failed("failed"))
  return input?[argument()]
}
match read() {
  Ok(_) => print "unexpected"
  Err(_) => print "failed"
}
""", [], {}, b"", "result-index-failure.xsh")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, """failed
""")?
}

proc test_optional_fields_guard_each_hop_and_flatten_null_layers() [error] {
  let absent: OptionalPostfixConfig? = null
  let missing_server: OptionalPostfixConfig? = {server: null}
  let missing_host: OptionalPostfixConfig? = {server: {host: null}}
  let present: OptionalPostfixConfig? = {server: {host: " label "}}
  test.eq(absent?.server?.host?.trim() ?? "default", "default")?
  test.eq(missing_server?.server?.host?.trim() ?? "default", "default")?
  test.eq(missing_host?.server?.host?.trim() ?? "default", "default")?
  test.eq(present?.server?.host?.trim() ?? "default", "label")?
  let name: Str? = null
  let selected: Bool? = name?.contains("x")
  test.eq(selected ?? false, false)?
}

proc test_optional_postfix_null_branch_differential_witness(ctx: TestContext) [fs, error] {
  let before = test.run_script(ctx, """
proc fallback() [io] -> Str { print "fallback"; return "default" }
proc argument() [io] -> Str { print "argument"; return "x" }
proc label(name: Str?) [io] -> Str {
  return if name == null { fallback() } else { name.replace(argument(), "y") }
}
print label(null)
print label("x")
""", [], {}, b"", "optional-before.xsh")?
  let after = test.run_script(ctx, """
proc fallback() [io] -> Str { print "fallback"; return "default" }
proc argument() [io] -> Str { print "argument"; return "x" }
proc label(name: Str?) [io] -> Str {
  return name?.replace(argument(), "y") ?? fallback()
}
print label(null)
print label("x")
""", [], {}, b"", "optional-after.xsh")?
  test.ok(before.success, before.stderr)?
  test.ok(after.success, after.stderr)?
  test.eq(after.stdout, before.stdout)?
  test.eq(after.stdout, """fallback
default
argument
y
""")?
}

proc test_optional_runtime_record_fields_preserve_result_layers() [error] {
  let failure: Error? = OptionalPostfixError.Failed("42")
  test.eq((failure?.message?.parse_int() ?? Ok(0))?, 42)?
  let absent: Error? = null
  test.eq((absent?.message?.parse_int() ?? Ok(0))?, 0)?
  let handle: ProcessHandle? = null
  test.eq((handle?.command?.parse_int() ?? Ok(0))?, 0)?
}
