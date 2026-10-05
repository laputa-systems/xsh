type Captured = {status: Status, stdout: Str, stderr: Str}

# Writes `source` to a fresh script outside any project configuration.
proc script(ctx: TestContext, source: Str) [fs, error] -> Result[Path] {
  test.temp_file(ctx, name: "lint.xsh", contents: bytes.from_text(source))
}

# Runs `xsht lint` on `file` with `flags` and requires that the file parsed
# and checked, so that an absent finding is a statement about the linter. The
# tool runs in the file's directory: the configuration it reads is the
# defaults, never that of the suite's directory.
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

# Requires that `file` passes `xsht check`.
proc assert_checked(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Formats `file` in place and requires that a second pass changes nothing.
proc assert_fmt_converges(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
}

test test_duration_arithmetic_conversion_fix_rechecks_and_converges { |ctx|
  let file = script(ctx, "let pause = time.millis(250)\nlet budget = time.seconds(2)\n")?
  assert findings(file, "lint.duration-arithmetic")? == 2
  let text = fixed(file, "lint.duration-arithmetic")?
  assert "250 * 1ms" in text, text
  assert "2 * 1s" in text, text
  assert_checked(file)
  assert_fmt_converges(file)
  assert_checked(file)
  assert findings(file, "lint.duration-arithmetic")? == 0
}

test test_duration_arithmetic_conversion_keeps_clamping_saturation_unknowns_and_comments { |ctx|
  let file = script(
    ctx,
    "pure convert(value: Int) -> Duration { time.millis(value) }\nlet negative = time.millis(-1)\nlet saturated = time.seconds(9223372036854775807)\nlet commented = time.seconds(\n  # retain conversion annotation\n  2\n)\n",
  )?
  assert findings(file, "lint.duration-arithmetic")? == 0
}

test test_block_string_concatenation_fix_rechecks_exact_bytes_and_converges { |ctx|
  let file = script(ctx, "let value = \"first\\n\" + \"  café\\n\"\nprint \$value\n")?
  assert findings(file, "lint.prefer-block-string")? == 1
  let text = fixed(file, "lint.prefer-block-string")?
  assert "\"\"\"" in text, text
  assert_checked(file)

  # The block string decodes to the bytes the concatenation produced.
  let executed = test.expect(ctx, text, status: 0)?
  assert executed.stdout == "first\n  café\n\n"
  assert findings(file, "lint.prefer-block-string")? == 0
  assert_fmt_converges(file)
  assert_checked(file)
}

test test_prepared_constant_fix_keeps_comments_and_converges { |ctx|
  let file = script(
    ctx,
    "# protocol\nlet version = 1 # stable\nlet values: List[Int] = []\nlet runtime = 1 / 0\nlet ordinary = version\npure helper(input: Int) -> Int { let local = 2; input + local }\n",
  )?
  assert findings(file, "lint.prefer-const")? == 2
  let text = fixed(file, "lint.prefer-const")?
  assert "const version = 1 # stable" in text, text
  assert "let runtime = 1 / 0" in text, text
  assert "let ordinary = version" in text, text
  assert "let local = 2" in text, text
  assert_checked(file)
  assert findings(file, "lint.prefer-const")? == 0
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  let laid_out = file.read_text()?
  assert "const version = 1 # stable" in laid_out, laid_out
}

test test_named_argument_spread_requires_exact_stable_visible_fields_and_converges { |ctx|
  let code = "lint.prefer-named-argument-spread"
  let file = script(ctx, p"tests/fixtures/syntax/valid/named-argument-forwarding.xsh".read_text()?)?

  # Partial records and effectful receivers must not forward.
  let reported = lint(file, ["--only", code])?
  assert reported.stderr.split(f"[{code}]").len() - 1 == 3, reported.stderr
  assert findings(file, code)? == 3

  # Only the first finding carries a fix: comments remain intact, and
  # mutable receivers retain repeated reads.
  assert reported.stderr.split("\nhelp: ").len() - 1 == 1, reported.stderr
  assert "help: spread the checked record fields -> ...options\n" in reported.stderr, reported.stderr
  let text = fixed(file, code)?
  assert "print \${forwarding_total(...options)}" in text, text
  assert "  first: options.first,\n  # Retain this forwarding comment.\n  second: options.second,\n" in text, text
  assert "forwarding_total(first: changing.first, second: changing.second)" in text, text
  assert_checked(file)
  assert_fmt_converges(file)
  let laid_out = file.read_text()?
  assert findings(file, code)? == 2
  assert fixed(file, code)? == laid_out
}

test test_typed_map_keys_fmt_and_checked_literal_fix_converge { |ctx|
  let file = script(
    ctx,
    "type Identifier = Int\nvar values: Map[Identifier, Str] = {}\nvalues = values.set(20, \"twenty\")\nvalues = values.set(3, \"three\")\nprint values.len()\n",
  )?
  let text = fixed(file, "lint.prefer-map-literal")?
  assert "var values: Map[Identifier, Str] = {[20]: \"twenty\", [3]: \"three\"}" in text, text
  assert_checked(file)
  assert_fmt_converges(file)
  assert findings(file, "lint.prefer-map-literal")? == 0

  let comprehension = script(ctx, "let values = {[key + 1]: value for {key, value} in {[1]: 2}}\n")?
  assert_fmt_converges(comprehension)
  assert_checked(comprehension)
}

test test_parametric_record_constructor_fix_keeps_concrete_alias_and_converges { |ctx|
  let code = "lint.prefer-record-constructor"
  let file = script(
    ctx,
    "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet count: Count = {value: 7}\nprint \${count.value + 1}\n",
  )?
  assert findings(file, code)? == 1
  let text = fixed(file, code)?
  assert "let count: Count = Count(value: 7)" in text, text
  assert_checked(file)
  assert findings(file, code)? == 0
  assert_fmt_converges(file)

  let direct = script(ctx, "type Box[T] = {value: T}\nlet count: Box[Int] = {value: 7}\n")?
  assert findings(direct, code)? == 1
  let inferred = fixed(direct, code)?
  assert "let count: Box[Int] = Box(value: 7)" in inferred, inferred
  assert_checked(direct)
}

test test_absence_lookup_sentinel_fix_requires_proven_immutable_origin { |ctx|
  let file = script(
    ctx,
    "let position = \"a\".find(\"z\")\nlet alias = position\nlet found = alias != -1\nlet arbitrary: Int? = -1\nlet unrelated = arbitrary == -1\nvar mutable = \"a\".find(\"z\")\nlet unstable = mutable == -1\n",
  )?
  assert findings(file, "lint.lookup-absence")? == 1
  let text = fixed(file, "lint.lookup-absence")?
  assert "alias != null" in text, text
  assert "arbitrary == -1" in text, text
  assert "mutable == -1" in text, text
  assert_checked(file)
}

test test_absence_lookup_sentinel_proof_respects_shadowing_and_narrowing { |ctx|
  let file = script(
    ctx,
    "let position = \"x\".find(\":\")\nlet direct = \"x\".find(\":\") == -1\n{ let position: Int? = -1; let unrelated = position == -1 }\nif position != null { let ordinary = position == -1 }\nlet commented = \"x\".find(\n  # retain absence explanation\n  \":\"\n) == -1\n",
  )?
  assert findings(file, "lint.lookup-absence")? == 1
  let text = fixed(file, "lint.lookup-absence")?
  assert "let direct = \"x\".find(\":\") == null" in text, text
  assert "let ordinary = position == -1" in text, text
  assert "let unrelated = position == -1" in text, text
  assert text.ends_with("# retain absence explanation\n  \":\"\n) == -1\n"), text
  assert_checked(file)
}
