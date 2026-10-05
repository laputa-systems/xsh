type Captured = {status: Status, stdout: Str, stderr: Str}

# Writes `source` to a fresh script in a directory of its own.
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

# Requires that `file` parses and checks.
proc assert_checked(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Formats `file` in place, requires a second pass to change nothing, and
# returns the formatted text.
proc formatted(file: Path) [fs, process, env, error] -> Result[Str] {
  let written = run.capture --text "xsht" fmt $file
  assert written.status.exited_with(0), written.stderr
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
  file.read_text()
}

test test_context_scope_scaffold_fix_keeps_the_checked_value_type_and_converges { |ctx|
  let source = "proc example() [env, error] {\n  var selected = \"\"\n  env ({XSH_SCOPE: \"inner\"}) { selected = env.get(\"XSH_SCOPE\")? }?\n  print \$selected\n}\n"
  let file = script(ctx, source)?
  assert findings(file, "lint.prefer-context-scope-value")? == 1
  assert fixed(file, "lint.prefer-context-scope-value")? != source
  assert_checked(file)
  let _ = formatted(file)?
  assert findings(file, "lint.prefer-context-scope-value")? == 0
}

test test_context_scope_scaffold_declines_cleanup_comments_and_placeholder_reads { |ctx|
  for source in [
    "on TERM [env, error] { let ignored = env.get(\"X\")?; }\nproc example() [env, error] { var selected = \"\"; env ({X: \"inner\"}) { selected = env.get(\"X\")? }?; print \$selected }\n",
    "proc example() [env, error] { var selected = \"\"; defer { print \$selected }; env ({X: \"inner\"}) { selected = env.get(\"X\")? }?; print \$selected }\n",
    "proc example() [env, error] { var selected = \"\"; env ({X: \"inner\"}) { # assignment timing\n selected = env.get(\"X\")? }?; print \$selected }\n",
    "proc example() [env, error] { var selected = \"\"; env ({X: selected}) { selected = env.get(\"X\")? }?; print \$selected }\n",
  ] {
    let file = script(ctx, source)?
    assert fixed(file, "lint.prefer-context-scope-value")? == source, source
  }
}

test test_env_scope_migration_fix_keeps_comments_and_rechecks { |ctx|
  let source = "env {\n  X = \"one\" # selected once\n  Y = 2;\n} { print \${env.get(\"X\")?} }?\n"
  let file = script(ctx, source)?
  let reported = run.capture --text "xsht" check $file
  assert reported.status.exited_with(2), reported.stderr
  assert reported.stderr.split("err[parse.env-scope-migration]").len() == 2, reported.stderr
  assert reported.stderr.split("err[").len() == 2, reported.stderr

  # `xsht lint` reports the same parser finding under its migration code.
  let _ = lint(file, ["--fix", "--only", "lint.env-scope"])?
  let text = file.read_text()?
  assert text != source
  assert "# selected once" in text, text
  assert_checked(file)
  let _ = formatted(file)?
}

test test_generic_record_constructor_alias_fix_rechecks_and_converges { |ctx|
  let source = "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet count = Count(value: 7)\nprint \${count.value + 1}\n"
  let file = script(ctx, source)?
  assert findings(file, "lint.prefer-generic-record-constructor")? == 1
  let text = fixed(file, "lint.prefer-generic-record-constructor")?
  assert "type Count = Box[Int]" in text, text
  assert "let count = Box(value: 7)" in text, text
  assert_checked(file)
  assert findings(file, "lint.prefer-generic-record-constructor")? == 0
}

test test_generic_record_constructor_alias_keeps_conversions_and_ambiguous_evidence { |ctx|
  for source in [
    "type Box[T] = {value: T?}\ntype Count = Box[Int]\nlet count = Count(value: null)\n",
    "type Box[T] = {value: List[T]}\ntype Count = Box[Int]\nlet count = Count(value: [])\n",
    "type Box[T] = {value: T}\ntype Count = Box[UInt]\nlet count = Count(value: 7)\n",
    "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet count = Count(...{value: 7})\n",
    "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet count = Count(\n# preserve this argument comment\nvalue: 7)\n",
  ] {
    let file = script(ctx, source)?
    assert findings(file, "lint.prefer-generic-record-constructor")? >= 1, source
    assert fixed(file, "lint.prefer-generic-record-constructor")? == source, source
  }
}

test test_default_parameter_annotation_fixes_recheck_keep_comments_and_converge { |ctx|
  let source = "# café precedes every edit.\nconst defaults = {jobs: 4}\npure next() -> Int { 3 }\npure choose(jobs: Int = defaults.jobs + 1, value: Int = next()) -> Int {\n  # café remains attached to the body.\n  jobs + value\n}\nlet result = choose(value: 7)\n"
  let file = script(ctx, source)?
  assert findings(file, "lint.default-param-type")? == 2
  let text = fixed(file, "lint.default-param-type")?
  assert "jobs = defaults.jobs + 1, value = next()" in text, text
  assert "# café remains attached to the body." in text, text
  assert_checked(file)
  assert findings(file, "lint.default-param-type")? == 0
}

test test_inferred_require_target_fix_keeps_validation_and_converges { |ctx|
  let file = script(
    ctx,
    "type Manifest = {jobs: UInt}\nlet raw: Any = {jobs: 4}\nlet value: Manifest = raw.require(Manifest)?\n",
  )?
  assert findings(file, "lint.inferred-require-target")? == 1
  let text = fixed(file, "lint.inferred-require-target")?
  assert "raw.require()?" in text, text
  assert_checked(file)
  assert findings(file, "lint.inferred-require-target")? == 0
}

test test_inferred_require_target_keeps_unanchored_and_different_instances { |ctx|
  for source in [
    "type Manifest = {jobs: UInt}\nlet raw: Any = {jobs: 4}\nlet value = raw.require(Manifest)?\n",
    "type Marker[T] = {name: Str}\nlet raw: Any = {name: \"ready\"}\nlet value: Marker[Int] = raw.require(Marker[Str])?\n",
  ] {
    assert findings(script(ctx, source)?, "lint.inferred-require-target")? == 0, source
  }
}

test test_inferred_require_formatting_round_trip_and_comments_keep_the_operation { |ctx|
  let file = script(
    ctx,
    "type Row = {name: Str}\nlet raw: Any = {name: \"ready\"}\nlet inferred: Row = raw.require()?\nlet explicit: Row = raw.require(\n  # Preserve the boundary explanation.\n  Row\n)?\n",
  )?
  assert findings(file, "lint.inferred-require-target")? == 0
  let text = formatted(file)?
  assert "raw.require()?" in text, text
  assert "# Preserve the boundary explanation." in text, text
  assert_checked(file)
}

test test_local_inference_annotation_fix_keeps_checked_expression_types_and_converges { |ctx|
  for source in [
    "proc gather() -> List[Path] {\n  # preserve initializer evidence\n  var entries: List[Path] = []\n  for destination in [p\"one\"] {\n    entries += [destination]\n  }\n\n  entries\n}\n",
    "proc choose() -> Path? {\n  var selected: Path? = null\n  for destination in [p\"one\"] {\n    selected = destination\n  }\n\n  selected\n}\n",
    "pure size(items: List[Path]) -> Int { items.len() }\nproc count() -> Int { let entries: List[Path] = []; size(entries) }\n",
  ] {
    let file = script(ctx, source)?
    assert findings(file, "lint.needless-annotation")? >= 1, source
    let text = fixed(file, "lint.needless-annotation")?
    assert text != source, source
    let comment = "# preserve initializer evidence"
    assert comment not in source or comment in text, text
    assert_checked(file)
    assert findings(file, "lint.needless-annotation")? == 0, text
    let _ = formatted(file)?
    assert_checked(file)
  }
}

test test_local_inference_annotation_fix_requires_an_identical_material_contract { |ctx|
  for source in [
    "proc inspect() -> Unit { let entries: List[Path] = []; print entries.len() }\n",
    "proc choose() -> Unit { var selected: Path? = null; print selected }\n",
    "proc choose() -> Any { var selected: Any = null; selected = 12; selected }\n",
    "proc gather() -> List[UInt] { var entries: List[UInt] = []; entries += [12]; entries }\n",
    "proc counts() -> Map[Int] { let entries: Map[Int] = map.empty(); entries }\n",
    "let entries: List[Path] = []\nprint entries.len()\n",
  ] {
    assert fixed(script(ctx, source)?, "lint.needless-annotation")? == source, source
  }
}
