type Captured = {status: Status, stdout: Str, stderr: Str}

type CoverageApiHits = {api_hits: Record}

type CoverageHitCount = {tests: Int}

type SourceCoverage = {source_coverage: List[Any]}

const imports_helper = """use helper

test test_imported_helper [error] {
  test.eq(helper.value(), "ok")?
}
"""

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

# Runs `xsht` with `arguments` in `root`.
proc xsht(root: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  cd (root) {
    run.capture --text "xsht" @arguments
  }
}

test test_runner_lists_and_filters_native_tests {
  let listed = run.capture --text "xsht" test --list test_dns
  assert listed.status.exited_with(0), listed.stderr
  # The filter is a substring of the test name: every listed test has it, in
  # path order, and tests named otherwise are left out. The repository gains
  # tests, so the list is not pinned whole.
  let names = listed.stdout.lines()
  assert names[0] == "tests/xsh/basic.xsh::test_dns_mock", listed.stdout
  assert "tests/xsh/stdlib/dns.xsh::test_dns_module_with_mocks" in names, listed.stdout
  for name in names {
    assert "::test_dns" in name, name
  }

  let exact = run.capture --text "xsht" test --exact tests/xsh/basic.xsh::test_pass
  assert exact.status.exited_with(0), exact.stderr
  assert "running 1 tests" in exact.stdout, exact.stdout
  assert "tests/xsh/basic.xsh::test_pass ... ok" in exact.stdout, exact.stdout
}

test test_runner_discovers_tests_from_current_directory { |ctx|
  let source = """
test test_alpha [error] {
  test.eq("a", "a")?
}

test test_beta [error] {
  test.eq("b", "b")?
}
"""
  let root = project(ctx, {"tests/sub/main.xsh": source})?
  let listed = xsht(root, ["test", "--list"])?
  assert listed.status.exited_with(0), listed.stderr
  assert listed.stdout == "tests/sub/main.xsh::test_alpha\ntests/sub/main.xsh::test_beta\n"

  let filtered = xsht(root, ["test", "beta"])?
  assert filtered.status.exited_with(0), filtered.stderr
  assert "running 1 tests" in filtered.stdout, filtered.stdout
  assert "tests/sub/main.xsh::test_beta ... ok" in filtered.stdout, filtered.stdout
}

test test_runner_does_not_read_example_catalogs { |ctx|
  let root = project(ctx, {"examples/catalog.json": "not a catalog"})?
  let listed = xsht(root, ["test", "--list"])?
  assert listed.status.exited_with(0), listed.stderr
  assert listed.stdout == ""
  assert listed.stderr == ""
}

test test_runner_succeeds_when_current_directory_has_no_tests_dir { |ctx|
  let root = test.temp_dir(ctx, name: "no-tests")?
  let ran = xsht(root, ["test"])?
  assert ran.status.exited_with(0), ran.stderr
  assert ran.stdout == "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 skipped\n"
  assert ran.stderr == ""
}

test test_runner_uses_cwd_config_for_excludes_and_module_path { |ctx|
  let helper = """##! CWD config helper module.
## Returns the helper value for the native test.
export pure value() -> Str {
  return "ok"
}
"""
  let root = project(
    ctx,
    {
      "xsht-config.ini": "exclude = tests/ignored/**/*.xsh\nmodule_path = lib\n",
      "lib/helper.xsh": helper,
      "tests/main.xsh": imports_helper,
      "tests/ignored/bad.xsh": "print \"this excluded file is not a native test\"\n",
    },
  )?
  let ran = xsht(root, ["test"])?
  assert ran.status.exited_with(0), ran.stdout + ran.stderr
  assert "running 1 tests" in ran.stdout, ran.stdout
  assert "tests/main.xsh::test_imported_helper ... ok" in ran.stdout, ran.stdout
  assert ran.stderr == ""
}

test test_runner_reports_failures_and_can_keep_temp_roots { |ctx|
  let source = r"""
test test_alpha [fs, io, error] { |ctx|
  print ${ctx.temp_root.display()}
  print "alpha stdout"
  eprint "alpha stderr"
  fp"{ctx.temp_root}/marker".write("kept")?
  test.fail("alpha failed")?
}

test test_beta [error] {
  test.fail("beta should not run")?
}
"""
  let root = project(ctx, {"tests/main.xsh": source})?
  let fail_fast = xsht(root, ["test", "--keep-temp", "--fail-fast"])?
  assert fail_fast.status.exited_with(1), fail_fast.stderr
  assert "running 2 tests" in fail_fast.stdout, fail_fast.stdout
  assert "tests/main.xsh::test_alpha ... FAILED" in fail_fast.stdout, fail_fast.stdout
  assert "test tests/main.xsh::test_beta" not in fail_fast.stdout, fail_fast.stdout
  assert "stdout:\n" in fail_fast.stdout, fail_fast.stdout
  assert "alpha stdout" in fail_fast.stdout, fail_fast.stdout
  assert "stderr:\nalpha stderr" in fail_fast.stdout, fail_fast.stdout
  assert "AssertionError.Failed: alpha failed" in fail_fast.stdout, fail_fast.stdout
  let kept = [
    line
    for line in fail_fast.stdout.lines()
    if "/xsh-test-" in line and line.ends_with("-test_alpha")
  ]
  assert kept.len() == 1, fail_fast.stdout
  let temp_root = fp"{kept[0]}"
  defer { temp_root.remove() }
  assert fp"{temp_root}/marker".read_text()? == "kept"

  let nocapture = xsht(root, ["test", "--nocapture", "--exact", "tests/main.xsh::test_alpha"])?
  assert nocapture.status.exited_with(1), nocapture.stderr
  assert "alpha stdout" in nocapture.stdout, nocapture.stdout
  assert "stdout:\n" not in nocapture.stdout, nocapture.stdout
  assert nocapture.stderr == "alpha stderr\n"
}

test test_runner_captures_process_output_by_default { |ctx|
  let source = """
test test_process_output [process, error] {
  let command = process.command_argv(
    "sh",
    ["sh", "-c", "printf process-stdout; printf process-stderr >&2"],
  )
  test.ok(process.run(command)?.exited_with(0))?
}
"""
  let root = project(ctx, {"tests/main.xsh": source})?
  let captured = xsht(root, ["test"])?
  assert captured.status.exited_with(0), captured.stdout + captured.stderr
  assert "tests/main.xsh::test_process_output ... ok" in captured.stdout, captured.stdout
  assert "process-stdout" not in captured.stdout, captured.stdout
  assert "process-stderr" not in captured.stdout, captured.stdout
  assert captured.stderr == ""

  let nocapture = xsht(root, ["test", "--nocapture", "--exact", "tests/main.xsh::test_process_output"])?
  assert nocapture.status.exited_with(0), nocapture.stdout + nocapture.stderr
  assert "process-stdout" in nocapture.stdout, nocapture.stdout
  assert nocapture.stderr == "process-stderr"
}

test test_runner_cov_list_does_not_execute_tests {
  let listed = run.capture --text "xsht" test --cov --list --exact tests/xsh/basic.xsh::test_pass
  assert listed.status.exited_with(0), listed.stderr
  assert listed.stdout == "tests/xsh/basic.xsh::test_pass\n"
  assert listed.stderr == ""
}

test test_runner_api_requires_coverage_report {
  let rejected = run.capture --text "xsht" test --api --list tests/xsh/basic.xsh::test_pass
  assert rejected.status.exited_with(2), rejected.stderr
  assert rejected.stdout == ""
  assert rejected.stderr == "xsht: `--api` requires `--cov`\n"
}

test test_runner_cov_exact_prints_coverage_sections {
  let ran = run.capture --text "xsht" test --cov --exact tests/xsh/basic.xsh::test_pass
  assert ran.status.exited_with(0), ran.stdout + ran.stderr
  assert "running 1 tests" in ran.stdout, ran.stdout
  assert "tests/xsh/basic.xsh::test_pass ... ok" in ran.stdout, ran.stdout
  assert "coverage report" in ran.stdout, ran.stdout
  assert "Source coverage" in ran.stdout, ran.stdout
  assert "API coverage" not in ran.stdout, ran.stdout
  assert "uncovered standard APIs" not in ran.stdout, ran.stdout
}

test test_runner_cov_api_opt_in_prints_api_sections {
  let ran = run.capture --text "xsht" test --cov --api --exact tests/xsh/basic.xsh::test_pass
  assert ran.status.exited_with(0), ran.stdout + ran.stderr
  assert "API coverage" in ran.stdout, ran.stdout
  assert "uncovered standard APIs" in ran.stdout, ran.stdout
  assert "APIs covered by tests" in ran.stdout, ran.stdout
}

test test_runner_cov_json_out_writes_structured_report { |ctx|
  let report = fp"{test.temp_dir(ctx, name: "cov-json")?}/coverage.json"
  let ran = run.capture --text "xsht" test --exact tests/xsh/basic.xsh::test_pass --cov-json $report
  assert ran.status.exited_with(0), ran.stdout + ran.stderr
  assert "running 1 tests" in ran.stdout, ran.stdout
  assert "coverage report" not in ran.stdout, ran.stdout

  let fields = json.read(report)?.require(Record)?
  let _ = fields.require(SourceCoverage)?
  assert "api_hits" not in fields
}

test test_runner_cov_json_includes_nested_xsh_processes { |ctx|
  let root = test.temp_dir(ctx, name: "nested-cov")?.resolve()?
  let child = fp"{root}/child.xsh"
  let report = fp"{root}/coverage.json"
  child.write("print \${cpu.count()}\n")
  fp"{root}/tests".mkdir()
  fp"{root}/tests/main.xsh".write(
    "\ntest test_child_coverage [process, error] {\n  let output = run.text (Path(" + json.encode(
      ctx.xsh_bin.display(),
    )? + ")) (Path(" + json.encode(child.display())? + ")) ?\n  test.ok(output.trim().parse_int()? > 0)?\n}\n",
  )
  let ran = xsht(
    root,
    ["test", "--cov", "--api", "--exact", "tests/main.xsh::test_child_coverage", "--cov-json", report.display()],
  )?
  assert ran.status.exited_with(0), ran.stdout + ran.stderr
  let hits = json.read(report)?.require(CoverageApiHits)?.api_hits.get("module.cpu.count")?.require(CoverageHitCount)?
  assert hits.tests > 0
}

test test_runner_reports_lazy_default_runtime_failure_without_panicking { |ctx|
  let root = project(
    ctx,
    {
      "tests/lowering.xsh": "pure helper(x: Int = 1 / 0) -> Int {\n  return x\n}\n\ntest test_lowering {\n  let _ = helper()\n}\n",
    },
  )?
  for filter in ["tests/lowering.xsh", "test_lowering"] {
    let ran = xsht(root, ["test", filter])?
    assert ran.status.exited_with(1), ran.stderr
    assert "division by zero" in ran.stdout, ran.stdout
    assert "compact.indexed-build" not in ran.stdout, ran.stdout
  }
}

test test_declaration_discovery_preserves_names_and_runs_each_once { |ctx|
  let source = """pure helper() -> Int { 2 }
proc ordinary_helper() { print helper() }
proc test_named_helper(value: Int) -> Int { value }
test test_old_name { assert test_named_helper(helper()) == 2 }
test no_prefix { |ctx| assert "no_prefix" in ctx.name }
test discarded { |_| }
"""
  let root = project(ctx, {"tests/explicit.xsh": source})?
  let listed = xsht(root, ["test", "--list"])?
  assert listed.status.exited_with(0), listed.stderr
  assert listed.stdout == "tests/explicit.xsh::discarded\ntests/explicit.xsh::no_prefix\ntests/explicit.xsh::test_old_name\n"

  let ran = xsht(root, ["test", "--jobs", "1"])?
  assert ran.status.exited_with(0), ran.stdout
  assert "3 passed; 0 failed" in ran.stdout, ran.stdout
}

test test_declaration_legacy_proc_has_actionable_failure { |ctx|
  let root = project(ctx, {"tests/legacy.xsh": "proc test_old(ctx: TestContext) -> Result[Unit] {}\n"})?
  let ran = xsht(root, ["test", "--jobs", "1"])?
  assert ran.status.exited_with(1), ran.stdout
  assert "check.legacy-test-proc" in ran.stdout, ran.stdout
  assert "keep the exact declared name" in ran.stdout, ran.stdout
  assert "0 passed; 0 failed" not in ran.stdout, ran.stdout

  let filtered = xsht(root, ["test", "--exact", "tests/legacy.xsh::test_old"])?
  assert filtered.status.exited_with(1), filtered.stdout
  assert "check.legacy-test-proc" in filtered.stdout, filtered.stdout
}

test test_declaration_import_registers_without_execution_or_discovery { |ctx|
  let root = project(
    ctx,
    {
      "tests/helper.xsh": "##! Import registration fixture.\n## Returns the fixture value.\nexport pure value() -> Int { 7 }\ntest imported { assert false }\n",
      "tests/entry.xsh": "use helper\ntest entry { assert helper.value() == 7 }\n",
    },
  )?
  let ran = xsht(root, ["test", "--jobs", "1", "tests/entry.xsh"])?
  assert ran.status.exited_with(0), ran.stdout
  assert "1 passed; 0 failed" in ran.stdout, ran.stdout
  assert "::imported" not in ran.stdout, ran.stdout
}
