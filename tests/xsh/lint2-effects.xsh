type Captured = {status: Status, stdout: Str, stderr: Str}

const private_effects = "lint.prefer-inferred-private-effects"

# Writes `source` to a fresh script outside any project configuration.
proc script(ctx: TestContext, source: Str) [fs, error] -> Result[Path] {
  test.temp_file(ctx, name: "lint.xsh", contents: bytes.from_text(source))
}

# Runs `xsht lint` on `file` with `flags` and requires that the file parsed
# and checked, so that an absent finding is a statement about the linter. The
# tool runs in the file's directory: the configuration it reads is the one
# beside the file, or the defaults, never that of the suite's directory.
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

# The text of `file` after every fix of the default rule set was applied.
proc fixed_by_every_rule(file: Path) [fs, process, env, error] -> Result[Str] {
  let _ = lint(file, ["--fix"])?
  file.read_text()
}

# Requires that `file` passes `xsht check`.
proc assert_checked(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# A project holding `source` as `main.xsh` whose configuration turns the
# private effect clause rule on or off, so that the test does not depend on
# the configuration of the directory the suite runs in.
proc private_effects_project(ctx: TestContext, source: Str, enabled: Bool) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "project")?
  fp"{root}/xsht-config.ini".write(f"[lint]\nprefer-inferred-private-effects = {enabled}\n")
  fp"{root}/main.xsh".write(source)
  Ok(root)
}

# How many findings with `code` `xsht lint` reports for `main.xsh` in `root`.
proc project_findings(root: Path, code: Str) [process, env, error] -> Result[Int] {
  findings(fp"{root}/main.xsh", code)
}

# The text of `main.xsh` in `root` after the fixes of `code` alone were applied.
proc project_fixed(root: Path, code: Str) [fs, process, env, error] -> Result[Str] {
  fixed(fp"{root}/main.xsh", code)
}

test test_unconditional_retry_gains_no_selective_filter { |ctx|
  let file = script(
    ctx,
    "error FetchError = Busy(message: Str) | Fatal(message: Str)\nproc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: \"busy\")) }\nlet result = retry [0ms] { attempt()? }\n",
  )?
  let reported = lint(file, [])?
  assert " on (" not in reported.stderr, reported.stderr
  let text = fixed_by_every_rule(file)?
  assert " on (" not in text, text
  assert "retry [0ms] { attempt()? }" in text, text
}

test test_manual_selective_loop_with_observable_counter_and_delay_stays_a_loop { |ctx|
  let file = script(
    ctx,
    "error FetchError = Busy(message: Str) | Fatal(message: Str)\nproc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: \"busy\")) }\nvar attempts = 0\nlet result = loop {\n  attempts += 1\n  let result = attempt()\n  match result {\n    Ok(value) => break Ok(value)\n    Err(error) => {\n      break Err(error) unless error is FetchError.Busy\n      break Err(error) when attempts == 2\n      time.sleep(0ms)\n    }\n  }\n}\nprint \${attempts}\n",
  )?
  let reported = lint(file, [])?
  assert "retry" not in reported.stderr, reported.stderr
  let text = fixed_by_every_rule(file)?
  assert "retry" not in text, text
  assert "let result = loop {" in text, text
}

test test_try_capture_migration_keeps_retry_metadata_and_lexical_returns { |ctx|
  let source = "proc outer() -> Result[Int] {\n  let value = retry [] {\n    return Ok(7)\n  }?\n  value\n}\nlet nested = retry [] { Ok(7) }\n"
  let file = script(ctx, source)?
  let reported = lint(file, [])?
  for line in reported.stderr.lines() {
    assert "-> " not in line or "try " not in line, reported.stderr
  }

  assert fixed_by_every_rule(file)? == source
}

test test_fmt_lexical_ctx_keeps_value_and_label_and_converges { |ctx|
  let file = script(
    ctx,
    "let value = ctx f\"operation {1 + 2}\" {\n  # retain region explanation\n  7\n}\nprint \$value\n",
  )?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  let text = file.read_text()?
  assert "ctx f\"operation {1 + 2}\"" in text, text
  assert "# retain region explanation" in text, text
  assert_checked(file)
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
}

test test_lexical_ctx_lint_visits_label_effects_and_body_bindings { |ctx|
  let file = script(
    ctx,
    "proc label() [io] -> Str { print \"label\"; \"operation\" }\nproc operation() [io] -> Unit { ctx label() { print \"body\" } }\noperation()\n",
  )?
  assert findings(file, "lint.unused-binding")? == 0
}

test test_private_proc_effects_lints_do_not_reinsert_inferred_annotations { |ctx|
  let root = private_effects_project(
    ctx,
    "proc clock() -> Int { let _ = time.now(); 42 }\nproc forwarding() -> Int { clock() }\n",
    enabled: true,
  )?
  assert project_findings(root, "lint.missing-effects")? == 0
  assert project_findings(root, private_effects)? == 0
}

test test_private_effects_removal_is_configurable_checked_and_convergent { |ctx|
  let source = "proc clock() [time] -> Int { let _ = time.now(); 42 }\nlet value = clock()\n"
  let disabled = private_effects_project(ctx, source, enabled: false)?
  assert project_findings(disabled, private_effects)? == 0
  let root = private_effects_project(ctx, source, enabled: true)?
  assert project_findings(root, private_effects)? == 1
  assert project_fixed(root, private_effects)? == "proc clock() -> Int { let _ = time.now(); 42 }\nlet value = clock()\n"
  assert_checked(fp"{root}/main.xsh")
  assert project_findings(root, private_effects)? == 0
}

test test_private_effects_removal_covers_streams_comments_and_bracketed_parameters { |ctx|
  for source in [
    "stream ticks() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nproc caller() -> Int { ticks().collect().len() }\n",
    "# Reads the clock.\nproc documented() [time] -> Int { let _ = time.now(); 42 }\n",
    "# Reads the clock [UTC].\nproc clock(offsets: List[Int] = [1, 2]) [time] -> Int { let _ = time.now(); offsets.len() }\nproc caller() -> Int { clock() }\n",
  ] {
    let root = private_effects_project(ctx, source, enabled: true)?
    assert project_findings(root, private_effects)? == 1, source
    let text = project_fixed(root, private_effects)?
    assert text == source.replace(" [time]", with: ""), text
    assert ") -> Int {" in text or ") -> Stream" in text, text
    let commented = "#" in source
    let still_commented = "#" in text
    assert still_commented == commented, text
    assert_checked(fp"{root}/main.xsh")
  }
}

test test_private_effects_removal_flags_recursion_whose_body_needs_the_effects { |ctx|
  let root = private_effects_project(
    ctx,
    "proc spin(count: Int) [time] -> Int {\n  if count == 0 {\n    let _ = time.now()\n    return 0\n  }\n  spin(count - 1)\n}\n",
    enabled: true,
  )?
  assert project_findings(root, private_effects)? == 1
  let text = project_fixed(root, private_effects)?
  assert text.starts_with("proc spin(count: Int) -> Int {\n"), text
  assert_checked(fp"{root}/main.xsh")
}

test test_signature_cli_literal_schema_fix_keeps_bindings_and_converges { |ctx|
  let file = script(
    ctx,
    "type Options = {jobs: Int, verbose: Bool}\nproc main(...argv: List[Str]) [error] {\n  let {jobs, verbose}: Options = cli.parse(argv, {jobs: {kind: \"Int\", default: 4, help: \"Int, default: 4\"}, verbose: {kind: \"Bool\", default: false, help: \"Bool, default: false\"}})?\n  print \$jobs \$verbose\n}\n",
  )?
  assert findings(file, "lint.prefer-signature-cli")? == 1
  let text = fixed(file, "lint.prefer-signature-cli")?
  assert "cli main(jobs: Int = 4, verbose: Bool = false)" in text, text
  assert "print \$jobs \$verbose" in text, text
  assert_checked(file)
  assert findings(file, "lint.prefer-signature-cli")? == 0
}
