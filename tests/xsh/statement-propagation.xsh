# A statement-position `Result[Unit]` propagates its failure, so `step()?` and
# `step()` as statements are one program. Each case runs both spellings and
# requires the same output, status, and traceback.

const PRELUDE = r"""error Step = Bad(message: Str)

proc step(fail: Bool) -> Result[Unit, Step] {
  if fail { return Err(Step.Bad("stopped")) }
}
"""

# A traceback line without its script name. The failing statement's line also
# loses its end column, which is the one position the removed `?` moves.
pure site(line: Str) -> Str {
  let parts = line.split("both.xsh-")
  if parts.len() < 2 {
    return line
  }
  let position = parts[1].split(":")[1..].join(":")
  if line.starts_with("at ") {
    return position.split("-")[0]
  }
  position
}

proc run_both(ctx: TestContext, body: Str) [fs, process, env, time, error] -> Result[Unit] {
  let explicit = test.run_script(ctx, PRELUDE + body.replace("<?>", "?"), [], {}, b"", "both.xsh")?
  let implicit = test.run_script(ctx, PRELUDE + body.replace("<?>", ""), [], {}, b"", "both.xsh")?
  # Both spellings check, and fail only by the propagated error.
  assert "err[" not in explicit.stderr, explicit.stderr
  assert "err[" not in implicit.stderr, implicit.stderr
  assert explicit.stdout != ""
  assert explicit.status == implicit.status, implicit.stderr
  assert explicit.stdout == implicit.stdout
  let explicit_lines = explicit.stderr.lines()
  let implicit_lines = implicit.stderr.lines()
  assert explicit_lines.len() == implicit_lines.len(), implicit.stderr
  for index in range(explicit_lines.len()) {
    assert site(explicit_lines[index]) == site(implicit_lines[index])
  }
}

test test_statement_propagation_matches_in_a_function_body { |ctx|
  run_both(
    ctx,
    r"""
proc work() -> Result[Int, Step] {
  step(false)<?>
  print "first"
  step(true)<?>
  print "unreachable"
  1
}

print ${work()?}
""",
  )?
}

test test_statement_propagation_matches_in_statement_blocks { |ctx|
  run_both(
    ctx,
    r"""
proc work(items: List[Bool]) -> Result[Unit, Step] {
  for item in items {
    step(item)<?>
    print "passed"
  }
  if items.len() > 1 {
    step(true)<?>
  }
}

work([false, false, true, false])
""",
  )?
  run_both(
    ctx,
    r"""
proc work(flag: Bool) -> Result[Unit, Step] {
  if flag {
    step(true)<?>
  } else {
    step(false)<?>
  }
}

work(false)
print "ok"
work(true)
""",
  )?
}

test test_statement_propagation_matches_at_capture_boundaries { |ctx|
  run_both(
    ctx,
    r"""
proc work() [time] -> Int {
  let once = try {
    step(true)<?>
    1
  }
  var attempts = 0
  let again = retry [1ms, 1ms] {
    attempts += 1
    step(attempts < 3)<?>
    attempts
  }
  let unit: Result[Unit, Step] = try {
    step(true)<?>
  }
  print ${once is Err(_)} ${unit is Err(_)}
  again ?? 0
}

print ${work()}
""",
  )?
}

test test_statement_propagation_matches_in_cleanup_and_handlers { |ctx|
  run_both(
    ctx,
    r"""
proc count() -> Result[Int, Step] {
  Err(Step.Bad("count"))
}

proc work() -> Result[Int, Step] {
  defer {
    step(true)<?>
    print "unreachable cleanup"
  }
  defer { print "cleanup" }
  let value = count() ?? { |_|
    step(false)<?>
    7
  }
  print $value
  count() ?? { |_|
    step(true)<?>
    8
  }
}

print ${work() is Err(_)}
""",
  )?
}

test test_statement_propagation_matches_in_stream_callbacks { |ctx|
  run_both(
    ctx,
    r"""
proc work(items: List[Bool]) -> Result[Int, Step] {
  items |> each { |item|
    step(false)<?>
  }
  let seen = items |> map { |item|
    step(item)<?>
    1
  } |> count
  seen
}

print ${work([false, false])?}
print ${work([false, true]) is Err(_)}
work([false, true])?
""",
  )?
}

test test_statement_propagation_matches_at_top_level_and_through_plain_procs { |ctx|
  run_both(
    ctx,
    r"""
proc inner() [io, error] -> Int {
  step(true)<?>
  print "unreachable"
  1
}

proc outer() [io, error] -> Int {
  inner() + 1
}

step(false)<?>
print "top"
let caught = try { outer() }
print ${caught is Err(_)}
step(true)<?>
print "unreachable"
""",
  )?
  # The final top-level statement may be the exit status, but a `Result[Unit]`
  # there still propagates.
  run_both(
    ctx,
    r"""
proc entry(...argv: List[Str]) [io, error] -> Result[Unit, Step] {
  print "entry"
  step(true)<?>
  print "unreachable"
}

entry(@args)<?>
""",
  )?
}

test test_statement_propagation_matches_inside_context_scopes { |ctx|
  let root = test.temp_dir(ctx, name: "statement-propagation")?
  run_both(
    ctx,
    f"""
proc work(dir: Path) [env, io, error] -> Result[Int, Error] {{
  cd (dir) {{
    step(false)<?>
    print "inside"
    step(true)<?>
    print "unreachable"
  }}
  1
}}

print ${{work(p"{root}") is Err(_)}}
work(p"{root}")?
""",
  )?
}

# The direct tail of a `Result[Unit]` function is the one statement where the
# two spellings differ: without `?` the `Result` is the function's value, and
# the traceback starts at the caller instead of inside the function.
test test_result_unit_function_tail_reports_its_own_propagation { |ctx|
  let explicit = test.run_script(
    ctx,
    PRELUDE + r"""
proc work() -> Result[Unit, Step] {
  step(true)?
}

work()
""",
  )?
  assert explicit.status == 3
  assert "proc work at" in explicit.stderr
}

# A `cd`, `env`, `try`, or `retry` block in statement position is a
# `Result[Unit]` too, so its trailing `?` is the same redundancy.
test test_statement_scope_propagation_matches { |ctx|
  let root = test.temp_dir(ctx, name: "statement-scope-propagation")?
  run_both(
    ctx,
    f"""
proc work(dir: Path, fail: Bool) [env, io, time, error] -> Result[Int, Error] {{
  cd $dir {{
    print "command cd"
    step(false)?
  }}<?>
  cd (dir) {{
    print "expression cd"
    step(false)?
  }}<?>
  env MODE=fast {{
    print "command env"
    step(false)?
  }}<?>
  env ({{MODE: "fast"}}) {{
    print "expression env"
    step(fail)?
  }}<?>
  try {{
    print "try"
    step(false)?
  }}<?>
  retry [1ms] {{
    print "retry"
    step(false)?
  }}<?>
  1
}}

print ${{work(p"{root}", false)?}}
print ${{work(p"{root}", true) is Err(_)}}
print ${{work(p"{root}/missing", false) is Err(_)}}
work(p"{root}/missing", false)?
""",
  )?
  run_both(
    ctx,
    f"""
proc work(dir: Path) [env, io, error] -> Result[Unit, Error] {{
  cd $dir {{
    print "inside"
  }}<?>
}}

work(p"{root}")
print "entered"
work(p"{root}/missing")
""",
  )?
}

const LINT_CANDIDATE = r"""error Step = Bad(message: Str)

proc step(fail: Bool) -> Result[Unit, Step] {
  if fail { return Err(Step.Bad("stopped")) }
}

proc count() -> Result[Int, Step] {
  2
}

proc finish(fail: Bool) -> Result[Unit, Step] {
  step(false)<removed>
  step(fail)?
}

proc work(dir: Path, fail: Bool) [env, io, error] -> Result[Int] {
  step(false)<removed>
  defer step(false)?
  for flag in [false, false] {
    step(flag)<removed>
  }
  let captured = try {
    step(fail)?
  }
  print ${captured is Err(_)}
  cd $dir {
    step(false)<removed>
  }<removed>
  env ({MODE: "x"}) {
    step(false)<removed>
  }<removed>
<unit-match>
<value-match>
  let total = count()?
  finish(fail)<removed>
  total
}

print ${work(p"ROOT", false)?}
print ${work(p"ROOT", true) is Err(_)}
"""

# The three rules through the CLI: every redundant `?` goes, a re-propagating
# `match` becomes `?` and then loses it where the statement propagates anyway,
# and each `?` that carries information stays (a function's `Result[Unit]`
# tail, `defer`, a bound `try` tail, and a value).
test test_propagation_lints_fix_only_the_redundant_spellings { |ctx|
  let root = test.temp_dir(ctx, name: "propagation-lints")?
  let outline = LINT_CANDIDATE.replace("ROOT", root.display())
  let source = outline.replace("<removed>", "?")
    .replace(
      "<unit-match>",
      "  match step(false) {\n    Ok(_) => {}\n    Err(problem) => return Err(problem)\n  }",
    )
    .replace(
      "<value-match>",
      "  match count() {\n    Ok(_) => {}\n    Err(problem) => return Err(problem)\n  }",
    )
  let expected = outline.replace("<removed>", "")
    .replace("<unit-match>", "  step(false)")
    .replace("<value-match>", "  let _ = count()?")
  let rules = "lint.redundant-propagation,lint.redundant-scope-propagation,lint.prefer-propagation"
  let candidate = test.temp_file(ctx, name: "propagation-lints.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only $rules --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?
  assert fixed == expected

  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  assert before.success, before.stderr
  assert after.success, after.stderr
  assert before.stdout == "false\n2\ntrue\ntrue\n"
  assert after.stdout == before.stdout

  let repeated = run.capture --text "xsht" lint --only $rules $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint." not in repeated.stderr
}
