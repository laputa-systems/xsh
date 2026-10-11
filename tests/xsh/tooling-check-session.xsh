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
