# A note is advice with no safe rewrite: `xsht lint` prints and counts it and
# still succeeds. `lint.prefer-non-empty-argv` is such a rule.
const NOTED = r"""proc launch(argv: List[Str]) {
  run @argv
}

launch(["true"])
"""

test test_lint_note_is_reported_without_failing_the_run { |ctx|
  let file = test.temp_file(ctx, name: "noted.xsh", contents: bytes.from_text(NOTED))?
  let linted = run.capture --text --accept=[0, 1] "xsht" lint $file ?
  assert linted.status.exited_with(0), linted.stderr
  assert "note[lint.prefer-non-empty-argv]" in linted.stderr, linted.stderr
  assert "xsht lint: 0 findings, 1 note\n" in linted.stderr, linted.stderr

  # `--only` selects a note like any other code.
  let selected = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-non-empty-argv $file ?
  assert selected.status.exited_with(0), selected.stderr
  assert "note[lint.prefer-non-empty-argv]" in selected.stderr, selected.stderr
  let other = run.capture --text --accept=[0, 1] "xsht" lint --only lint.unused-callable $file ?
  assert other.status.exited_with(0), other.stderr
  assert "lint.prefer-non-empty-argv" not in other.stderr, other.stderr
  assert "0 findings" not in other.stderr, other.stderr
}

test test_lint_deny_notes_fails_on_a_note { |ctx|
  let file = test.temp_file(ctx, name: "denied.xsh", contents: bytes.from_text(NOTED))?
  let denied = run.capture --text --accept=[0, 1] "xsht" lint --deny-notes $file ?
  assert denied.status.exited_with(1), denied.stderr
  assert "note[lint.prefer-non-empty-argv]" in denied.stderr, denied.stderr

  # A fix run has nothing to write for a note, and reports it the same way.
  let fixed = run.capture --text --accept=[0, 1] "xsht" lint --fix $file ?
  assert fixed.status.exited_with(0), fixed.stderr
  assert "note[lint.prefer-non-empty-argv]" in fixed.stderr, fixed.stderr
  assert file.read_text()? == NOTED
}

test test_lint_note_beside_a_finding_is_counted_apart { |ctx|
  let source = NOTED + "\nproc unused() {\n  print x\n}\n"
  let file = test.temp_file(ctx, name: "mixed.xsh", contents: bytes.from_text(source))?
  let linted = run.capture --text --accept=[0, 1] "xsht" lint $file ?
  assert linted.status.exited_with(1), linted.stderr
  assert "warn[lint.unused-callable]" in linted.stderr, linted.stderr
  assert "xsht lint: 1 finding, 1 note\n" in linted.stderr, linted.stderr
}

test test_check_is_unaffected_by_lint_notes { |ctx|
  let file = test.temp_file(ctx, name: "checked.xsh", contents: bytes.from_text(NOTED))?
  let checked = run.capture --text --accept=[0, 1] "xsht" check $file ?
  assert checked.status.exited_with(0), checked.stderr
  assert "lint.prefer-non-empty-argv" not in checked.stderr, checked.stderr
}

# `?? ""` makes a missing value and an empty one the same text, and a test of
# the binding for emptiness then asks about both.
test test_empty_sentinel_is_noted_and_left_as_written { |ctx|
  let source = r"""pure describe(titles: Map[Str], key: Str) -> Str {
  let title = titles.get(key) ?? ""
  return "untitled" when title.is_empty()
  title
}

print describe({draft: ""}, "draft")
"""
  let file = test.temp_file(ctx, name: "sentinel.xsh", contents: bytes.from_text(source))?
  let linted = run.capture --text --accept=[0, 1] "xsht" lint $file ?
  assert linted.status.exited_with(0), linted.stderr
  assert "note[lint.empty-sentinel]" in linted.stderr, linted.stderr
  assert "`if let Ok(title) = titles.get(key) { ... }`" in linted.stderr, linted.stderr

  let fixed = run.capture --text --accept=[0, 1] "xsht" lint --fix --only lint.empty-sentinel $file ?
  assert fixed.status.exited_with(0), fixed.stderr
  assert file.read_text()? == source

  # A binding that is used without the test is an ordinary default.
  let plain = source.replace("  return \"untitled\" when title.is_empty()\n", with: "")
  let quiet_file = test.temp_file(ctx, name: "default.xsh", contents: bytes.from_text(plain))?
  let quiet = run.capture --text --accept=[0, 1] "xsht" lint $quiet_file ?
  assert quiet.status.exited_with(0), quiet.stderr
  assert "lint.empty-sentinel" not in quiet.stderr, quiet.stderr
}
