const unspelled_module = """##! Configuration loading.

## Why loading failed.
export error ConfigError = Missing(file: Path)

## A loader contract.
export type Loader = module {
  export proc load(file: Path) [fs, error] -> Result[Str]
  export optional pure parse(text: Str) -> Result[List[Result[Int]], ConfigError]
}

## A cached outcome.
export type Cached = {name: Str, last: Result[Int]}

## Loads a file.
export proc load(file: Path) [fs, error] -> Result[Str] {
  let text: Result[Str] = file.read_text()
  text
}

## Counts lines.
export pure count(text: Str, seen: Result[Int]) -> Result[Int, ConfigError] {
  let previous = seen ?? 0
  Ok(text.lines().len() + previous)
}

## Doubles a value.
export pure double(value: Int) -> Int { value * 2 }

proc helper(file: Path) [fs, error] -> Result[Str] {
  file.read_text()
}

## Reads and discards a file.
export proc touch(file: Path) [fs, error] {
  let text = helper(file)?
  assert text.byte_len() >= 0
}
"""

const importer = r"""use config
let scratch = fs.tempdir()?
defer scratch.close()?
let file = fp"{scratch.host_path()?}/conf"
file.write("a\nb\n")?
print ${config.load(file)?.trim()}
print ${config.count("x\ny", Ok(1))?}
print ${config.double(4)}
config.touch(file)?
"""

test test_public_result_error_reports_every_public_result { |ctx|
  let root = test.temp_dir(ctx, name: "public-result-report")?
  let module_file = fp"{root}/config.xsh"
  module_file.write_atomic(unspelled_module)
  let reported = run.capture --text "xsht" lint --only lint.public-result-error $module_file ?
  assert ! reported.status.exited_with(0), reported.stderr
  let findings = [line for line in reported.stderr.lines() if "lint.public-result-error" in line]
  assert findings.len() == 5, reported.stderr
  for line in ["config.xsh:8:", "config.xsh:9:", "config.xsh:13:", "config.xsh:16:", "config.xsh:22:"] {
    assert line in reported.stderr, reported.stderr
  }

  # Private signatures, body annotations, and inferred returns are not public.
  for line in ["config.xsh:17:", "config.xsh:30:", "config.xsh:35:"] {
    assert line not in reported.stderr, reported.stderr
  }
}

test test_public_result_error_fix_spells_the_broad_error_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "public-result-fix")?
  let module_file = fp"{root}/config.xsh"
  module_file.write_atomic(unspelled_module)
  let module_env = {XSH_MODULE_PATH: root.display()}
  let fixing = run.capture --text "xsht" lint --fix --only lint.public-result-error $module_file ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = module_file.read_text()?
  for spelled in [
    "export proc load(file: Path) [fs, error] -> Result[Str, Error]\n",
    "export optional pure parse(text: Str) -> Result[List[Result[Int, Error]], ConfigError]",
    "export type Cached = {name: Str, last: Result[Int, Error]}",
    "export proc load(file: Path) [fs, error] -> Result[Str, Error] {",
    "export pure count(text: Str, seen: Result[Int, Error]) -> Result[Int, ConfigError] {",
    "  let text: Result[Str] = file.read_text()",
    "proc helper(file: Path) [fs, error] -> Result[Str] {",
  ] {
    assert spelled in fixed, fixed
  }

  let after = test.run_script(ctx, importer, [], module_env)?
  assert after.success, after.stderr
  assert after.stdout == """a
b
3
8
"""
  let second = run.capture --text "xsht" lint --only lint.public-result-error $module_file ?
  assert second.status.exited_with(0), second.stderr
}

test test_public_result_error_reaches_imported_modules { |ctx|
  let root = test.temp_dir(ctx, name: "public-result-import")?
  fp"{root}/config.xsh".write_atomic(unspelled_module)
  let main = fp"{root}/main.xsh"
  main.write_atomic(importer)
  let checked = run.capture --text "xsht" check $main ?
  assert "check.public-result-error" in checked.stderr, checked.stderr
  assert "config.xsh:16:" in checked.stderr, checked.stderr
}

test test_unspelled_public_results_are_check_errors { |ctx|
  let root = test.temp_dir(ctx, name: "public-result-rejected")?
  fp"{root}/config.xsh".write_atomic(unspelled_module)?
  let rejected = test.run_script(ctx, importer, [], {XSH_MODULE_PATH: root.display()})?
  assert ! rejected.success, rejected.stdout
  assert "check.public-result-error" in rejected.stderr, rejected.stderr

  let private = test.run_script(
    ctx,
    r"""proc helper(text: Str) -> Result[Str] {
  let kept: Result[Str] = Ok(text)
  kept
}
print ${helper("private")?}
""",
  )?
  assert private.success, private.stderr
  assert private.stdout == "private\n"
}
