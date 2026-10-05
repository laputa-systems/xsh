# `xsht desugar SCRIPT` prints the script with every sugar statement replaced
# by its expansion.

const sugared = """type Row = {kind: Str, size: Int}

# Sums the files.
proc tally(rows: List[Row], limit: Int) -> Int {
  var total = 0
  for row in rows {
    # directories have no size
    continue unless row.kind == "file"
    break when total > limit # stop early
    guard row.size < 100 else {
      # too large to count
      return -1
    }

    repeat 2 times {
      total += row.size
    }
  }

  total
}

print tally([Row(kind: "dir", size: 9), Row(kind: "file", size: 3)], 10)
print tally([Row(kind: "file", size: 300)], 10)
"""

proc desugared(ctx: TestContext, source: Str) [fs, process, error] -> Result[Str] {
  let script = test.temp_file(ctx, name: "sugared.xsh", contents: bytes.from_text(source))?
  let result = run.capture --text "xsht" desugar $script
  assert result.status.exited_with(0), result.stderr
  assert result.stderr == ""
  result.stdout
}

test test_desugar_prints_each_expansion_with_its_comments { |ctx|
  let output = desugared(ctx, sugared)?
  assert """  for row in rows {
    # directories have no size
    if row.kind == "file" {} else {
      continue
    }

    if total > limit { break } # stop early
    if row.size < 100 {} else {
      # too large to count
      return -1
    }

    for _ in range(2) {
      total += row.size
    }
  }
""" in output, output
  assert output.starts_with("type Row = {kind: Str, size: Int}\n\n# Sums the files.\n"), output
}

test test_desugared_script_checks_formats_and_runs_like_the_script { |ctx|
  let output = desugared(ctx, sugared)?
  let candidate = test.temp_file(ctx, name: "desugared.xsh", contents: bytes.from_text(output))?
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stdout
  assert desugared(ctx, output)? == output

  let before = test.run_script(ctx, sugared)?
  let after = test.run_script(ctx, output)?
  assert before.success, before.stderr
  assert after.success, after.stderr
  assert before.stdout == "6\n-1\n"
  assert after.stdout == before.stdout
}

test test_desugar_leaves_the_script_alone_and_refuses_one_that_does_not_parse { |ctx|
  let script = test.temp_file(
    ctx,
    name: "kept.xsh",
    contents: bytes.from_text("""repeat 2 times { print "tick" }
"""),
  )?
  let printed = run.capture --text "xsht" desugar $script
  assert printed.stdout == """for _ in range(2) { print "tick" }
""", printed.stdout
  assert script.read_text()? == """repeat 2 times { print "tick" }
"""

  let broken = fp"{test.temp_dir(ctx, name: "refused")?}/broken.xsh"
  broken.write("""return 1 when
""")
  let refused = run.capture --text "xsht" desugar $broken
  assert refused.status.exited_with(1), refused.stderr
  assert refused.stdout == ""
  assert "err[parse." in refused.stderr, refused.stderr
  assert "broken.xsh:" in refused.stderr, refused.stderr
}
