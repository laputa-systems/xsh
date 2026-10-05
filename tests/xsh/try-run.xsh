pure outcome(result: Result[Str, ProcessError]) -> Str {
  if let Ok(text) = result {
    f"ok {text.trim()}"
  } else {
    "failed"
  }
}

test test_try_run_is_the_run_form_result_as_a_value {
  let kept = try run.text sh -c "echo kept"
  assert outcome(kept) == "ok kept"
  let failed = try run.text sh -c "exit 3"
  assert outcome(failed) == "failed"
  assert outcome((try run.text sh -c "echo grouped")) == "ok grouped"
  let fallback = (try run.text sh -c "exit 1") ?? "fallback"
  assert fallback == "fallback"
  let record = try run.capture --text sh -c "echo out; exit 4"
  match record {
    Ok(captured) => assert captured.stdout == "out\n" and ! captured.status.exited_with(0)
    Err(problem) => test.fail(problem.message)
  }

  let subject = if let Ok(output) = try run.bytes sh -c "printf ab" { output.len() } else { 0 }
  assert subject == 2
}

test test_a_bare_capturing_run_form_fails_its_function { |ctx|
  let bare = r"""proc describe(code: Int) -> Result[Str] {
  defer { print "cleanup" }
  let text = run.text sh -c f"echo out; exit {code}"
  print "after"
  Ok(text.trim())
}

match describe(3) {
  Ok(text) => print $text
  Err(problem) => print ${"exited with status 3" in problem.message}
}
print ${describe(0)?}
let kept = try run.text sh -c "exit 4"
print f"{kept is Err(_)}"
let status = run sh -c "exit 5"
print f"{status.success}"
let record = run.capture --text sh -c "echo captured; exit 6"
print ${record.stdout.trim()} ${record.status.exited_with(6)}
let lines = run.stream --text sh -c "echo a; echo b" |> collect()
print ${lines.len()}
let top = run.text sh -c "echo top; exit 7"
print "unreachable"
"""
  # The bare form and the form with `?` are one program: the same output,
  # the same error, and the same failing span.
  let written = bare.replace("exit {code}\"\n", with: "exit {code}\" ?\n").replace("exit 7\"\n", with: "exit 7\" ?\n")
  assert written.split(" ?\n").len() == 3, written
  let ran = test.expect(ctx, bare, status: 3, stderr: ["`sh` exited 7", ":21:1-"])?
  assert ran.stdout == "cleanup\ntrue\nafter\ncleanup\nout\ntrue\nfalse\ncaptured true\n2\n", ran.stdout
  let propagated = test.expect(ctx, written, status: 3)?
  assert propagated.stdout == ran.stdout
  assert propagated.stderr.replace("script.xsh-2", with: "script.xsh-1") == ran.stderr, propagated.stderr
}

test test_a_bare_capturing_run_form_needs_a_function_that_can_fail { |ctx|
  let ran = test.expect(
    ctx,
    "proc count() [process] -> Int {\n  let text = run.text sh -c \"echo 1\"\n  text.byte_len()\n}\n\nprint f\"{count()}\"\n",
    status: 2,
    stderr: [
      "err[check.effect-violation]: `?` requires the `error` effect",
      "err[check.try-context]: `?` requires a Result-returning context",
      "note: a capturing run form propagates a failed command as `?` does; write `try run...` to keep the failure as a value",
    ],
  )?
  assert ran.stdout == "", ran.stdout
  # Under `try` the failure is a value, and the function needs neither.
  let kept = test.expect(
    ctx,
    "proc count() [process] -> Int {\n  let text = try run.text sh -c \"echo 1\"\n  (text ?? \"\").byte_len()\n}\n\nprint f\"{count()}\"\n",
    status: 0,
  )?
  assert kept.stdout == "2\n", kept.stdout
}

test test_a_try_block_captures_the_run_form_it_holds { |ctx|
  let source = r"""let outcome = try {
  run.text sh -c "echo one; exit 2"
}
match outcome {
  Ok(text) => print $text
  Err(problem) => print ${"exited with status 2" in problem.message}
}
let nested = try {
  try run.text sh -c "exit 2"
}
match nested {
  Ok(inner) => print f"{inner is Err(_)}"
  Err(_) => print "outer"
}
"""
  let ran = test.expect(ctx, source, status: 0)?
  assert ran.stdout == "true\ntrue\n", ran.stdout
}

test test_try_run_rejects_a_propagating_or_status_form { |ctx|
  test.expect(
    ctx,
    "let text = try run.text sh -c \"echo x\" ?\n",
    status: 2,
    stderr: ["`try` keeps the run form's failure as a value, and `?` would propagate it"],
  )?
  test.expect(
    ctx,
    "let status = try run.status sh -c \"exit 3\"\n",
    status: 2,
    stderr: ["err[check.try-result]: `try` captures a run form that can fail, and this one yields `Status`"],
  )?
  test.expect(ctx, "let status = try run sh -c \"exit 3\"\n", status: 2, stderr: ["err[check.try-result]"])?
}

test test_redundant_propagation_removes_it_from_a_capturing_run_form { |ctx|
  let source = r"""proc first_line(text: Str) -> Str {
  text.lines()[0]
}

pure both(head: Str, tail: Str) -> Str {
  head + tail
}

let version = run.text sh -c "echo v1; echo more" ?
let words = (run.text sh -c "echo a b" ?).split(" ")
let listed = run.stream --text sh -c "echo x; echo y" ? |> collect()
let joined = both(run.text sh -c "printf j"?, "!")
let kept = try run.text sh -c "exit 9"
let status = run.status sh -c "exit 1" ?
var label = ""
label = run.text sh -c "printf set" ? when status.success
print ${first_line(version)} ${words.len()} ${listed.len()} $joined ${kept is Err(_)} ${status.success} $label
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "capture.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.redundant-propagation $candidate
  assert first.status.exited_with(1), first.stderr
  assert first.stderr.split("`?` on a run form that already fails with its command").len() == 4, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.redundant-propagation $candidate
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "let version = run.text sh -c \"echo v1; echo more\"\n" in fixed, fixed
  assert "let words = (run.text sh -c \"echo a b\").split(\" \")\n" in fixed, fixed
  assert "let listed = run.stream --text sh -c \"echo x; echo y\" |> collect()\n" in fixed, fixed
  # A `?` that also ends the run form, a status form's `?`, and one a guard
  # follows are unchanged.
  assert "let joined = both(run.text sh -c \"printf j\"?, \"!\")\n" in fixed, fixed
  assert "let status = run.status sh -c \"exit 1\" ?\n" in fixed, fixed
  assert "label = run.text sh -c \"printf set\" ? when status.success\n" in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
  let formatted = run.capture --text "xsht" fmt --check $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let again = run.capture --text "xsht" lint --only lint.redundant-propagation $candidate
  assert again.status.exited_with(0), again.stderr
}
