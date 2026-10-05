type Captured = {status: Status, stdout: Str, stderr: Str}

# Writes `source` to a fresh script outside any project configuration.
proc script(ctx: TestContext, source: Str) [fs, error] -> Result[Path] {
  test.temp_file(ctx, name: "lint.xsh", contents: bytes.from_text(source))
}

# Runs `xsht lint` on `file` with `flags` and requires that the file parsed
# and checked, so that an absent finding is a statement about the linter.
# It runs in the directory of `file`, where no project configuration applies:
# the runner's own directory is the repository, whose configuration turns
# some default rules off.
proc lint(file: Path, flags: List[Str]) [process, env, error] -> Result[Captured] {
  let linted = cd (file.parent()) {
    run.capture --text "xsht" lint @flags $file
  }?

  assert linted.status.exited_with(0) or linted.status.exited_with(1), linted.stderr
  Ok(linted)
}

# How many findings with `code` the configured rule set reports for `file`.
proc findings(file: Path, code: Str) [process, env, error] -> Result[Int] {
  let linted = lint(file, [])?
  Ok(linted.stderr.split(f"[{code}]").len() - 1)
}

# The text of `file` after the fixes of `code` alone were applied.
proc fixed(file: Path, code: Str) [fs, process, env, error] -> Result[Str] {
  let _ = lint(file, ["--fix", "--only", code])?
  file.read_text()
}

# Requires that `file` checks.
proc assert_checked(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Formats `file` in place and requires that a second pass changes nothing.
proc assert_formats_stably(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
}

# A project whose `tests` directory is its native test root, holding `source`
# both as `tests/legacy.xsh` and, outside the root, as `ordinary.xsh`.
proc test_root_project(ctx: TestContext, source: Str) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "project")?
  fp"{root}/xsht-config.ini".write("test_roots = tests\n")
  fp"{root}/tests".mkdir()
  fp"{root}/tests/legacy.xsh".write(source)
  fp"{root}/ordinary.xsh".write(source)
  Ok(root)
}

test test_private_pure_return_removal_is_opt_in_exact_and_convergent { |ctx|
  let source = "pure label(name: Str) -> Str { name.trim() }\nprint label(\"ready\")\n"
  assert findings(script(ctx, source)?, "lint.prefer-inferred-pure-return")? == 0
  let root = test.temp_dir(ctx, name: "project")?
  fp"{root}/xsht-config.ini".write("[lint]\nprefer-inferred-pure-returns = true\n")
  let file = fp"{root}/label.xsh"
  file.write(source)
  assert findings(file, "lint.prefer-inferred-pure-return")? == 1
  assert fixed(file, "lint.prefer-inferred-pure-return")? == "pure label(name: Str) { name.trim() }\nprint label(\"ready\")\n"
  assert_checked(file)
  assert findings(file, "lint.prefer-inferred-pure-return")? == 0
}

test test_identical_match_arms_fix_joins_adjacent_alternatives_and_converges { |ctx|
  let file = script(
    ctx,
    r"""enum Event { Added(Str), Changed(Str), Deleted(Str) }
let event = Added("café")
let selected = match event {
  Added(name) => name.upper()
  Changed(name) => name.upper()
  Deleted(name) => name.upper()
}
print $selected
""",
  )?
  assert findings(file, "lint.identical-match-arms")? == 1
  let text = fixed(file, "lint.identical-match-arms")?
  assert "Added(name) | Changed(name) | Deleted(name) => name.upper()" in text, text
  assert_checked(file)
  assert findings(file, "lint.identical-match-arms")? == 0
  assert_formats_stably(file)
}

test test_identical_match_arms_keep_guards_comments_and_capture_types { |ctx|
  for source in [
    "enum Event { Number(Int), Text(Str) }\nlet result = match Number(1) { Number(value) => 0 Text(value) => 0 }\n",
    "let result = match [1] {\n [left] => 0\n [right, ..] => 0\n _ => 1\n}\n",
    "let result = match 1 { 1 if false => 0 2 => 0 _ => 1 }\n",
    "let result = match 1 { 1 => 0 # keep reason\n 2 => 0 _ => 1 }\n",
  ] {
    assert findings(script(ctx, source)?, "lint.identical-match-arms")? == 0, source
  }
}

test test_fmt_pattern_alias_keeps_group_precedence_and_comments { |ctx|
  let file = script(
    ctx,
    "let selected = match [1] {\n ([name] | [name, ..]) as original => { # whole value\n name + original.len()\n }\n _ => 0\n}\n",
  )?
  assert_formats_stably(file)
  let text = file.read_text()?
  assert "([name] | [name, ..]) as original" in text, text
  assert "# whole value" in text, text
  assert_checked(file)
}

test test_value_pipeline_fix_rewrites_safe_nested_and_linear_calls { |ctx|
  for source in [
    "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet selected = outer(10, inner(2))\n",
    "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet temporary = inner(2)\nlet selected = outer(10, value: temporary)\n",
  ] {
    let file = script(ctx, source)?
    assert findings(file, "lint.prefer-value-pipeline")? > 0, source
    let text = fixed(file, "lint.prefer-value-pipeline")?
    assert "inner(2) |> outer(10," in text, text
    assert_checked(file)
    assert findings(file, "lint.prefer-value-pipeline")? == 0, text
    assert_formats_stably(file)
  }
}

test test_fmt_assert_keeps_statement_and_message_comments { |ctx|
  let source = "proc check(value: Int) [error] {\n  # café context\n  assert value == 2, f\"value {value}\" # useful context\n}\n"
  let file = script(ctx, source)?
  assert_formats_stably(file)
  assert file.read_text()? == source
  assert_checked(file)
}

test test_core_assert_fix_rewrites_literal_context_and_refuses_eager_or_consumed_results { |ctx|
  let file = script(
    ctx,
    r"""proc context() [io] -> Str { print "context"; "detail" }
proc assertions(dynamic: Any) [io, error] {
  test.ok(true, "café")?
  test.eq(1 + 1, 2, message: "equality")?
  test.ne("left", "right", "inequality")?
  test.ok(true, context())?
  test.eq(dynamic, 1, "dynamic")?
  let consumed = test.ok(true, "consumed")
  consumed?
  let captured: Result[Unit] = try { test.ok(true, "captured")? }
  let retried: Result[Unit] = retry [] { test.eq(1, 1, "retried")? }
}
""",
  )?
  assert findings(file, "lint.core-assert")? == 4
  let text = fixed(file, "lint.core-assert")?
  assert "assert true, \"café\"" in text, text

  # The eager context keeps its effects.
  assert "test.ok(true, context())?" in text, text
  assert "test.eq(dynamic, 1, \"dynamic\")?" in text, text
  assert "let consumed = test.ok(true, \"consumed\")" in text, text
  assert "try { test.ok(true, \"captured\")? }" in text, text
  assert "retry [] { test.eq(1, 1, \"retried\")? }" in text, text
  assert_checked(file)
  assert findings(file, "lint.core-assert")? == 1
  assert fixed(file, "lint.core-assert")? == text
}

test test_legacy_test_proc_migration_keeps_context_and_effects_and_converges { |ctx|
  let root = test_root_project(
    ctx,
    "proc test_exact_name(ctx: TestContext) [error] -> Result[Unit] {\n  test.eq(ctx.name, ctx.name)?\n  return Ok()\n}\n",
  )?
  let file = fp"{root}/tests/legacy.xsh"
  assert findings(file, "lint.legacy-test-proc")? == 1
  let text = fixed(file, "lint.legacy-test-proc")?
  assert text.starts_with("test test_exact_name [error] { |ctx|"), text
  assert "return Ok()" in text, text
  assert_checked(file)
  assert findings(file, "lint.legacy-test-proc")? == 0
  assert_formats_stably(file)
  let formatted = file.read_text()?
  assert "test test_exact_name [error] { |ctx|" in formatted, formatted
}

test test_legacy_test_proc_migration_declines_callers_and_ordinary_files { |ctx|
  let source = "proc test_called() {}\nproc caller() { test_called() }\n"
  let root = test_root_project(ctx, source)?
  let file = fp"{root}/tests/legacy.xsh"
  assert findings(file, "lint.legacy-test-proc")? > 0
  assert fixed(file, "lint.legacy-test-proc")? == source
  assert findings(fp"{root}/ordinary.xsh", "lint.legacy-test-proc")? == 0
}

test test_fmt_enum_declaration_is_canonical_and_idempotent { |ctx|
  let file = script(ctx, "enum Token { Present(Str), }\ntype Alias = Token\n")?
  assert_formats_stably(file)
  let text = file.read_text()?
  assert "enum Token { Present(Str) }" in text, text
  assert "type Alias = Token" in text, text
}

test test_fmt_enum_comments_stay_with_their_variants { |ctx|
  let declaration = "export enum Choice {\n  Selected(Int), # payload café\n  # absent choice\n  Empty,\n}"
  let file = script(ctx, declaration + "\ntype Alias = Choice\n")?
  assert_formats_stably(file)
  let text = file.read_text()?
  assert declaration in text, text
}

# The tail of a function whose return type is inferred is the function's
# value: the bare call there would return what the binding drops, so the
# helper would stop being a `Unit` procedure, and writing `return` after the
# bare call draws `lint.redundant-bare-return`. The rule does not report what
# its own fix cannot do. A helper that declares its return is still reported.
test test_redundant_discard_leaves_the_tail_of_an_inferred_return_body { |ctx|
  let source = r"""proc assert_invalid_net_input(ctx: TestContext, source: Str, kind: Str) [error] {
  let _ = test.expect(ctx, source, status: 3, stderr: [kind])?
}

test test_invalid_input { |ctx|
  assert_invalid_net_input(ctx, "exit 3", "kind")
}
"""
  let root = test_root_project(ctx, source)?
  let file = fp"{root}/tests/legacy.xsh"
  assert_checked(file)
  assert findings(file, "lint.redundant-discard")? == 0
  assert fixed(file, "lint.redundant-discard")? == source

  # Each branch of a tail `if` is a tail too.
  let branching = source.replace(
    "  let _ = test.expect(ctx, source, status: 3, stderr: [kind])?\n",
    with: "  if kind == \"\" {\n    let _ = test.expect(ctx, source, status: 3)?\n  } else {\n    let _ = test.expect(ctx, source, status: 3, stderr: [kind])?\n  }\n",
  )
  file.write(branching)
  assert_checked(file)
  assert findings(file, "lint.redundant-discard")? == 0

  let declared = source.replace("[error] {", with: "[error] -> Result[Unit] {")
  file.write(declared)
  assert findings(file, "lint.redundant-discard")? == 1
  assert fixed(file, "lint.redundant-discard")? == declared.replace("  let _ = test", with: "  test")
  assert_checked(file)
}

# `link.symlink(to: target)` is one column wider than `fs.symlink(target,
# link)` when the link is a string literal, which the method spells `p"..."`.
# Where that column is the first one past the formatter's width, the fix is
# the broken call that `xsht fmt` prints, not a note to break it by hand.
test test_argument_label_fix_breaks_a_call_that_no_longer_fits_its_line { |ctx|
  let width = 120
  let around = "  fs.symlink(root.parent(), \"\")".byte_len()
  let filler = "llllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllll"
  for statement_width in [width - 1, width] {
    let link = filler.byte_slice(0, statement_width - around)
    let source = f"proc stage(root: Path) [fs, error] {{\n  fs.symlink(root.parent(), \"{link}\")\n}}\n"
    let file = script(ctx, source)?
    assert_checked(file)
    assert findings(file, "lint.prefer-argument-label")? == 1
    let text = fixed(file, "lint.prefer-argument-label")?
    let call = f"p\"{link}\".symlink("
    let expected = if statement_width == width {
      f"  {call}\n    to: root.parent(),\n  )\n"
    } else {
      f"  {call}to: root.parent())\n"
    }

    assert expected in text, text
    assert findings(file, "lint.prefer-argument-label")? == 0
    assert_checked(file)
    let formatted = cd (file.parent()) {
      run.capture --text "xsht" fmt --check $file
    }?

    assert formatted.status.exited_with(0), formatted.stdout
  }
}
