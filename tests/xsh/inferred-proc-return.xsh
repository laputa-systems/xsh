# `lint.prefer-inferred-proc-return` drops a private proc's return annotation
# only when a second check of the file proves nothing else changes, and
# `lint.redundant-result-unit` leaves a family-typed `Result[Unit, E]` alone.

const disks_module = """##! Disk records.

## One mounted filesystem.
export type Disk = {mount: Str, used: Int}

## The disk mounted at `mount`.
export pure disk(mount: Str) -> Disk {
  Disk(mount:, used: mount.byte_len())
}
"""

const script = r"""use disks

error PickError {
  Negative(message: Str)
}

proc root() -> Result[disks.Disk] {
  disks.disk("/")
}

proc used() [error] -> Result[Int] {
  root()?.used
}

proc depth(n: Int) [error] -> Result[Int] {
  if n <= 0 {
    return 0
  }

  depth(n - 1)? + 1
}

proc pick(n: Int) -> Result[Int, PickError] {
  if n < 0 {
    return Err(.Negative(message: "negative"))
  }

  n
}

proc refuse(n: Int) -> Result[Unit, PickError] {
  if n < 0 {
    return Err(.Negative(message: "negative"))
  }
}

proc names() -> Result[List[Str]] {
  []
}

print ${used()?} ${depth(2)?} ${pick(3) ?? 0} ${names()?.len()}
let _ = refuse(1)
"""

proc project(ctx: TestContext, config: Str) [fs, error] -> Result[Path, Error] {
  let root = test.temp_dir(ctx, name: "inferred-proc-return")?
  fp"{root}/lib".mkdir()
  fp"{root}/bin".mkdir()
  fp"{root}/xsht-config.ini".write(config)
  fp"{root}/lib/disks.xsh".write(disks_module)
  fp"{root}/bin/report.xsh".write(script)
  root
}

test test_inferred_proc_returns_are_dropped_only_where_proved { |ctx|
  let root = project(ctx, "module_path = lib\n\n[lint]\nprefer-inferred-proc-returns = true\n")?
  let report = fp"{root}/bin/report.xsh"
  let before = run.capture --text "xsh" $report
  assert before.status.exited_with(0), before.stderr
  assert before.stdout == "1 2 3 0\n", before.stdout

  let reported = run.capture --text "xsht" lint --only lint.prefer-inferred-proc-return $report
  assert reported.stderr.split("warn[lint.prefer-inferred-proc-return]").len() == 3, reported.stderr

  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-inferred-proc-return $report
  assert fixed.status.exited_with(0), fixed.stderr
  let text = report.read_text()?
  # Proved through the imported module's types.
  assert "proc root() {" in text, text
  assert "proc used() [error] {" in text, text
  # Recursive, inferred-variant, and expected-type annotations stay.
  assert "proc depth(n: Int) [error] -> Result[Int] {" in text, text
  assert "proc pick(n: Int) -> Result[Int, PickError] {" in text, text
  assert "proc refuse(n: Int) -> Result[Unit, PickError] {" in text, text
  assert "proc names() -> Result[List[Str]] {" in text, text

  let checked = run.capture --text "xsht" check $report
  assert checked.status.exited_with(0), checked.stderr
  let after = run.capture --text "xsh" $report
  assert after.stdout == before.stdout, after.stdout

  let again = run.capture --text "xsht" lint --only lint.prefer-inferred-proc-return $report
  assert "lint.prefer-inferred-proc-return" not in again.stderr, again.stderr
}

test test_inferred_proc_return_lint_is_opt_in { |ctx|
  let root = project(ctx, "module_path = lib\n")?
  let report = fp"{root}/bin/report.xsh"
  let reported = run.capture --text "xsht" lint --only lint.prefer-inferred-proc-return $report
  assert "lint.prefer-inferred-proc-return" not in reported.stderr, reported.stderr
}

# A family in the annotation is a contract the body's `Err(.Variant(...))`
# relies on; only the broad `Result[Unit]` is redundant.
test test_redundant_result_unit_keeps_a_family_typed_annotation { |ctx|
  let root = project(ctx, "module_path = lib\n")?
  let report = fp"{root}/bin/report.xsh"
  let reported = run.capture --text "xsht" lint --only lint.redundant-result-unit $report
  assert "lint.redundant-result-unit" not in reported.stderr, reported.stderr
  let fixed = run.capture --text "xsht" lint --fix $report
  assert "check.inferred-variant" not in fixed.stderr, fixed.stderr
  let text = report.read_text()?
  assert "proc refuse(n: Int) -> Result[Unit, PickError] {" in text, text
  assert "proc pick(n: Int) -> Result[Int, PickError] {" in text, text
}
