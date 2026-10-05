type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file` and returns what `rule` reports for it. The
# source must check, so that a silent rule is a decision of the rule.
proc reported(file: Path, rule: Str, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let report = lint(file, ["--only", rule])?
  assert "err[" not in report.stderr, report.stderr
  Ok(report.stderr)
}

# Applies the fixes of `rule` alone to `file` and returns the text it then
# holds.
proc fixed_by(file: Path, rule: Str) [fs, process, env, error] -> Result[Str] {
  let _ = lint(file, ["--fix", "--only", rule])?
  file.read_text()
}

# Requires `file` to check without a diagnostic.
proc assert_checks(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Requires `file` to be laid out as `xsht fmt` lays it out.
proc assert_formatted(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

test test_prefer_slice_fixes_proven_byte_bounds_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "prefer-slice")?
  let file = fp"{root}/slice.xsh"
  let source = r"""let data = b"abcdef"
let prefix = data.slice(0, length: 3) # é retained
let suffix = b"abcdef".slice(offset: 2)
let whole = data.slice(0, data.len())
let empty = data.slice(0, 0)
print ${prefix.base64()} ${suffix.base64()} ${whole.base64()} ${empty.base64()}
"""
  let report = reported(file, "lint.prefer-slice", source)?
  assert report.split("warn[lint.prefer-slice]").len() == 5, report
  assert report.split("help: rewrite equivalent byte slice -> ").len() == 5, report
  assert_checks(file)
  assert_formatted(file)

  assert fixed_by(file, "lint.prefer-slice")? == r"""let data = b"abcdef"
let prefix = data[..3] # é retained
let suffix = b"abcdef"[2..]
let whole = data[..]
let empty = data[..0]
print ${prefix.base64()} ${suffix.base64()} ${whole.base64()} ${empty.base64()}
"""
  assert_checks(file)
  assert_formatted(file)
  let again = lint(file, [])?
  assert "lint.prefer-slice" not in again.stderr, again.stderr
}

test test_prefer_slice_retains_uncertain_offsets_counts_and_overflow { |ctx|
  let root = test.temp_dir(ctx, name: "prefer-slice-retained")?
  let file = fp"{root}/slice.xsh"
  let source = """pure count() -> Int {
  return 3
}
pure selected(data: Bytes) -> List[Bytes] {
  let negative = data.slice(-1)
  let uncertain = data.slice(2)
  let arithmetic = data.slice(1, data.len() - 1)
  let effect_count = data.slice(0, count())
  let overflow = data.slice(1, 9223372036854775807)
  [negative, uncertain, arithmetic, effect_count, overflow]
}
"""
  let report = reported(file, "lint.prefer-slice", source)?
  assert_checks(file)
  # Each of the five is reported with a note and without a fix.
  assert report.split("warn[lint.prefer-slice]").len() == 6, report
  assert report.split("\nnote: no automatic fix: ").len() == 6, report
  assert "help: " not in report, report
  assert fixed_by(file, "lint.prefer-slice")? == source
}

test test_record_destructuring_fix_roundtrips_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "record-destructuring")?
  let file = fp"{root}/destructure.xsh"
  let source = r"""# 源
let config = {root: "src", build: {jobs: 3, target: "native"}}
let root = config.root
let jobs = config.build.jobs
let target_name = config.build.target
print $root $jobs $target_name
"""
  let report = reported(file, "lint.prefer-record-destructuring", source)?
  assert "warn[lint.prefer-record-destructuring]" in report, report
  assert_checks(file)

  assert fixed_by(file, "lint.prefer-record-destructuring")? == r"""# 源
let config = {root: "src", build: {jobs: 3, target: "native"}}
let {root, build: {jobs, target: target_name, ..}, ..} = config
print $root $jobs $target_name
"""
  assert_checks(file)
  assert_formatted(file)
  let again = lint(file, [])?
  assert "lint.prefer-record-destructuring" not in again.stderr, again.stderr
}

test test_record_destructuring_retains_annotations_comments_and_effectful_roots { |ctx|
  let root = test.temp_dir(ctx, name: "record-destructuring-retained")?
  let file = fp"{root}/destructure.xsh"
  for source in [
    "let config = {a: 1, b: 2}\nlet a: Int = config.a\nlet b = config.b\nprint \$a \$b\n",
    "let config = {a: 1, b: 2}\nlet a = config.a # useful\nlet b = config.b\nprint \$a \$b\n",
    "type Fields = {a: Int, b: Int}\npure source() -> Fields { return {a: 1, b: 2} }\nlet a = source().a\nlet b = source().b\nprint \$a \$b\n",
  ] {
    let report = reported(file, "lint.prefer-record-destructuring", source)?
    assert "lint.prefer-record-destructuring" not in report, f"{source}{report}"
    assert fixed_by(file, "lint.prefer-record-destructuring")? == source, source
  }
}

test test_formatter_preserves_comments_inside_nested_record_binding_targets { |ctx|
  let root = test.temp_dir(ctx, name: "record-binding-comments")?
  let file = fp"{root}/binding.xsh"
  let source = r"""let config = {root: "src", build: {jobs: 3, target: "native"}}
let {root, build: {
  jobs, # worker count
  target: target_name, ..
}, ..} = config
print $root $jobs $target_name
"""
  file.write(source)
  assert_formatted(file)
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == source
}
