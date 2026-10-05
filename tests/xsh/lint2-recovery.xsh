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

# How many findings with `code` the default rule set reports for `file`.
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

# Requires that `file` is already in formatter layout.
proc assert_formatted(file: Path) [process, env, error] {
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
}

# The text of `file` after `xsht fmt` rewrote it.
proc formatted(file: Path) [fs, process, env, error] -> Result[Str] {
  let rewritten = run.capture --text "xsht" fmt $file
  assert rewritten.status.exited_with(0), rewritten.stderr
  file.read_text()
}

test test_optional_postfix_fix_keeps_null_fallback_and_converges { |ctx|
  let file = script(
    ctx,
    "let name: Str? = null\nlet label = if name == null { \"default\" } else { name.trim() }\nprint \$label\n",
  )?
  assert findings(file, "lint.prefer-optional-postfix")? == 1
  let _ = fixed(file, "lint.prefer-optional-postfix")?
  assert_checked(file)
  let text = formatted(file)?
  assert "name?.trim() ??" in text, text
  assert findings(file, "lint.prefer-optional-postfix")? == 0
  assert_formatted(file)
}

test test_optional_postfix_refuses_mutation_comments_and_optional_results { |ctx|
  for source in [
    "let name: Str? = null\nlet label = if name == null { \"default\" } else { print \"selected\"; name.trim() }\nprint \$label\n",
    "var name: Str? = null\nname = \"x\"\nlet label = if name == null { \"default\" } else { name.trim() }\nprint \$label\n",
    "let name: Str? = null\nlet label = if name == null {\n  # retain explanation\n  \"default\"\n} else { name.trim() }\nprint \$label\n",
    "type Item = {name: Str?}\nlet item: Item? = null\nlet name: Str? = if item == null { \"default\" } else { item.name }\nprint (name ?? \"\")\n",
  ] {
    assert findings(script(ctx, source)?, "lint.prefer-optional-postfix")? == 0, source
  }
}

test test_fmt_keeps_guarded_postfix_and_unicode_text { |ctx|
  let file = script(
    ctx,
    "let text: Str? = null\nlet prefix = text?[..2] ?? \"α\"\nlet suffix = text?[1..] ?? \"β\"\nlet whole = text?[..] ?? \"γ\"\nlet values: List[Int]? = null\nlet item = values?[0] ?? 3\nprint (text?.trim() ?? prefix) \$suffix \$whole \$item\n",
  )?
  let text = formatted(file)?
  assert "text?[..2]" in text, text
  assert "values?[0]" in text, text
  assert_formatted(file)
  assert_checked(file)
}

test test_defer_block_helper_fix_is_checked_and_idempotent { |ctx|
  let file = script(ctx, "proc cleanup() [] -> Unit {\n  print \"café\"\n}\ndefer cleanup()\nprint \"body\"\n")?
  assert findings(file, "lint.prefer-defer-block")? == 1

  # The finding carries two edits: the helper declaration goes and its body
  # replaces the deferred call.
  let text = fixed(file, "lint.prefer-defer-block")?
  assert "defer {\n  print \"café\"\n}" in text, text
  assert "cleanup" not in text, text
  assert_checked(file)
  assert findings(file, "lint.prefer-defer-block")? == 0
}

test test_defer_block_helper_refuses_captures_failures_comments_and_multiple_uses { |ctx|
  for source in [
    "let message = \"captured\"\nproc cleanup() [] -> Unit { print \$message }\ndefer cleanup()\n",
    "proc cleanup() [error] { let _ = \"bad\".parse_int()? }\ndefer cleanup()\n",
    "# preserve helper docs\nproc cleanup() [] -> Unit { print \"done\" }\ndefer cleanup()\n",
    "proc cleanup() [] -> Unit { print \"done\" }\ndefer cleanup()\ncleanup()\n",
    "proc cleanup() [] -> Unit { print \"done\" }\nproc caller() [] { defer cleanup() }\ncaller()\n",
  ] {
    assert findings(script(ctx, source)?, "lint.prefer-defer-block")? == 0, source
  }
}

test test_regex_literal_fix_decodes_patterns_keeps_comments_and_converges { |ctx|
  let file = script(
    ctx,
    r"""let escaped = regex.compile("^\\s*[A-Z]+$")? # retained
let quoted = regex.compile("^\".*\"$")?
let raw = regex.compile(r"\$\{literal\}")?
print ${escaped.matches("WORD")} ${quoted.matches("\"word\"")} ${raw.matches(r"${literal}")}
""",
  )?
  assert findings(file, "lint.prefer-regex-literal")? == 3
  let text = fixed(file, "lint.prefer-regex-literal")?
  assert r"""rx"^\s*[A-Z]+$" # retained""" in text, text
  assert "rx\"\"\"^\".*\"$\"\"\"" in text, text
  assert_checked(file)
  assert_formatted(file)
  assert findings(file, "lint.prefer-regex-literal")? == 0
}

test test_regex_literal_keeps_invalid_dynamic_consumed_and_recovered_compiles { |ctx|
  let source = r"""let pattern = "[a-z]+"
let invalid = regex.compile("(")
let dynamic = regex.compile(pattern)?
let consumed = regex.compile("[a-z]+")
let recovered = regex.compile("[a-z]+") ?? rx".*"
let contextual = regex.compile("[a-z]+").context("user pattern")?
let unrepresentable = regex.compile("\"\"\"")?
let commented = regex.compile(
  # explanation
  "[a-z]+",
)?
print ${dynamic.matches("x")} ${recovered.matches("x")} ${contextual.matches("x")} ${unrepresentable.matches("x")} ${commented.matches("x")}
"""
  let file = script(ctx, source)?
  let reported = lint(file, ["--only", "lint.prefer-regex-literal"])?
  assert reported.stderr.split("[lint.prefer-regex-literal]").len() == 2, reported.stderr
  assert "\nnote: " in reported.stderr, reported.stderr
  assert findings(file, "lint.prefer-regex-literal")? == 1
  assert fixed(file, "lint.prefer-regex-literal")? == source
}

test test_regex_literal_keeps_compile_calls_in_result_recovery_branches { |ctx|
  let file = script(
    ctx,
    r"""with value = regex.compile("(") { print ${value.matches("x")} } else { let fallback = regex.compile(".*")?; print ${fallback.matches("x")} }
let recovered = regex.compile("(") ?? regex.compile(".*")?
let matched = match regex.compile("(") {
  Ok(value) => value,
  Err(_) => regex.compile(".*")?,
}
match regex.compile("(") {
  Ok(value) => { print ${value.matches("x")} },
  Err(_) => { let fallback = regex.compile(".*")?; print ${fallback.matches("x")} },
}
print ${recovered.matches("x")} ${matched.matches("x")}
""",
  )?
  assert findings(file, "lint.prefer-regex-literal")? == 0
}

test test_yield_delegation_forwarding_fix_is_checked_and_idempotent { |ctx|
  for iterable in ["values", "Ok(values)?"] {
    let file = script(
      ctx,
      "stream rows(values: List[Int]) [error] -> Stream[Int] {\n  for item in " + iterable + " {\n    yield item\n  }\n}\n",
    )?
    assert findings(file, "lint.prefer-yield-delegation")? == 1, iterable
    let text = fixed(file, "lint.prefer-yield-delegation")?
    assert f"yield @{iterable}" in text or f"yield @({iterable})" in text, text
    assert "for item in" not in text, text
    assert_checked(file)
    assert findings(file, "lint.prefer-yield-delegation")? == 0, text
  }
}

test test_yield_delegation_keeps_nontransparent_forwarding_loops { |ctx|
  for body in [
    "yield item * 2",
    "if item > 0 { yield item }",
    "print \$item\n    yield item",
    "defer close()\n    yield item",
    "yield item\n    break",
    "# current item\n    yield item",
  ] {
    let file = script(
      ctx,
      "proc close() [io] { print \"close\" }\nstream rows(values: List[Int]) [io, error] -> Stream[Int] {\n  for item in values {\n    " + body + "\n  }\n}\n",
    )?
    assert findings(file, "lint.prefer-yield-delegation")? == 0, body
  }

  for source in [
    "stream rows(values: Result[List[Int]]) [error] -> Stream[Int] { for item in values { yield item } }\n",
    "stream rows(values: Stream[Int]) [] -> Stream[Int] { for item in values |> map { |number| number + 1 } { yield item } }\n",
  ] {
    assert findings(script(ctx, source)?, "lint.prefer-yield-delegation")? == 0, source
  }
}

test test_fmt_keeps_yield_delegation_and_postfix_guards { |ctx|
  let file = script(ctx, "stream rows() -> Stream[Int] {\n  yield @[1, 2]\n  yield @([3]) when true\n}\n")?
  let text = formatted(file)?
  assert "yield @[1, 2]\n" in text, text
  assert "when true\n" in text, text
  assert_checked(file)
  assert_formatted(file)
}

test test_error_fallback_fix_keeps_the_handler_and_converges { |ctx|
  let file = script(
    ctx,
    "pure recover(outcome: Result[Str]) -> Str {\n  let selected = match outcome { Ok(value) => value, Err(failure) => failure.message }\n  selected\n}\n",
  )?
  assert findings(file, "lint.error-fallback-block")? == 1
  let _ = fixed(file, "lint.error-fallback-block")?
  assert_checked(file)
  let text = formatted(file)?
  assert "outcome ?? { |failure|" in text, text
  assert "failure.message" in text, text
  assert findings(file, "lint.error-fallback-block")? == 0
  assert_formatted(file)
}

test test_error_fallback_refuses_success_transforms_guards_and_error_patterns { |ctx|
  for source in [
    "pure recover(outcome: Result[Str]) -> Str { let selected = match outcome { Ok(value) => value.trim(), Err(failure) => failure.message }; selected }\n",
    "pure recover(outcome: Result[Str]) -> Str { let selected = match outcome { Ok(value) if true => value, _ => \"other\" }; selected }\n",
    "pure recover(outcome: Result[Str]) -> Str { let selected = match outcome { Ok(value) => value, Err(is NotFound) => \"missing\", _ => \"other\" }; selected }\n",
    "pure recover(outcome: Result[Str]) -> Str { let selected = match outcome { Ok(value) => value, Err(failure) => {\n# retain this explanation\nfailure.message\n} }; selected }\n",
  ] {
    assert findings(script(ctx, source)?, "lint.error-fallback-block")? == 0, source
  }
}

test test_error_fallback_fix_keeps_a_multiline_handler { |ctx|
  let file = script(
    ctx,
    "enum Choice { Text(Str), Count(Int) }\npure recover(outcome: Result[Choice]) -> Str {\n  match match outcome { Ok(value) => value, Err(failure) => Text(\n    \"fallback\",\n  ) } {\n    Text(text) => text,\n    Count(count) => f\"{count}\",\n  }\n}\n",
  )?
  assert findings(file, "lint.error-fallback-block")? == 1
  let text = fixed(file, "lint.error-fallback-block")?
  assert "outcome ?? { |failure|" in text, text
  assert "Text(\n    \"fallback\",\n  )" in text, text
  assert_checked(file)
}

test test_error_fallback_handler_return_keeps_the_success_path_reachable { |ctx|
  let file = script(
    ctx,
    "pure choose(outcome: Result[Int]) -> Int {\n  let selected = outcome ?? { |_| return 7 }\n  selected\n}\n",
  )?
  assert findings(file, "lint.unreachable")? == 0
}
