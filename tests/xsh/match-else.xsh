enum Level { Info, Warn, Fault(Str) }

pure statement_label(level: Level) -> Str {
  var label = "unset"
  match level {
    Fault(reason) => label = f"fault: {reason}"
    Warn => label = "warn"
    else => label = "fine"
  }

  label
}

pure value_label(level: Level) -> Str {
  let label = match level {
    Info => "info",
    else => "other",
  }
  label
}

test test_match_else_runs_only_when_no_earlier_arm_selects {
  assert statement_label(Fault("disk")) == "fault: disk"
  assert statement_label(Info) == "fine"
  assert statement_label(Warn) == "warn"
  assert value_label(Info) == "info"
  assert value_label(Warn) == "other"
  assert value_label(Fault("disk")) == "other"
}

test test_match_else_follows_guarded_and_wildcard_patterns {
  let pairs = [[1, 2], [3, 3], [0, 9]]
  var labels = [
    match pair {
      [0, _] => "zero",
      [a, b] if a == b => "same",
      else => "other",
    }
    for pair in pairs
  ]
  assert labels == ["other", "same", "zero"]
}

# An arm body that is an `if` without its own `else` ends at the line break:
# the `else =>` below it is the match's catch-all, not the `if`'s branch.
test test_match_else_after_an_if_arm_body {
  var seen = []
  for level in [Info, Warn] {
    match level {
      Info => if ! seen.is_empty() { seen += ["late info"] }
      else => seen += ["catch-all"]
    }
  }

  assert seen == ["catch-all"]
}

# `else` makes a value match exhaustive and a statement match over an enum
# complete, exactly as a `_` arm does.
test test_match_else_counts_as_the_catch_all { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "match-else-check.xsh",
    contents: bytes.from_text("""enum Level { Info, Warn, Fault(Str) }
const level: Level = Warn
match level {
  Info => print "info"
  else => {}
}
let label = match level { Info => "info", else => "other" }
print \$label
"""),
  )?
  let checked = run.capture --text --accept=[0, 1] "xsht" check $candidate ?
  let report = checked.stdout + checked.stderr
  assert checked.status.exited_with(0), report
  assert "check.non-exhaustive-match" not in report, report
  assert "check.match-value-exhaustive" not in report, report
}

test test_match_else_takes_no_guard { |ctx|
  let output = test.run_script(
    ctx,
    """let value = 3
match value {
  1 => print "one"
  else if value > 2 => print "big"
}
""",
  )?
  assert output.status != 0
  assert output.stdout == ""
  assert "parse.match-else-arm" in output.stderr, output.stderr
  assert "takes no guard" in output.stderr, output.stderr
  assert ":4:" in output.stderr, output.stderr
}

test test_match_else_must_be_the_last_arm { |ctx|
  for source in [
    """let value = 3
match value {
  else => print "other"
  1 => print "one"
}
""",
    """let value = 3
let label = match value {
  else => "other",
  1 => "one",
}
print \$label
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status != 0
    assert output.stdout == ""
    assert "parse.match-else-arm" in output.stderr, output.stderr
    assert "must be the last match arm" in output.stderr, output.stderr
    assert ":4:" in output.stderr, output.stderr
  }
}

# `else` is an arm head. Inside a pattern the wildcard is `_`.
test test_match_else_is_not_a_pattern { |ctx|
  let output = test.run_script(
    ctx,
    """enum Level { Info, Fault(Str) }
match Fault("disk") {
  Fault(else) => print "fault"
  Info => print "info"
}
""",
  )?
  assert output.status != 0
  assert output.stdout == ""
  assert "parse." in output.stderr, output.stderr
}

test test_match_else_formats_and_lints_as_written { |ctx|
  let source = """enum Level { Info, Warn, Fault(Str) }
const level: Level = Warn
match level {
  Info => print "info"
  else   =>   print "else"
}
match level {
  Fault(_) => print "fault"
  _ => print "wildcard"
}
let label = match level { Info => "info", _ => "other" }
print \$label
"""
  let before = test.expect(ctx, source, status: 0)?
  assert before.stdout == "else\nwildcard\nother\n"
  let candidate = test.temp_file(ctx, name: "match-else.xsh", contents: bytes.from_text(source))?

  # The formatter keeps whichever catch-all spelling was written.
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let text = candidate.read_text()?
  assert "  else => print \"else\"\n" in text, text
  assert "  _ => print \"wildcard\"\n" in text, text
  assert "_ => \"other\"" in text, text

  # The lint reports each last `_ =>` arm and never a wildcard inside a pattern.
  let linted = run.capture --text --accept=[0, 1] "xsht" lint $candidate ?
  let report = linted.stdout + linted.stderr
  assert report.split("lint.prefer-match-else").len() == 3, report
  let fixed = run.capture --text "xsht" lint --fix $candidate ?
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert "  else => print \"wildcard\"\n" in rewritten, rewritten
  assert "else => \"other\"" in rewritten, rewritten
  assert "Fault(_) =>" in rewritten, rewritten
  assert "_ =>" not in rewritten, rewritten

  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
}
