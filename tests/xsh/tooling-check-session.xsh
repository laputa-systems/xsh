type Captured = {status: Status, stdout: Str, stderr: Str}

proc run_tool(root: Path, tool: Str) [process, env, error] -> Result[Captured] {
  let entry = fp"{root}/entry.xsh"
  cd (root) {
    if tool == "xsh" {
      run.capture --text "xsh" $entry
    } else {
      run.capture --text "xsht" $tool $entry
    }
  }
}

proc lint_only(root: Path, rule: Str) [process, env, error] -> Result[Captured] {
  cd (root) {
    run.capture --text "xsht" lint --only $rule "entry.xsh"
  }
}

proc fixture(ctx: TestContext, name: Str) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name:)?
  fp"{root}/xsht-config.ini".write("module_path = modules\n")
  fp"{root}/modules".mkdir()
  root
}

proc expect_code(root: Path, code: Str) [process, env, error] {
  for tool in ["xsh", "check", "lint"] {
    let result = run_tool(root, tool)?
    assert result.status.exited_with(2), f"{tool}: {result.stderr}"
    assert f"err[{code}]" in result.stderr, f"{tool}: {result.stderr}"
    assert result.stdout == "", f"{tool}: {result.stdout}"
  }
}

test test_checker_warnings_are_shared_and_follow_lint_selection { |ctx|
  let root = fixture(ctx, "checker-warnings")?
  fp"{root}/entry.xsh".write(r"""error Input = Failed(message: Str)
error Output = Failed(message: Str)
proc translate(input: Result[Int, Input]) -> Result[Int, Output] {
  match input {
    Ok(value) => return value
    Err(failure) => return Err(Output.Failed(message: failure.message))
  }
}
proc legacy_process_error() {
  let _error = ProcessError("legacy")
}
proc unused_local() {
  let unused = 1
}
""")

  let checked = run_tool(root, "check")?
  assert checked.status.exited_with(2), checked.stderr
  assert "warn[check.error-cause]" in checked.stderr, checked.stderr
  assert "warn[check.migration-error]" in checked.stderr, checked.stderr

  let linted = run_tool(root, "lint")?
  assert "err[" not in linted.stderr, linted.stderr
  assert "warn[check.error-cause]" in linted.stderr, linted.stderr
  assert "warn[check.migration-error]" in linted.stderr, linted.stderr

  let linted_only = lint_only(root, "lint.unused-local")?
  assert "warn[lint.unused-local]" in linted_only.stderr, linted_only.stderr
  assert "warn[check.error-cause]" not in linted_only.stderr, linted_only.stderr
  assert "warn[check.migration-error]" not in linted_only.stderr, linted_only.stderr

  fp"{root}/execution.xsh".write(r"""error Input = Failed(message: Str)
error Output = Failed(message: Str)
proc translate(input: Result[Int, Input]) -> Result[Int, Output] {
  match input {
    Ok(value) => return value
    Err(failure) => return Err(Output.Failed(message: failure.message))
  }
}
print "entry-ran"
""")
  let executed = cd (root) {
    run.capture --text "xsh" "execution.xsh"
  }?
  assert executed.status.exited_with(0), executed.stderr
  assert executed.stdout == "entry-ran\n", executed.stdout
  assert executed.stderr == "", executed.stderr

  fp"{root}/lowering-error.xsh".write("proc legacy_process_error() { let _error = ProcessError(\"legacy\") }\n")
  let failed_execution = cd (root) {
    run.capture --text "xsh" "lowering-error.xsh"
  }?
  assert failed_execution.status.exited_with(2), failed_execution.stderr
  assert "err[compact.indexed-build]" in failed_execution.stderr, failed_execution.stderr
  assert "warn[check.migration-error]" not in failed_execution.stderr, failed_execution.stderr
}

test test_missing_import_is_a_checker_error_in_every_tool { |ctx|
  let root = fixture(ctx, "missing-import")?
  fp"{root}/entry.xsh".write("use missing\nprint \"must not execute\"\n")
  expect_code(root, "check.unknown-module")
  for tool in ["xsh", "check", "lint"] {
    let result = run_tool(root, tool)?
    assert "parse.module-read" not in result.stderr, f"{tool}: {result.stderr}"
    assert fp"{root}/missing.xsh".display() in result.stderr, f"{tool}: {result.stderr}"
    assert fp"{root}/modules/missing.xsh".display() in result.stderr, f"{tool}: {result.stderr}"
  }
}

test test_unreadable_candidate_is_a_load_error_in_every_tool { |ctx|
  let root = fixture(ctx, "unreadable-import")?
  fp"{root}/entry.xsh".write("use unreadable\nprint \"must not execute\"\n")
  fp"{root}/unreadable.xsh".mkdir()
  expect_code(root, "parse.module-read")
}

test test_invalid_utf8_import_keeps_its_source_boundary_code { |ctx|
  let root = fixture(ctx, "invalid-utf8-import")?
  fp"{root}/entry.xsh".write("use broken\nprint \"must not execute\"\n")
  fp"{root}/broken.xsh".write(b"\xff")
  expect_code(root, "source.invalid-utf8")
  for tool in ["xsh", "check", "lint"] {
    let result = run_tool(root, tool)?
    assert "parse.module-read" not in result.stderr, f"{tool}: {result.stderr}"
  }
}

test test_missing_import_does_not_hide_independent_checker_errors { |ctx|
  let root = fixture(ctx, "missing-import-recovery")?
  fp"{root}/entry.xsh".write("use missing\nlet value: Int = \"wrong\"\nprint $value\n")
  for tool in ["xsh", "check", "lint"] {
    let result = run_tool(root, tool)?
    assert result.status.exited_with(2), f"{tool}: {result.stderr}"
    assert "err[check.unknown-module]" in result.stderr, f"{tool}: {result.stderr}"
    assert "err[check.type-mismatch]" in result.stderr, f"{tool}: {result.stderr}"
  }
}

test test_source_errors_stay_before_checking { |ctx|
  let root = fixture(ctx, "source-error-order")?
  fp"{root}/entry.xsh".write("use broken\nlet value: Int = \"wrong\"\nprint $value\n")
  fp"{root}/broken.xsh".write(b"\xff")
  for tool in ["xsh", "check", "lint"] {
    let result = run_tool(root, tool)?
    assert "err[source.invalid-utf8]" in result.stderr, f"{tool}: {result.stderr}"
    assert "check.type-mismatch" not in result.stderr, f"{tool}: {result.stderr}"
  }
}

test test_non_directory_search_component_means_the_candidate_is_absent { |ctx|
  let root = fixture(ctx, "non-directory-import")?
  fp"{root}/entry.xsh".write("use obstruction.child\nprint \"must not execute\"\n")
  fp"{root}/obstruction".write("ordinary file")
  expect_code(root, "check.unknown-module")
}

test test_existing_unreadable_candidate_cannot_fall_through_to_a_later_file { |ctx|
  let root = fixture(ctx, "unreadable-before-valid")?
  fp"{root}/entry.xsh".write("use unreadable\nprint \"must not execute\"\n")
  fp"{root}/unreadable.xsh".mkdir()
  fp"{root}/modules/unreadable.xsh".write("##! Module behind an unreadable candidate.\n")
  expect_code(root, "parse.module-read")
}

test test_invalid_utf8_stops_parsing_and_import_loading_in_every_tool { |ctx|
  let root = fixture(ctx, "invalid-utf8-graph")?
  fp"{root}/entry.xsh".write("use broken\nprint \"must not execute\"\n")
  fp"{root}/broken.xsh".write(b"\xff\nuse unreadable\n")
  fp"{root}/unreadable.xsh".mkdir()
  for tool in ["xsh", "check", "lint"] {
    let result = run_tool(root, tool)?
    assert result.status.exited_with(2), f"{tool}: {result.stderr}"
    assert "err[source.invalid-utf8]" in result.stderr, f"{tool}: {result.stderr}"
    assert "lex.unexpected-character" not in result.stderr, f"{tool}: {result.stderr}"
    assert "parse.module-read" not in result.stderr, f"{tool}: {result.stderr}"
    assert result.stdout == "", f"{tool}: {result.stdout}"
  }
}

test test_proc_main_argument_spread_is_checked_for_every_tool { |ctx|
  let root = fixture(ctx, "main-argument-spread")?
  fp"{root}/entry.xsh".write(r"""proc main(argv: List[Str]) {
  print argv.len()
}
""")
  expect_code(root, "compact.main-missing-spread")

  for source in [
    r"""proc main(...argv: List[Str]) {
  print argv.len()
}
""",
    r"""proc main(argv: List[Str] = []) {
  print argv.len()
}
""",
    r"""proc main(argv: List[Str]) {
  print argv.len()
}
main(args)
""",
  ] {
    fp"{root}/entry.xsh".write(source)
    for tool in ["xsh", "check", "lint"] {
      let result = run_tool(root, tool)?
      assert result.status.exited_with(0), f"{tool}: {result.stderr}"
      assert "compact.main-missing-spread" not in result.stderr, f"{tool}: {result.stderr}"
    }
  }

  fp"{root}/entry.xsh".write(r"""proc main(argv: List[Str]) {
  print argv.len()
}
main(@args)
""")
  expect_code(root, "compact.main-missing-spread")

  fp"{root}/modules/worker.xsh".write(r"""##! Worker module.
## Exported value.
export const value = 1
## Main entry.
export proc main(argv: List[Str]) {
  print argv.len()
}
""")
fp"{root}/entry.xsh".write(r"""use worker
print ${worker.value}
""")
  for tool in ["xsh", "check", "lint"] {
    let result = run_tool(root, tool)?
    assert result.status.exited_with(0), f"{tool}: {result.stderr}"
    assert "compact.main-missing-spread" not in result.stderr, f"{tool}: {result.stderr}"
  }
}
