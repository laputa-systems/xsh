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

# Requires that `file` checks and is already in formatter layout.
proc assert_checked_and_formatted(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

test test_comparison_chain_fix_coalesces_stable_operands_and_converges { |ctx|
  let file = script(ctx, p"tests/fixtures/lint/comparison-chain.xsh".read_text()?)?
  assert findings(file, "lint.prefer-comparison-chain")? == 3
  let text = fixed(file, "lint.prefer-comparison-chain")?
  assert "lower <= middle < upper" in text, text
  assert "0 <= middle < upper <= 20" in text, text
  assert "0 < middle <= 10" in text, text
  assert_checked_and_formatted(file)
  assert findings(file, "lint.prefer-comparison-chain")? == 0
}

test test_comparison_chain_keeps_calls_mutable_reads_and_comments { |ctx|
  let file = script(ctx, p"tests/fixtures/lint/comparison-chain-unsafe.xsh".read_text()?)?
  assert findings(file, "lint.prefer-comparison-chain")? == 0
}

test test_prefer_guard_keeps_comments_else_and_multiple_actions { |ctx|
  # A comment is layout: the guard is still reported, without a rewrite that would drop it.
  let commented = "pure value() -> Int { if true { # keep\n return 1 }; return 2 }\n"
  let file = script(ctx, commented)?
  assert findings(file, "lint.prefer-guard")? == 1
  assert fixed(file, "lint.prefer-guard")? == commented
  for source in [
    "pure value() -> Int { if true { return 1 } else { return 2 } }\n",
    "proc value() [] -> Int { if true { print 1; return 1 }; return 2 }\n",
  ] {
    assert findings(script(ctx, source)?, "lint.prefer-guard")? == 0, source
  }
}

test test_prefer_guard_drops_the_braces_of_a_single_statement_match_arm { |ctx|
  # `xsht fmt` prints an arm holding one guarded statement without braces,
  # so the fix to the only statement of a braced arm takes the braces too.
  # An arm with a second statement, or with a comment, keeps them.
  let file = script(
    ctx,
    """pure pick(n: Int, c: Bool) -> Int {
  match n {
    0 => {
      if c {
        return 1
      }
    }
    1 => {
      if c {
        return 2
      }

      return 3
    }
    else => {}
  }

  4
}
""",
  )?
  assert fixed(file, "lint.prefer-guard")? == """pure pick(n: Int, c: Bool) -> Int {
  match n {
    0 => return 1 when c
    1 => {
      return 2 when c

      return 3
    }
    else => {}
  }

  4
}
"""
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr

  let commented = """pure pick(n: Int, c: Bool) -> Int {
  match n {
    0 => {
      # The cached value wins.
      if c {
        return 1
      }
    }
    else => {}
  }

  4
}
"""
  assert fixed(script(ctx, commented)?, "lint.prefer-guard")? == commented.replace(
    "      if c {\n        return 1\n      }\n",
    with: "      return 1 when c\n",
  )
}

test test_prefer_guard_groups_an_external_run_payload { |ctx|
  let file = script(
    ctx,
    "proc value(selected: Bool) [process] -> Status { if selected { return run.status /usr/bin/true }; return run.status /usr/bin/true }\n",
  )?
  let reported = lint(file, [])?
  assert "-> return (run.status /usr/bin/true) when selected\n" in reported.stderr, reported.stderr
  let text = fixed(file, "lint.prefer-guard")?
  assert "{ return (run.status /usr/bin/true) when selected; return run.status /usr/bin/true }" in text, text
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

test test_prefer_guard_keeps_unwieldy_payload_blocks { |ctx|
  let forty = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  let payload = forty + forty + forty
  let file = script(
    ctx,
    "pure value(selected: Bool) -> Str { if selected { return \"" + payload + "\" }; return \"fallback\" }\n",
  )?
  assert findings(file, "lint.prefer-guard")? == 0
}

test test_prefer_guard_reaches_nested_blocks_and_keeps_only_guards_over_the_column_cap { |ctx|
  # A `return Err(...)` guard is reported wherever it sits, but only when the
  # postfix form fits in 88 columns: the formatter has no readable layout
  # for a longer guarded statement, so the block stays.
  let file = script(
    ctx,
    """error AppError = Failed(message: Str)

proc validate(argv: List[Str]) [] -> Result[Unit] {
  if argv.len() > 4 {
    return Err(AppError.Failed("too many"))
  }

  if argv.len() > 3 {
    return Err(AppError.Failed("this message is long enough that the one-line guard passes the column cap"))
  }

  for arg in argv {
    if arg == "" {
      return Err(AppError.Failed("empty"))
    }

    for part in arg.split(",") {
      if part == "" {
        return Err(AppError.Failed("empty part"))
      }
    }
  }
}
""",
  )?
  let reported = lint(file, [])?
  let guards = collect {
    for line in reported.stderr.lines() {
      let found = rx"^help: use `return when` -> (.*)$".captures(line)
      yield found[1] when found.len() == 2
    }
  }

  assert guards == [
    "return Err(AppError.Failed(\"too many\")) when argv.len() > 4",
    "return Err(AppError.Failed(\"empty\")) when arg == \"\"",
    "return Err(AppError.Failed(\"empty part\")) when part == \"\"",
  ], reported.stderr
  let _ = fixed(file, "lint.prefer-guard")?
  assert_checked_and_formatted(file)
}

test test_guarded_return_keeps_following_statements_reachable { |ctx|
  let file = script(ctx, "pure value(selected: Bool) -> Int { return 1 when selected; return 2 }\n")?
  assert findings(file, "lint.dead-code")? == 0
}
