type Captured = {status: Status, stdout: Str, stderr: Str}

# The two migrations to core syntax: `lint.prefer-in` for membership calls
# and `lint.core-assert` for assertion helpers in statement position.
const migrations = "lint.prefer-in,lint.core-assert"

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file` and returns what the two migrations report for
# it. The source has no check error, so every finding is on the report.
proc reported(file: Path, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let report = lint(file, ["--only", migrations])?
  assert "err[" not in report.stderr, report.stderr
  Ok(report.stderr)
}

# Migrates `source` in `file` and returns the formatted result, which must
# check and must have no migration left to apply.
proc migrated(file: Path, source: Str) [fs, process, env, error] -> Result[Str] {
  let _ = reported(file, source)?
  let _ = lint(file, ["--fix", "--only", migrations])?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let fixed = file.read_text()?
  let _ = lint(file, ["--fix", "--only", migrations])?
  assert file.read_text()? == fixed, f"migration should be idempotent: {fixed}"
  Ok(fixed)
}

# How many times `needle` occurs in `text`.
pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

# Whether the first `earlier` in `text` comes before the first `later`.
pure precedes(text: Str, earlier: Str, later: Str) -> Bool {
  let around = text.split(earlier)
  around.len() > 1 and later not in around[0] and later in text
}

test test_lint_migrates_removed_membership_using_checked_identity { |ctx|
  let root = test.temp_dir(ctx, name: "membership")?
  let fixed = migrated(
    fp"{root}/membership.xsh",
    r"""type Row = {present: Int}
proc probe(names: List[Str], name: Str, text: Str, source_path: Path, mapping: Map[Int], fields: Row) {
  if names.contains(name) {}
  if ! names.contains(name) {}
  if text.contains("needle") {}
  if source_path.display().contains("/") {}
  if mapping.has("key") {}
  if fields.has("present") {}
}
""",
  )?
  for expression in [
    "name in names",
    "name not in names",
    "\"needle\" in text",
    "\"/\" in source_path.display()",
    "\"key\" in mapping",
    "\"present\" in fields",
  ] {
    assert expression in fixed, f"missing {expression}: {fixed}"
  }
}

test test_lint_preserves_membership_operand_order_with_statement_bindings { |ctx|
  let root = test.temp_dir(ctx, name: "membership-order")?
  let fixed = migrated(
    fp"{root}/membership.xsh",
    r"""proc container() -> Result[Str] { "abc" }
proc item() -> Result[Str] { "b" }
proc main() {
  test.contains(container()?, item()?)?
}
""",
  )?
  assert precedes(fixed, "= container()?", "= item()?"), fixed
  assert " in membership_argument_0_" in fixed, fixed
}

test test_lint_diagnoses_unsafe_membership_inside_conditions_without_hoisting { |ctx|
  let root = test.temp_dir(ctx, name: "membership-unsafe")?
  let file = fp"{root}/membership.xsh"
  let source = r"""proc main(source_path: Path, names: List[Str]) [fs, error] {
  if source_path.read_text()?.contains("needle") {}
  if names.contains(source_path.read_text()?) {}
}
"""
  let report = reported(file, source)?
  assert count(report, "warn[lint.prefer-in]") == 2, report
  # Null-safe consumption requires an explicit manual decision, and a
  # mutable receiver read must retain its order: neither has a fix.
  assert "help: " not in report, report
  assert ":2:6\n" in report and ":3:6\n" in report, report
  let _ = lint(file, ["--fix", "--only", migrations])?
  assert file.read_text()? == source
}

test test_lint_migrates_assertion_helpers_only_in_statement_use { |ctx|
  let root = test.temp_dir(ctx, name: "assertion-helpers")?
  let file = fp"{root}/assertions.xsh"
  let source = r"""proc main(text: Str) {
  test.ok(true)?
  test.eq(1, 2)?
  test.ne(1, 2)
  let consumed = test.contains(text, "é")
  test.contains(text, "é", message: "custom")?
  let retained = test.eq(1, 2)
  let _ = consumed
  let _ = retained
}
"""
  # The membership rewrite keeps the custom message as a named argument;
  # the assertion rewrite of a later round turns it into the assert message.
  let report = reported(file, source)?
  assert "-> test.ok(\"é\" in text, message: \"custom\")\n" in report, report
  let fixed = migrated(file, source)?
  assert "1 == 2" in fixed, fixed
  assert "1 != 2" in fixed, fixed
  assert "let consumed = test.ok(\"é\" in text)" in fixed, fixed
  assert "assert \"é\" in text, \"custom\"" in fixed, fixed
  assert "let retained = test.eq(1, 2)" in fixed, fixed
}

test test_lint_composes_nested_unicode_membership_and_grouped_receivers { |ctx|
  let root = test.temp_dir(ctx, name: "membership-nested")?
  let fixed = migrated(
    fp"{root}/membership.xsh",
    r"""proc main(text: Str) {
  test.ok(! text.contains("é"))?
  test.eq((-3.5).abs(), 3.5)?
}
""",
  )?
  assert "\"é\" not in text" in fixed, fixed
  assert "(-3.5).abs() == 3.5" in fixed, fixed
}

test test_lint_migrates_package_nested_assertions_and_multiline_match_membership { |ctx|
  let root = test.temp_dir(ctx, name: "membership-match")?
  let fixed = migrated(
    fp"{root}/membership.xsh",
    r"""proc probe(configured: Str, result: Result[Str], expected: Str) {
  test.eq(configured.contains("menucmd"), false)?
  test.ok(configured.contains("termcmd"))?
  match result {
    Ok(_) => test.fail("unexpected success")?
    Err(problem) => test.contains(
      problem.message,
      f"repeats {expected}",
    )?
  }
  match result {
    Ok(_) => assert true
    Err(problem) => assert problem.message.contains("repeats")
  }
}
""",
  )?
  assert ".contains(" not in fixed, fixed
  assert "\"menucmd\" in configured" in fixed, fixed
  assert "\"termcmd\" in configured" in fixed, fixed
  assert "problem.message" in fixed, fixed
  assert "repeats {expected}" in fixed, fixed
}

test test_lint_migrates_explicitly_propagated_read_membership_without_null_safe_guessing { |ctx|
  let root = test.temp_dir(ctx, name: "membership-read")?
  let file = fp"{root}/membership.xsh"
  let fixed = migrated(
    file,
    r"""proc main(source_path: Path) [fs, error] {
  test.contains(source_path.read_text()?, "needle")?
  if (source_path.read_text()?).contains("needle") {}
}
""",
  )?
  assert ".contains(" not in fixed, fixed
  assert count(fixed, "source_path.read_text()?") == 2, fixed

  let unsafe = r"""proc main(source_path: Path) [fs, error] {
  if source_path.read_text()?.contains("needle") {}
}
"""
  let report = reported(file, unsafe)?
  assert count(report, "warn[lint.prefer-in]") == 1, report
  # Null-safe consumption must remain explicit.
  assert "help: " not in report, report
  let _ = lint(file, ["--fix", "--only", migrations])?
  assert file.read_text()? == unsafe
}

# The migrated assertion evaluates the taken arm's operands and message
# once, in source order, and fails with the author's message.
test test_lint_match_membership_snapshots_preserve_effects_and_failure { |ctx|
  let root = test.temp_dir(ctx, name: "membership-effects")?
  let program = r"""proc haystack() [io] -> Str { print 1; "abc" }
proc needle() [io] -> Str { print 2; "NEEDLE" }
proc message() [io] -> Str { print 3; "custom membership failure" }
proc skipped() [io] -> Str { print 9; "abc" }
proc main() {
  match true {
    true => test.contains(haystack(), needle(), message: message())?
    false => test.contains(skipped(), needle())?
  }
  print 4
}
"""
  for case in [{needle: "b", succeeds: true}, {needle: "missing", succeeds: false}] {
    let file = fp"{root}/membership-{case.needle}.xsh"
    let fixed = migrated(file, program.replace("NEEDLE", with: case.needle))?
    let traced = run.capture --text "xsht" trace $file
    assert traced.status.exited_with(0) == case.succeeds, f"{fixed}\n{traced.stderr}"
    if case.succeeds {
      assert traced.stdout == "1\n2\n3\n4\n", fixed
    } else {
      assert traced.stdout == "1\n2\n3\n", fixed
      assert "custom membership failure" in traced.stderr, traced.stderr
    }
  }
}

# A local already named like a generated binding does not capture one.
test test_lint_preserves_named_argument_order_and_hygiene { |ctx|
  let root = test.temp_dir(ctx, name: "assertion-hygiene")?
  let fixed = migrated(
    fp"{root}/assertions.xsh",
    r"""proc left() -> Result[Int] { 1 }
proc right() -> Result[Int] { 2 }
proc main() {
  let membership_argument_0_130 = 1
  test.eq(right: right()?, left: left()?)?
  let _ = membership_argument_0_130
}
""",
  )?
  assert precedes(fixed, "= right()?", "= left()?"), fixed
}

test test_lint_named_assertion_snapshots_preserve_runtime_source_order { |ctx|
  let root = test.temp_dir(ctx, name: "assertion-order")?
  for case in [{operation: "eq", right: "7"}, {operation: "ne", right: "8"}] {
    let source = "proc left() [io] -> Int { print 1; 7 }\nproc right() [io] -> Int { print 2; " + case.right + " }\nproc main() { test." + case.operation + "(right: right(), left: left())? }\n"
    let original = fp"{root}/{case.operation}-original.xsh"
    original.write(source)
    let before = run.capture --text "xsht" trace $original
    assert before.status.exited_with(0), before.stderr
    assert before.stdout == "2\n1\n", source

    let rewritten = fp"{root}/{case.operation}-fixed.xsh"
    let fixed = migrated(rewritten, source)?
    assert "membership_argument_0_" in fixed, fixed
    let after = run.capture --text "xsht" trace $rewritten
    assert after.status.exited_with(0), after.stderr
    assert after.stdout == before.stdout, fixed
  }
}

test test_lint_leaves_dynamic_and_comment_bearing_migrations_actionable { |ctx|
  let root = test.temp_dir(ctx, name: "membership-actionable")?
  let file = fp"{root}/membership.xsh"
  let dynamic = "proc main(value: Any) { test.contains(value, 1)? }\n"
  let report = reported(file, dynamic)?
  assert count(report, "warn[lint.prefer-in]") == 1, report
  assert "help: " not in report, report
  let _ = lint(file, ["--fix", "--only", migrations])?
  assert file.read_text()? == dynamic

  # The rewrite of an assertion whose operand holds a comment is offered
  # with the comment inside it.
  let commented = reported(file, "proc main() { test.ok(retry [] { # keep this reason\n true }?)? }\n")?
  assert "help: rewrite assertion -> assert retry [] { # keep this reason\n true }?\n" in commented, commented
}

test test_lint_migrates_set_negation_and_stream_item_membership { |ctx|
  let root = test.temp_dir(ctx, name: "membership-set")?
  let fixed = migrated(
    fp"{root}/membership.xsh",
    r"""proc probe(mapping: Map[Int], keys: List[Str], names: Set[Str]) {
  assert ! set.has(names, "missing")
  let present = keys |> where mapping.has(.)
  let absent = keys |> where ! mapping.has(.) and ! mapping.has("other")
  let _ = present
  let _ = absent
}
""",
  )?
  assert "\"missing\" not in names" in fixed, fixed
  assert "(.) in mapping" in fixed, fixed
  assert "(.) not in mapping" in fixed, fixed
}

test test_lint_does_not_rewrite_user_fields_named_contains_or_has { |ctx|
  let root = test.temp_dir(ctx, name: "membership-user-fields")?
  let report = reported(
    fp"{root}/membership.xsh",
    r"""pure contains(value: Str) -> Bool { true }
proc main() {
  let custom = {contains: contains, has: contains}
  let _ = custom.contains
  let _ = custom.has
  assert contains("value")
}
""",
  )?
  assert "lint.prefer-in" not in report and "lint.core-assert" not in report, report
}
