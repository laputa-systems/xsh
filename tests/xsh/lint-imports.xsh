type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file`, requires it to be laid out as `xsht fmt` lays it
# out, and returns what `rule` alone reports for it. The checker must accept
# the source: a check error would hide the rule.
proc reported_by(file: Path, rule: Str, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
  let reported = lint(file, ["--only", rule])?
  assert "err[" not in reported.stderr, reported.stderr
  Ok(reported.stderr)
}

# Applies the fixes of `rule` alone to `file` and returns the text the file
# then holds, which `xsht fmt` must leave as it is.
proc fixed_by(file: Path, rule: Str) [fs, process, env, error] -> Result[Str] {
  let fixed = lint(file, ["--fix", "--only", rule])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1), fixed.stderr
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
  file.read_text()
}

test test_unsorted_import_block_is_reported_with_a_sorting_fix { |ctx|
  let dir = test.temp_dir(ctx, name: "unsorted-imports")?
  let file = fp"{dir}/imports.xsh"
  let reported = reported_by(file, "lint.unsorted-imports", "use json\nuse env\nuse fs\n")?
  assert "warn[lint.unsorted-imports]" in reported, reported
  assert "help: sort import block -> use env\nuse fs\nuse json\n" in reported, reported
  assert fixed_by(file, "lint.unsorted-imports")? == "use env\nuse fs\nuse json\n"
}

test test_import_groups_are_sorted_independently { |ctx|
  let dir = test.temp_dir(ctx, name: "import-groups")?
  let file = fp"{dir}/imports.xsh"
  let reported = reported_by(file, "lint.unsorted-imports", "use json\nuse fs\nlet divider = 1\nuse time\nuse env\n")?
  assert reported.split("warn[lint.unsorted-imports]").len() == 3, reported
  let first = reported.split("help: sort import block -> use fs\nuse json\n")
  assert first.len() == 2, reported
  assert "help: sort import block -> use env\nuse time\n" in first[1], reported
  assert fixed_by(file, "lint.unsorted-imports")? == "use fs\nuse json\nlet divider = 1\nuse env\nuse time\n"
}

# Sorting would move the comment away from the import it explains.
test test_commented_import_block_is_reported_without_a_fix { |ctx|
  let dir = test.temp_dir(ctx, name: "commented-imports")?
  for name in ["zeta", "alpha"] {
    fp"{dir}/{name}.xsh".write(f"##! The {name} module.\n## A value.\nexport const value = 1\n")
  }

  let file = fp"{dir}/imports.xsh"
  let source = "use zeta\n# keep this import near zeta for now\nuse alpha\n"
  let reported = reported_by(file, "lint.unsorted-imports", source)?
  assert "warn[lint.unsorted-imports]" in reported, reported
  assert "help: " not in reported, reported
  assert fixed_by(file, "lint.unsorted-imports")? == source
}

test test_top_level_const_order_is_reported_without_a_fix { |ctx|
  let dir = test.temp_dir(ctx, name: "const-order")?
  let file = fp"{dir}/consts.xsh"
  let source = """pure helper() -> Int {
  return 1
}

let answer = 42
let dynamic = answer
let status = run.status true
"""
  let reported = reported_by(file, "lint.organize-top-level-consts", source)?
  assert reported.split("warn[lint.organize-top-level-consts]").len() == 2, reported
  assert "help: " not in reported, reported
  assert fixed_by(file, "lint.organize-top-level-consts")? == source
}
