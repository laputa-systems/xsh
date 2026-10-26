test test_selected_lint_fix_applies_beside_an_unselected_check_error { |ctx|
  # The first match draws the error `check.non-exhaustive-match`, which the
  # selection leaves out. That error was there before the fix, so it is not a
  # new diagnostic and must not reject the fix to the second match.
  let source = r"""enum Tok { A, B, C }

proc show(t: Tok) {
  match t {
    A => print a
    B => print b
  }
}

pure pick(t: Tok) -> Int {
  let n = match t {
    A => 1,
    _ => 2,
  }
  n + 1
}

show(A)
print ${pick(B)}
"""
  let file = test.temp_file(ctx, name: "selected-fix.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-match-else $file
  assert fixed.status.exited_with(0), fixed.stderr
  # Only the selected code is reported, and only its edit is applied.
  assert "check.non-exhaustive-match" not in fixed.stderr, fixed.stderr
  assert file.read_text()? == source.replace("    _ => 2,", with: "    else => 2,")
}

test test_selected_check_fix_applies_beside_an_unselected_check_error { |ctx|
  # The selected fix puts `assert` on the Bool statement. The match keeps its
  # unselected check error, which the rewrite neither adds nor removes, and
  # an unrestricted run still reports it.
  let source = """enum Tok { A, B, C }

proc show(t: Tok) {
  t == A
  match t {
    A => print a
    B => print b
  }
}

show(A)
"""
  let file = test.temp_file(ctx, name: "selected-check-fix.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --fix --only check.bool-statement $file
  assert fixed.status.exited_with(0), fixed.stderr
  assert file.read_text()? == source.replace("  t == A", with: "  assert t == A")
  let all = run.capture --text --accept=[2] "xsht" lint $file
  assert "err[check.non-exhaustive-match]" in all.stderr, all.stderr
}

test test_check_warning_does_not_hide_lint_findings { |ctx|
  # The repeated arm draws the warning `check.unreachable-match-arm`. A check
  # warning leaves the checked facts whole, so the file is still linted: the
  # report holds both, and a selection sees the lint finding alone.
  let source = r"""enum Tok { A, B }

proc show(t: Tok) {
  match t {
    A => print a
    B => print b
    A => print again
  }
}

pure pick(t: Tok) -> Int {
  let n = match t {
    A => 1,
    _ => 2,
  }
  n + 1
}

show(A)
print pick(B)
"""
  let file = test.temp_file(ctx, name: "check-warning.xsh", contents: bytes.from_text(source))?
  let all = run.capture --text "xsht" lint $file
  assert "warn[check.unreachable-match-arm]" in all.stderr, all.stderr
  assert "warn[lint.prefer-match-else]" in all.stderr, all.stderr
  assert all.status.exited_with(1), all.stderr
  let selected = run.capture --text "xsht" lint --only lint.prefer-match-else $file
  assert "warn[lint.prefer-match-else]" in selected.stderr, selected.stderr
  assert "check.unreachable-match-arm" not in selected.stderr, selected.stderr
  assert selected.status.exited_with(1), selected.stderr
}

test test_lint_failure_status_survives_notes_in_either_file_order { |ctx|
  let warning = "proc unused() { print warning }\n"
  let checked_error = "let value: Int = \"wrong\"\nprint value\n"
  let note = "proc launch(argv: List[Str]) { run @argv }\nlaunch([\"true\"])\n"
  let cases = [
    {sources: [warning], expected: 1, diagnostics: ["warn[lint.unused-callable]"]},
    {sources: [checked_error], expected: 2, diagnostics: ["err[check.type-mismatch]"]},
    {sources: [warning, checked_error], expected: 2, diagnostics: ["warn[lint.unused-callable]", "err[check.type-mismatch]"]},
    {sources: [checked_error, warning], expected: 2, diagnostics: ["err[check.type-mismatch]", "warn[lint.unused-callable]"]},
  ]
  for fix in [false, true] {
    for notes_first in [false, true] {
      for {sources, expected, diagnostics} in cases {
        let root = test.temp_dir(ctx, name: "lint-status-precedence")?
        let prefix = if notes_first { "a" } else { "z" }
        fp"{root}/{prefix}-notes.xsh".write(note)
        var index = 0
        for source in sources {
          fp"{root}/m-{index}.xsh".write(source)
          index += 1
        }
        let args = if fix { ["lint", "--fix"] } else { ["lint"] }
        let output = run.capture --text --accept=[0, 1, 2] "xsht" @args $root
        assert output.status.exited_with(expected), f"fix={fix} notes_first={notes_first} expected={expected}: {output.stderr}"
        for diagnostic in diagnostics {
          assert diagnostic in output.stderr, output.stderr
        }
        assert "note[lint.prefer-non-empty-argv]" in output.stderr, output.stderr
      }
    }
  }
}
