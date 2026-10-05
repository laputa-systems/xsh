type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# The first line of each diagnostic in `stderr`, `SEVERITY[CODE]: MESSAGE`,
# in the order they were reported.
pure headers(stderr: Str) -> List[Str] {
  let found = collect {
    for line in stderr.lines() {
      yield line when rx"^(warn|err|note)\[[^\]]+\]: ".matches(line)
    }
  }

  found
}

# Applies every fix `xsht lint` offers for `file` and requires the result to
# check and to be laid out as `xsht fmt` prints it.
proc assert_fixes_check_and_stay_formatted(file: Path) [process, env, error] {
  let fixed = lint(file, ["--fix"])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1), fixed.stderr
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stdout + formatted.stderr
}

test test_lint_reports_every_rule_of_a_script_in_a_stable_order_with_a_location { |ctx|
  let root = test.temp_dir(ctx, name: "rule-order")?
  let file = fp"{root}/rules.xsh"
  file.write(r"""proc main(argv: List[Str]) {
  let input = argv[0]
  let src = "tmp"
  let root = Path("target/lint")
  let unused = 1
  let p = Path(src)
  fp"{root}/src/lib".mkdir(parents: true)?
  run grep ${input} haystack ?

  if true {
    let src = "other"
    print ${src} ${argv[0]}
  }
}

main(args)?
""")
  let reported = lint(file, [])?
  assert reported.status.exited_with(1), reported.stderr
  let codes = [header.split("]: ")[0] + "]" for header in headers(reported.stderr)]
  assert codes == [
    "warn[lint.path-constructor]",
    "warn[lint.path-constructor]",
    "warn[lint.redundant-propagation]",
    "warn[lint.redundant-default]",
    "warn[lint.redundant-propagation]",
    "warn[lint.run-status]",
    "warn[lint.lexical-block]",
    "err[lint.shadowing]",
    "warn[lint.redundant-command-interpolation]",
    "warn[lint.unused-local]",
    "warn[lint.unused-local]",
    "warn[lint.redundant-propagation]",
  ], reported.stderr

  # Every diagnostic points at a place in the file.
  let located = collect {
    for line in reported.stderr.lines() {
      yield line when rx"^  .*rules\.xsh:[0-9]+:[0-9]+$".matches(line)
    }
  }
  assert located.len() == codes.len(), reported.stderr

  let again = lint(file, [])?
  assert headers(again.stderr) == headers(reported.stderr)
  assert_fixes_check_and_stay_formatted(file)
}

test test_lint_reports_named_underscore_locals_but_allows_sink_binding { |ctx|
  let root = test.temp_dir(ctx, name: "underscore-locals")?
  let file = fp"{root}/locals.xsh"
  file.write("proc main() {\n  let _ = 1\n  let _unused = 2\n}\n\nmain()?\n")
  let reported = lint(file, [])?
  assert headers(reported.stderr) == [
    "warn[lint.unused-local]: unused local variable `_unused`",
    "warn[lint.redundant-propagation]: `?` on a statement that already propagates its failure",
  ], reported.stderr
  assert_fixes_check_and_stay_formatted(file)
}

test test_lint_marks_display_string_interpolation_as_used { |ctx|
  let root = test.temp_dir(ctx, name: "display-string-use")?
  let file = fp"{root}/locals.xsh"
  file.write("proc main() {\n  let dir = \"tmp\"\n  let unused = \"never read\"\n  print f\"dir={dir}\"\n}\n")
  let reported = lint(file, ["--only", "lint.unused-local"])?
  # A genuinely unused local is still reported; the display-string
  # interpolation counts as a use.
  assert headers(reported.stderr) == ["warn[lint.unused-local]: unused local variable `unused`"], reported.stderr
  assert_fixes_check_and_stay_formatted(file)
}

test test_lint_marks_indexed_assignment_keys_as_used { |ctx|
  let root = test.temp_dir(ctx, name: "indexed-assignment-use")?
  let file = fp"{root}/locals.xsh"
  file.write("proc main() {\n  let key = \"name\"\n  var output: Map[Int] = {}\n  output[key] = 1\n}\n")
  let reported = lint(file, ["--only", "lint.unused-local"])?
  assert "err[" not in reported.stderr, reported.stderr
  assert "`key`" not in reported.stderr, reported.stderr
  assert_fixes_check_and_stay_formatted(file)
}
