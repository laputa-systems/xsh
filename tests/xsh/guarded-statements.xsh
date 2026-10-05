test test_a_guarded_expression_statement_runs_only_when_selected { |ctx|
  let source = r"""proc say(text: Str) {
  print $text
}

proc asked(text: Str) -> Bool {
  print $text
  true
}

say("when true") when true
say("when false") when false
say("unless true") unless true
say("unless false") unless false
say("payload") when asked("condition")
conf.missing when false
"""
  # The condition runs first, and the payload only if it is selected. A
  # dotted name before the guard is an expression, not a command.
  let _ = test.expect(ctx, source, status: 2, stderr: ["unresolved name `conf`"])?
  let kept = source.replace("conf.missing when false\n", with: "")
  let output = test.expect(ctx, kept, status: 0)?
  assert output.stdout == "when true\nunless false\ncondition\npayload\n", output.stdout
}

test test_a_guarded_assignment_assigns_only_when_selected {
  var total = 0
  var names = ["a"]
  var counts = {seen: 0}
  for row in [3, -1, 4] {
    total += row when row > 0
    names[0] = f"row {row}" unless row > 0
    counts.seen = row when row == 4
  }

  assert total == 7
  assert names == ["row -1"]
  assert counts.seen == 4
}

pure checked_port(raw: Int) -> Result[Int] {
  guard raw > 0 else fail f"port {raw} is not positive"
  guard raw < 65536 else fail "port is too large" because error.failure("range")
  Ok(raw)
}

pure unless_port(raw: Int) -> Result[Int] {
  fail f"port {raw} is not positive" unless raw > 0
  Ok(raw)
}

test test_guard_else_fail_means_fail_unless {
  assert checked_port(80)? == 80
  match checked_port(0) {
    Ok(value) => test.fail(f"accepted {value}")
    Err(problem) => {
      assert problem.message == "port 0 is not positive"
      match unless_port(0) {
        Ok(value) => test.fail(f"accepted {value}")
        Err(other) => assert other.message == problem.message
      }
    }
  }

  match checked_port(70000) {
    Ok(value) => test.fail(f"accepted {value}")
    Err(problem) => assert problem.message == "port is too large"
  }
}

test test_a_guarded_print_prints_only_when_selected { |ctx|
  let source = r"""proc show(verbose: Bool) {
  print "copying" when verbose
  print "quiet" unless verbose
  eprint "to stderr" when verbose
  print "when" "unless" when verbose
  print when verbose
  print "end"
}

show(true)
show(false)
"""
  let ran = test.expect(ctx, source, status: 0)?
  assert ran.stdout == "copying\nwhen unless\n\nend\nquiet\nend\n", ran.stdout
  assert ran.stderr == "to stderr\n", ran.stderr
}

test test_a_guarded_statement_propagates_like_its_if { |ctx|
  let source = r"""proc step(name: Str) -> Result[Unit] {
  fail f"bad {name}" when name == "second"
}

proc exists(name: Str) -> Result[Bool] {
  fail "no lookup" when name == "third"
  Ok(true)
}

proc work(name: Str) -> Result[Int] {
  step(name) when exists(name)
  Ok(1)
}

for name in ["first", "second", "third"] {
  match work(name) {
    Ok(value) => print f"{name}: {value}"
    Err(problem) => print f"{name}: {problem.message}"
  }
}
"""
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == "first: 1\nsecond: bad second\nthird: no lookup\n", output.stdout
}

test test_a_run_form_reads_the_guard_words_as_arguments { |ctx|
  let source = r"""let ready = false
run echo when ready
let listed = run.text echo one unless ready ?
print ${listed.trim()}
"""
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == "when ready\none unless ready\n", output.stdout
}

test test_a_guard_is_rejected_where_no_statement_takes_one { |ctx|
  for source in [
    "let ready = true\nlet total = 1 when ready\n",
    "let ready = true\ndefer print(\"x\") when ready\n",
    "pure port(raw: Int) -> Result[Int] {\n  guard raw > 0 else fail \"low\" when raw < 0\n  Ok(raw)\n}\n",
    "pure port(raw: Int) -> Result[Int] {\n  guard raw > 0 else return Ok(1)\n  Ok(raw)\n}\n",
  ] {
    let ran = test.expect(ctx, source, status: 2)?
    assert "err[parse." in ran.stderr, ran.stderr
  }
}

# `let` takes no postfix guard, so the ignored-value diagnostic of a guarded
# statement offers no `let _ = ` insertion: the result would not parse.
test test_a_guarded_statement_with_an_ignored_value_has_no_discard_fix { |ctx|
  let body = "pure next(count: Int) -> Int {\n  count + 1\n}\n\nlet ready = true\n"
  let guarded = test.expect(
    ctx,
    body + "next(1) when ready\nprint \"done\"\n",
    status: 2,
    stderr: ["check.ignored-result"],
  )?
  assert "help: discard" not in guarded.stderr, guarded.stderr
  assert "if COND { let _ = ... }" in guarded.stderr, guarded.stderr
  let plain = test.expect(ctx, body + "next(1)\nprint \"done\"\n", status: 2, stderr: ["check.ignored-result"])?
  assert "help: discard with `let _ =`" in plain.stderr, plain.stderr
}

test test_guarded_statements_format_and_desugar_as_written { |ctx|
  let source = r"""proc stage(tmp: Path, verbose: Bool) -> Result[Int] {
  var copied = 0
  print f"copying {tmp}" when verbose
  tmp.remove() when tmp.exists()
  copied = 1 unless verbose
  guard copied < 2 else fail "copied twice"
  Ok(copied)
}

print ${stage(/tmp/guarded-statements-missing, false)?}
"""
  let candidate = test.temp_file(ctx, name: "stage.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt --check $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let linted = run.capture --text "xsht" lint $candidate
  assert linted.status.exited_with(0), linted.stderr
  let desugared = run.capture --text "xsht" desugar $candidate
  assert desugared.status.exited_with(0), desugared.stderr
  let expected = r"""proc stage(tmp: Path, verbose: Bool) -> Result[Int] {
  var copied = 0
  if verbose { print f"copying {tmp}" }
  if tmp.exists() { tmp.remove() }
  if verbose {} else {
    copied = 1
  }

  if copied < 2 {} else {
    return Err(error.failure("copied twice"))
  }

  Ok(copied)
}

print ${stage(/tmp/guarded-statements-missing, false)?}
"""
  assert desugared.stdout == expected, desugared.stdout
}
