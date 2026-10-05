type Captured = {status: Status, stdout: Str, stderr: Str}

# Writes `source` to a fresh script outside any project configuration.
proc script(ctx: TestContext, source: Str) [fs, error] -> Result[Path] {
  test.temp_file(ctx, name: "lint.xsh", contents: bytes.from_text(source))
}

# Runs `xsht lint` on `file` with `flags` and requires that the file parsed
# and checked, so that an absent finding is a statement about the linter.
# It runs in the directory of `file`, where no project configuration applies:
# the runner's own directory is the repository, whose configuration turns
# some default rules off.
proc lint(file: Path, flags: List[Str]) [process, env, error] -> Result[Captured] {
  let linted = cd (file.parent()) {
    run.capture --text "xsht" lint @flags $file
  }?

  assert linted.status.exited_with(0) or linted.status.exited_with(1), linted.stderr
  Ok(linted)
}

# How many findings with `code` the default rule set reports for `file`.
proc findings(file: Path, code: Str) [process, env, error] -> Result[Int] {
  let linted = lint(file, [])?
  Ok(linted.stderr.split(f"[{code}]").len() - 1)
}

# The text of `file` after the fixes of `code` alone were applied.
proc fixed(file: Path, code: Str) [fs, process, env, error] -> Result[Str] {
  let _ = lint(file, ["--fix", "--only", code])?
  file.read_text()
}

# Requires that `file` checks.
proc assert_checked(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Requires that `file` checks and is already in formatter layout.
proc assert_checked_and_formatted(file: Path) [process, env, error] {
  assert_checked(file)
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

# Requires that the fixes of every default rule together leave `source` a
# program that checks.
proc assert_all_fixes_check(ctx: TestContext, source: Str) [fs, process, env, error] {
  let file = script(ctx, source)?
  let _ = lint(file, ["--fix"])?
  assert_checked(file)
}

test test_multi_clause_list_accumulator_fix_checks_and_converges { |ctx|
  let source = "let groups = [[1, 2], [3]]\nvar values: List[Int] = []\n\nfor batch in groups {\n  if batch.len() > 0 {\n    for value in batch {\n      if value > 1 {\n        values = values.push(value)\n      }\n    }\n  }\n}\n"
  let file = script(ctx, source)?
  assert findings(file, "lint.prefer-list-comp")? == 1
  assert fixed(file, "lint.prefer-list-comp")? != source
  assert_checked_and_formatted(file)
  assert findings(file, "lint.prefer-list-comp")? == 0
  assert_all_fixes_check(ctx, source)
}

test test_multi_clause_map_accumulator_fix_keeps_annotation_and_filters { |ctx|
  let source = "let entries = [{key: \"a\", values: [1, 2]}]\nvar values: Map[Int] = {}\nfor entry in entries {\n  for value in entry.values {\n    if value > 1 {\n      values[entry.key] = value\n    }\n  }\n}\n"
  let file = script(ctx, source)?
  assert findings(file, "lint.prefer-map-comp")? == 1
  let text = fixed(file, "lint.prefer-map-comp")?
  assert "var values: Map[Int] = {\n" in text, text
  assert_checked_and_formatted(file)
  assert_all_fixes_check(ctx, source)
}

test test_map_entry_iteration_fix_keeps_surrounding_text_and_converges { |ctx|
  let file = script(
    ctx,
    "# 源\nproc render(counts: Map[Int]) [error] -> List[Str] {\n  var output: List[Str] = []\n  for key in counts.keys() {\n    let count = counts.get(key)?\n    output += [f\"{key}={count}\"]\n  }\n\n  return output\n}\n",
  )?
  assert findings(file, "lint.prefer-map-entry-iteration")? == 1
  let text = fixed(file, "lint.prefer-map-entry-iteration")?
  assert "for {key, value: count} in counts" in text, text
  assert "counts.get(key)" not in text, text
  assert text.starts_with("# 源\nproc render(counts: Map[Int]) [error] -> List[Str] {\n"), text
  assert_checked_and_formatted(file)
  assert findings(file, "lint.prefer-map-entry-iteration")? == 0
}

test test_map_entry_iteration_has_no_fix_when_a_value_block_mutates_the_map { |ctx|
  let source = "var counts: Map[Int] = {a: 1, b: 2}\nfor key in counts.keys() {\n  let count = counts.get(key)?\n  let changed = if true { counts[\"b\"] = 9; 0 } else { 0 }\n  print f\"{key}={count}\"\n  let _ = changed\n}\n"
  let snapshot = source.replace(
    "for key in counts.keys() {\n  let count = counts.get(key)?",
    with: "for {key, value: count} in counts {",
  )
  assert snapshot != source

  # Entry iteration reads a snapshot, so the rewrite would change the output.
  for case in [{script: source, expected: "a=1\nb=9\n"}, {script: snapshot, expected: "a=1\nb=2\n"}] {
    let traced = run.capture --text "xsht" trace ${script(ctx, case.script)?}
    assert traced.status.exited_with(0), traced.stderr
    assert traced.stdout == case.expected, traced.stdout
  }

  let file = script(ctx, source)?
  assert fixed(file, "lint.prefer-map-entry-iteration")? == source
}

test test_list_splicing_fix_rechecks_keeps_unicode_and_converges { |ctx|
  let file = script(
    ctx,
    "# café\nlet flags = [\"-g\"]\nlet names = [\"main.xsh\"]\nlet argv = [\"cc\"].extend(flags).extend([\"-o\", \"app\"]).extend(names)\nprint argv.len()\n",
  )?
  assert findings(file, "lint.prefer-list-splicing")? > 0
  let text = fixed(file, "lint.prefer-list-splicing")?
  assert "[\"cc\", @flags, \"-o\", \"app\", @names]" in text, text
  assert text.starts_with("# café\n"), text
  assert_checked_and_formatted(file)
  assert findings(file, "lint.prefer-list-splicing")? == 0
}

test test_list_splicing_keeps_nested_elements_and_local_updates { |ctx|
  let file = script(
    ctx,
    "let groups = [[1]].extend([[2]]).extend([[3]])\nvar values = [1]\nvalues = values.extend([2])\nlet nested = groups.push([4])\nprint groups.len() nested.len()\n",
  )?
  let reported = lint(file, [])?
  assert reported.stderr.split("[lint.prefer-list-splicing]").len() == 2, reported.stderr
  assert "-> [[1], [2], [3]]\n" in reported.stderr, reported.stderr
}

test test_list_splicing_refuses_annotation_conversions { |ctx|
  let file = script(
    ctx,
    "type Row = {value: Int}\nlet left: List[Row] = [{value: 1}]\nlet right: List[Row] = [{value: 2}]\nlet combined = left.extend(right).extend(left)\nprint combined.len()\n",
  )?
  assert findings(file, "lint.prefer-list-splicing")? == 0
}

test test_list_splicing_reports_a_commented_construction_without_a_fix { |ctx|
  let source = "let argv = [\"head\"] + (if true { # retain this explanation\n  [\"tail\"]\n} else { [\"other\"] })\nprint argv.len()\n"
  let file = script(ctx, source)?
  assert findings(file, "lint.prefer-list-splicing")? > 0
  assert fixed(file, "lint.prefer-list-splicing")? == source
}

test test_fmt_list_pattern_nested_rest_and_comments_are_stable { |ctx|
  let file = script(ctx, p"tests/fixtures/syntax/list-pattern.xsh".read_text()?)?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  let text = file.read_text()?
  assert "[\"build\", _, ..]" in text, text
  assert "# Keep the selected command explanation." in text, text
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
}

test test_list_pattern_keeps_unsafe_bounds_mutability_annotations_and_comments { |ctx|
  let file = script(ctx, p"tests/fixtures/lint/list-pattern-unsafe.xsh".read_text()?)?
  assert findings(file, "lint.prefer-list-pattern")? == 0
}

test test_list_pattern_fix_rewrites_stable_bounded_extraction_and_converges { |ctx|
  let file = script(ctx, p"tests/fixtures/lint/list-pattern.xsh".read_text()?)?
  assert findings(file, "lint.prefer-list-pattern")? == 2
  let text = fixed(file, "lint.prefer-list-pattern")?
  assert "if let [\"build\", target] = values" in text, text
  assert "if let [7, target, ..] = values" in text, text
  assert_checked(file)
  assert findings(file, "lint.prefer-list-pattern")? == 0
}

test test_list_pattern_reachability_uses_unguarded_coverage {
  let checked = run.capture --text "xsht" check tests/fixtures/frontend-indexed/list-pattern-unreachable.xsh
  assert checked.stderr.split("[check.unreachable-match-arm]").len() == 2, checked.stderr
}
