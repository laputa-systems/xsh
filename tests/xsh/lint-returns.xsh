type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# The codes of the diagnostics in `stderr`, in the order they were reported.
pure codes(stderr: Str) -> List[Str] {
  let found = collect {
    for line in stderr.lines() {
      let header = rx"^(?:warn|err|note)\[([^\]]+)\]: ".captures(line)
      yield header[1] when header.len() == 2
    }
  }

  found
}

# Writes `source` to `file`, applies the fixes of `rule` alone, and returns
# the text the file then holds.
proc fixed_by(file: Path, rule: Str, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let fixed = lint(file, ["--fix", "--only", rule])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1) or fixed.status.exited_with(2), fixed.stderr
  file.read_text()
}

# Requires `file` to check without a diagnostic.
proc assert_checks(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Requires `file` to be laid out as `xsht fmt` prints it.
proc assert_formatted(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stdout + formatted.stderr
}

test test_lint_reports_redundant_result_unit_ceremony { |ctx|
  let root = test.temp_dir(ctx, name: "result-unit")?
  let file = fp"{root}/ceremony.xsh"
  file.write(
    "proc helper() -> Result[Unit] {\n  return Ok()\n}\n\nexport proc public() -> Result[Unit, Error] {\n  return Ok()\n}\n",
  )
  assert_checks(file)
  let reported = lint(file, [])?
  assert codes(reported.stderr) == [
    "lint.redundant-result-unit",
    "lint.redundant-ok-return",
    "lint.redundant-ok-return",
    "lint.unused-callable",
  ], reported.stderr

  let fixed = lint(file, ["--fix"])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1), fixed.stderr
  assert_checks(file)
  assert_formatted(file)
}

test test_lint_fixes_redundant_tail_ok_return { |ctx|
  let root = test.temp_dir(ctx, name: "tail-ok")?
  let file = fp"{root}/tail.xsh"
  let source = "proc parsed(value: Int) -> Result[Int] {\n  return Ok(value + 1)\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-ok-tail"])?
  assert "warn[lint.redundant-ok-tail]" in reported.stderr, reported.stderr

  let fixed = fixed_by(file, "lint.redundant-ok-tail", source)?
  assert fixed == "proc parsed(value: Int) -> Result[Int] {\n  value + 1\n}\n", fixed
  assert_checks(file)
  assert_formatted(file)
}

test test_lint_fixes_redundant_tail_return_binding { |ctx|
  let root = test.temp_dir(ctx, name: "tail-binding")?
  let file = fp"{root}/tail.xsh"
  let source = "proc overlap(left: List[Str], right: List[Str]) -> List[Str] {\n  var values = [item for item in left if item in right]\n  return values\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-tail-return-binding"])?
  assert "warn[lint.redundant-tail-return-binding]" in reported.stderr, reported.stderr

  let fixed = fixed_by(file, "lint.redundant-tail-return-binding", source)?
  assert fixed == "proc overlap(left: List[Str], right: List[Str]) -> List[Str] {\n  [item for item in left if item in right]\n}\n", fixed
  assert_checks(file)
  assert_formatted(file)
}

test test_lint_does_not_fix_tail_return_binding_across_comment { |ctx|
  let root = test.temp_dir(ctx, name: "tail-binding-comment")?
  let file = fp"{root}/tail.xsh"
  let source = "pure value() -> Int {\n  let answer = 42\n  # name the value while debugging this calculation\n  return answer\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-tail-return-binding"])?
  assert "warn[lint.redundant-tail-return-binding]" in reported.stderr, reported.stderr
  assert "help: " not in reported.stderr, reported.stderr
  assert fixed_by(file, "lint.redundant-tail-return-binding", source)? == source
}

test test_lint_fixes_typed_empty_list_tail_return_binding { |ctx|
  let root = test.temp_dir(ctx, name: "tail-binding-empty-list")?
  let file = fp"{root}/tail.xsh"
  let source = "pure values() -> List[Str] {\n  let items: List[Str] = []\n  return items\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-tail-return-binding"])?
  assert "warn[lint.redundant-tail-return-binding]" in reported.stderr, reported.stderr

  let fixed = fixed_by(file, "lint.redundant-tail-return-binding", source)?
  assert fixed == "pure values() -> List[Str] {\n  []\n}\n", fixed
  assert_checks(file)
  assert_formatted(file)
}

test test_lint_fixes_typed_tail_return_binding_when_initializer_already_matches { |ctx|
  let root = test.temp_dir(ctx, name: "tail-binding-typed")?
  let file = fp"{root}/tail.xsh"
  let source = "pure values(items: List[Str]) -> List[Str] {\n  let out: List[Str] = items\n  return out\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-tail-return-binding"])?
  assert "warn[lint.redundant-tail-return-binding]" in reported.stderr, reported.stderr

  let fixed = fixed_by(file, "lint.redundant-tail-return-binding", source)?
  assert fixed == "pure values(items: List[Str]) -> List[Str] {\n  items\n}\n", fixed
  assert_checks(file)
  assert_formatted(file)
}

# Dropping the annotated binding of a record literal would leave a block
# whose tail reads as a nested block, not a record.
test test_lint_does_not_suggest_unparseable_tail_return_for_typed_records { |ctx|
  let root = test.temp_dir(ctx, name: "tail-binding-record")?
  let file = fp"{root}/tail.xsh"
  let source = "type Item = {name: Str, active: Bool, count: Int}\n\nproc convert(value: Str) -> Item {\n  let item: Item = {name: value, active: true, count: 1}\n  return item\n}\n\nproc convert_all(values: List[Str]) -> List[Item] {\n  return values |> map { |value|\n    let item: Item = {name: value, active: true, count: 1}\n    item\n  } |> collect()\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-tail-return-binding"])?
  assert "err[" not in reported.stderr, reported.stderr
  assert "lint.redundant-tail-return-binding" not in reported.stderr, reported.stderr
  assert fixed_by(file, "lint.redundant-tail-return-binding", source)? == source

  let fixed = lint(file, ["--fix"])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1), fixed.stderr
  assert_checks(file)
  assert_formatted(file)
}

test test_lint_removes_checked_tail_returns_in_value_branches { |ctx|
  let root = test.temp_dir(ctx, name: "tail-return-branches")?
  let file = fp"{root}/tail.xsh"
  let source = "pure label(code: Int) -> Str {\n  match code {\n    0 => return \"ok\"\n    _ => {\n      let detail: Str = f\"exit {code}\"\n      return detail # retain this comment\n    }\n  }\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-tail-return"])?
  assert codes(reported.stderr) == ["lint.redundant-tail-return", "lint.redundant-tail-return"], reported.stderr

  let fixed = fixed_by(file, "lint.redundant-tail-return", source)?
  assert fixed == "pure label(code: Int) -> Str {\n  match code {\n    0 => \"ok\"\n    _ => {\n      let detail: Str = f\"exit {code}\"\n      detail # retain this comment\n    }\n  }\n}\n", fixed
  assert_checks(file)
  let again = lint(file, ["--only", "lint.redundant-tail-return"])?
  assert codes(again.stderr) == [], again.stderr
}

# A record literal left as a match arm's value stays an expression: the fixed
# arms check as records, not as blocks.
test test_lint_tail_return_keeps_match_arm_record_an_expression { |ctx|
  let root = test.temp_dir(ctx, name: "tail-return-record")?
  let file = fp"{root}/tail.xsh"
  let source = "type Row = {value: Int}\npure row(code: Int) -> Row {\n  match code {\n    0 => return {value: 1}\n    _ => return {value: 2}\n  }\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-tail-return"])?
  assert codes(reported.stderr) == ["lint.redundant-tail-return", "lint.redundant-tail-return"], reported.stderr

  let fixed = fixed_by(file, "lint.redundant-tail-return", source)?
  assert fixed == source.replace("return {value: 1}", with: "{value: 1}")
    .replace("return {value: 2}", with: "{value: 2}"), fixed
  assert_checks(file)
}

test test_lint_tail_return_preserves_grouping_and_unicode_comments { |ctx|
  let root = test.temp_dir(ctx, name: "tail-return-grouping")?
  let file = fp"{root}/tail.xsh"
  let source = "pure sum() -> Int {\n  return (1 + 2) * 3 # café\n}\n"
  file.write(source)
  assert_checks(file)
  let fixed = fixed_by(file, "lint.redundant-tail-return", source)?
  assert fixed == "pure sum() -> Int {\n  (1 + 2) * 3 # café\n}\n", fixed
  assert_checks(file)
}

test test_lint_keeps_conditional_and_callback_lexical_returns { |ctx|
  let root = test.temp_dir(ctx, name: "tail-return-lexical")?
  let file = fp"{root}/tail.xsh"
  let source = "pure conditional(flag: Bool) -> Int {\n  if flag { return 1 }\n  2\n}\npure callback() -> Int {\n  let rows = [1] |> map { |number| return 4 }\n  9\n}\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.redundant-tail-return"])?
  assert "err[" not in reported.stderr, reported.stderr
  assert "lint.redundant-tail-return" not in reported.stderr, reported.stderr
  assert fixed_by(file, "lint.redundant-tail-return", source)? == source
}
