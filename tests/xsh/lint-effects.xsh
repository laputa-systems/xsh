type Captured = {status: Status, stdout: Str, stderr: Str}

const rule = "lint.missing-effects"

# Writes each `files` entry, a path below the project root and its text, into
# a fresh directory.
proc project(ctx: TestContext, files: Map[Str, Str]) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "project")?
  for name in files.keys() {
    let file = fp"{root}/{name}"
    file.parent().mkdir()
    file.write(files[name])
  }

  Ok(root)
}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Requires `file` to check without a diagnostic and to be laid out as
# `xsht fmt` prints it.
proc assert_checks_and_is_formatted(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

# Requires the effect clause of `file` to be a check error that the fix of
# the missing-effects rule repairs by turning `source` into `expected`.
proc assert_effects_fix(file: Path, source: Str, expected: Str) [fs, process, env, error] {
  let before = run.capture --text "xsht" check $file
  assert "err[check.effect-violation]" in before.stderr, before.stderr
  let fixed = lint(file, ["--fix", "--only", rule])?
  assert fixed.status.exited_with(0), fixed.stderr
  assert file.read_text()? == expected, f"{source}{fixed.stderr}"
  assert_checks_and_is_formatted(file)
  let again = lint(file, ["--only", rule])?
  assert again.status.exited_with(0), again.stderr
  assert rule not in again.stderr, again.stderr
}

test test_missing_declared_effects_fix_adds_the_effect_the_body_needs { |ctx|
  let source = "proc load() [fs] {\n  let _ = Path(\"x\").read_text()?\n}\n"
  let root = project(ctx, {"load.xsh": source})?
  assert_effects_fix(fp"{root}/load.xsh", source, source.replace("[fs]", with: "[fs, error]"))
}

test test_effect_annotation_lints_skip_unrestricted_entrypoints { |ctx|
  let root = project(
    ctx,
    {
      "xsht-config.ini": "test_roots = tests\n",
      "tests/test-and-main.xsh": "test test_reads_clock { |_ctx|\n  let _ = time.now()\n}\n\nproc main() {\n  let _ = time.now()\n}\n",
      "tests/partial-clause.xsh": "test test_reads_clock [] { |_ctx|\n  let _ = time.now()\n}\n",
      "tests/cli-main.xsh": "cli main(count: Int) {\n  let _ = time.now()\n  print \$count\n}\n",
    },
  )?
  for name in ["tests/test-and-main.xsh", "tests/cli-main.xsh"] {
    let reported = lint(fp"{root}/{name}", ["--only", rule])?
    assert reported.status.exited_with(0), f"{name}: {reported.stderr}"
    assert rule not in reported.stderr, f"{name}: {reported.stderr}"
  }

  # Present clauses remain upper bounds.
  let partial = run.capture --text "xsht" check fp"{root}/tests/partial-clause.xsh"
  assert partial.status.exited_with(2), partial.stderr
  assert "err[check.effect-violation]" in partial.stderr, partial.stderr
}

test test_effect_annotation_lints_leave_inferred_exports_streams_and_main_alone { |ctx|
  let source = r"""##! Effect contract fixture.
## Reads the clock.
export proc stamp() -> Int {
  let _ = time.now()
  1
}

stream ticks() -> Stream[Int] {
  let _ = time.now()
  yield 1
}

## Reads the clock under the conventional entry name.
export proc main() {
  let _ = time.now()
}

for tick in ticks() {
  print ${stamp() + tick}
}
"""
  let root = project(ctx, {"xsht-config.ini": "test_roots = tests\n", "tests/contract.xsh": source})?
  let reported = lint(fp"{root}/tests/contract.xsh", ["--only", rule])?
  assert reported.status.exited_with(0), reported.stderr
  assert rule not in reported.stderr, reported.stderr
}

test test_missing_effects_fix_adds_the_effects_of_a_called_restricted_proc { |ctx|
  let source = "proc timestamp() [time] -> Int {\n  time.now()\n}\n\nproc stamp() [] -> Int {\n  timestamp()\n}\n"
  let root = project(ctx, {"stamp.xsh": source})?
  assert_effects_fix(
    fp"{root}/stamp.xsh",
    source,
    source.replace("proc stamp() [] -> Int", with: "proc stamp() [time] -> Int"),
  )
}

test test_missing_effects_fix_adds_the_effects_of_an_imported_module_proc { |ctx|
  let module_source = "##! Kbuild lint fixture module.\n## Returns a task status with an environment effect.\nexport proc image_task() [env] -> Int {\n  1\n}\n"
  let source = "use kbuild\n\nproc build() [] -> Int {\n  kbuild.image_task()\n}\n"
  let root = project(ctx, {"kbuild.xsh": module_source, "main.xsh": source})?
  assert_effects_fix(
    fp"{root}/main.xsh",
    source,
    source.replace("proc build() [] -> Int", with: "proc build() [env] -> Int"),
  )
  assert fp"{root}/kbuild.xsh".read_text()? == module_source
}
