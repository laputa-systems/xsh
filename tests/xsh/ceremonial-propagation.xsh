# The report of an uncaught failure without the path of the script it names,
# which differs between two runs of one test.
pure without_paths(report: Str) -> Str {
  rx"/[^ \n]*/script\.xsh-[0-9]+".replace(report, "script")
}

test test_a_plain_run_statement_fails_the_same_with_and_without_propagation { |ctx|
  let written = "proc build() -> Result[Int] {\n  defer { print \"cleanup\" }\n  run sh -c \"echo out; exit 3\" ?\n  print \"after\"\n  Ok(1)\n}\n\nlet built = build()?\n"
  let bare = written.replace(" ?\n", "\n")
  assert bare != written
  let with_propagation = test.run_script(ctx, written)?
  let without = test.run_script(ctx, bare)?
  assert with_propagation.status == 3 and without.status == 3, with_propagation.stderr
  assert with_propagation.stdout == "out\ncleanup\n", with_propagation.stdout
  assert without.stdout == with_propagation.stdout
  # The same error, the same failing span (the run form without its `?`),
  # and the same call path.
  assert "`sh` exited 3" in without.stderr, without.stderr
  assert ":3:3-3:31" in with_propagation.stderr, with_propagation.stderr
  assert without_paths(without.stderr) == without_paths(with_propagation.stderr), without.stderr
}

test test_a_captured_plain_run_statement_is_the_same_error { |ctx|
  let written = "let outcome = try {\n  run sh -c \"exit 4\" | run cat ?\n  print \"after\"\n}\nmatch outcome {\n  Ok(_) => print \"ok\"\n  Err(problem) => print f\"{problem is ProcessError} {problem.message}\"\n}\n"
  let with_propagation = test.run_script(ctx, written)?
  let without = test.run_script(ctx, written.replace(" ?\n", "\n"))?
  assert with_propagation.success and without.success, with_propagation.stderr + without.stderr
  assert "pipeline segment 0 `sh` exited with status 4" in with_propagation.stdout, with_propagation.stdout
  assert without.stdout == with_propagation.stdout
}

test test_redundant_propagation_removes_it_from_a_plain_run_statement { |ctx|
  let source = r"""proc build(target: Str) -> Result[Str] {
  run sh -c "true" ?
  run sh -c "echo piped" | run cat ?
  let listed = run.text sh -c "echo listed" ?
  Ok(f"{target} {listed.trim()}")
}

let built = build("all")?
print $built
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "build.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.redundant-propagation $candidate
  assert first.stderr.split("`?` on a `run` statement that already fails with its command").len() == 3, first.stderr
  assert first.stderr.split("`?` on a run form that already fails with its command").len() == 2, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.redundant-propagation $candidate
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "  run sh -c \"true\"\n  run sh -c \"echo piped\" | run cat\n" in fixed, fixed
  # A capturing form fails with its command too.
  assert "  let listed = run.text sh -c \"echo listed\"\n" in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
  let formatted = run.capture --text "xsht" fmt --check $candidate
  assert formatted.status.exited_with(0), formatted.stderr
}

test test_prefer_propagation_rewrites_an_assigning_match { |ctx|
  let source = r"""pure parse(text: Str) -> Result[Int] {
  text.parse_int()
}

pure total(texts: List[Str]) -> Result[Int] {
  var sum = 0
  for text in texts {
    var value = 0
    match parse(text) {
      Ok(parsed) => value = parsed
      Err(problem) => return Err(problem)
    }

    sum += value
  }

  Ok(sum)
}

match total(["1", "2", "x"]) {
  Ok(sum) => print $sum
  Err(problem) => print $problem.message
}
print ${total(["1", "2"])?}
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "total.xsh", contents: bytes.from_text(source))?
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-propagation $candidate
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "    var value = 0\n    value = parse(text)?\n\n    sum += value\n" in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
}

test test_a_deferred_result_fails_the_same_with_and_without_propagation { |ctx|
  let written = r"""proc step(name: Str, bad: Bool) -> Result[Unit] {
  print f"step {name}"
  if bad {
    fail f"bad {name}"
  }
}

proc work(bad: Bool) -> Result[Int] {
  defer step("first", false)?
  errdefer step("on error", false)?
  defer step("second", bad)?
  defer step("third", bad)?
  print "body"
  Ok(1)
}

print ${work(false)?}
let failed = work(true)?
"""
  let bare = written.replace(")?\n  ", ")\n  ")
  assert bare.split("?").len() == 3, bare
  let with_propagation = test.run_script(ctx, written)?
  let without = test.run_script(ctx, bare)?
  assert with_propagation.status != 0 and without.status == with_propagation.status, with_propagation.stderr
  # Last registered runs first; the first failure is primary, the later one
  # is a reported cleanup failure, and the failure makes `errdefer` due.
  let order = "body\nstep third\nstep second\nstep first\n1\nbody\nstep third\nstep second\nstep on error\nstep first\n"
  assert with_propagation.stdout == order, with_propagation.stdout
  assert without.stdout == order, without.stdout
  assert "bad third" in without.stderr and "bad second" in without.stderr, without.stderr
  assert without_paths(without.stderr) == without_paths(with_propagation.stderr), without.stderr
}

test test_redundant_propagation_removes_it_from_a_deferred_call { |ctx|
  let source = r"""proc step(name: Str) -> Result[Unit] {
  print f"step {name}"
}

proc work() -> Result[Int] {
  defer step("first")?
  errdefer step("unused") ?
  defer {
    step("second")?
  }
  Ok(1)
}

print ${work()?}
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "work.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.redundant-propagation $candidate
  assert first.stderr.split("`?` on a deferred action that already fails with its `Result`").len() == 3, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.redundant-propagation $candidate
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "  defer step(\"first\")\n  errdefer step(\"unused\")\n  defer {\n    step(\"second\")\n  }\n" in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
  let formatted = run.capture --text "xsht" fmt --check $candidate
  assert formatted.status.exited_with(0), formatted.stderr
}
