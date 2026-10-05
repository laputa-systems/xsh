test test_selected_lint_fix_applies_beside_an_unselected_check_warning { |ctx|
  # The first match draws `check.non-exhaustive-match`, which the selection
  # leaves out. That warning was there before the fix, so it is not a new
  # diagnostic and must not reject the fix to the second match.
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
  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-match-else $file ?
  assert fixed.status.exited_with(0), fixed.stderr
  # Only the selected code is reported, and only its edit is applied.
  assert "check.non-exhaustive-match" not in fixed.stderr, fixed.stderr
  assert file.read_text()? == source.replace("    _ => 2,", with: "    else => 2,")
}

test test_selected_check_fix_applies_beside_an_unselected_check_warning { |ctx|
  # The selected fix puts `assert` on the Bool statement. The match keeps its
  # unselected check warning, which the rewrite neither adds nor removes, and
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
  let fixed = run.capture --text "xsht" lint --fix --only check.bool-statement $file ?
  assert fixed.status.exited_with(0), fixed.stderr
  assert file.read_text()? == source.replace("  t == A", with: "  assert t == A")
  let all = run.capture --text "xsht" lint $file ?
  assert "check.non-exhaustive-match" in all.stderr, all.stderr
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
  let all = run.capture --text "xsht" lint $file ?
  assert "warn[check.unreachable-match-arm]" in all.stderr, all.stderr
  assert "warn[lint.prefer-match-else]" in all.stderr, all.stderr
  assert all.status.exited_with(2), all.stderr
  let selected = run.capture --text "xsht" lint --only lint.prefer-match-else $file ?
  assert "warn[lint.prefer-match-else]" in selected.stderr, selected.stderr
  assert "check.unreachable-match-arm" not in selected.stderr, selected.stderr
  assert selected.status.exited_with(1), selected.stderr
}
