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

test test_try_run_means_what_the_bare_form_means { |ctx|
  let bare = r"""let got = run.text sh -c "echo one; exit 2"
print f"{got is Err(_)}"
let ok = run.text sh -c "echo two"
print ${(ok ?? "none").trim()}
"""
  let written = r"""let got = try run.text sh -c "echo one; exit 2"
print f"{got is Err(_)}"
let ok = try run.text sh -c "echo two"
print ${(ok ?? "none").trim()}
"""
  let before = test.run_script(ctx, bare)?
  let after = test.run_script(ctx, written)?
  assert before.success and after.success, before.stderr + after.stderr
  assert before.stdout == "true\ntwo\n", before.stdout
  assert after.stdout == before.stdout
}

test test_try_run_rejects_a_propagating_or_status_form { |ctx|
  let _ = test.expect(
    ctx,
    "let text = try run.text sh -c \"echo x\" ?\n",
    status: 2,
    stderr: ["`try` keeps the run form's failure as a value, and `?` would propagate it"],
  )?
  let _ = test.expect(
    ctx,
    "let status = try run.status sh -c \"exit 3\"\n",
    status: 2,
    stderr: ["err[check.try-result]: `try` captures a run form that can fail, and this one yields `Status`"],
  )?
  let _ = test.expect(ctx, "let status = try run sh -c \"exit 3\"\n", status: 2, stderr: ["err[check.try-result]"])?
}

test test_explicit_run_capture_fix_writes_try_and_converges { |ctx|
  let source = r"""proc first_line(text: Str) -> Str {
  text.lines()[0]
}

let version = run.text sh -c "echo v1; echo more"
let missing = run.text sh -c "exit 9"
let propagated = run.text sh -c "echo v2" ?
let status = run.status sh -c "exit 1"
let shown = if let Ok(text) = run.text sh -c "echo v3" { first_line(text) } else { "none" }
print ${first_line(version ?? "none")} ${missing is Err(_)} ${propagated.trim()} ${status.success} $shown
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "capture.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.explicit-run-capture $candidate ?
  assert first.status.exited_with(1), first.stderr
  assert first.stderr.split("warn[lint.explicit-run-capture]").len() == 4, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.explicit-run-capture $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "let version = try run.text sh -c \"echo v1; echo more\"\n" in fixed, fixed
  assert "let missing = try run.text sh -c \"exit 9\"\n" in fixed, fixed
  # A form that propagates, and one that yields a `Status`, are unchanged.
  assert "let propagated = run.text sh -c \"echo v2\" ?\n" in fixed, fixed
  assert "let status = run.status sh -c \"exit 1\"\n" in fixed, fixed
  assert "let shown = if let Ok(text) = try run.text sh -c \"echo v3\" { first_line(text) }" in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
  let formatted = run.capture --text "xsht" fmt --check $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let expanded = run.capture --text "xsht" desugar $candidate ?
  assert "let version = try run.text sh -c" in expanded.stdout, expanded.stdout
}
