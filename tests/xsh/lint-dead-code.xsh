type Captured = {status: Status, stdout: Str, stderr: Str}

const unreachable = "warn[lint.dead-code]: unreachable code"

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

# Writes `source` to a fresh `dead.xsh` and reports its dead code alone.
proc dead_code(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Captured] {
  let root = test.temp_dir(ctx, name: "dead-code")?
  let file = fp"{root}/dead.xsh"
  file.write(source)
  lint(file, ["--only", "lint.dead-code"])
}

# Writes `source` to a fresh `callables.xsh` and reports its unused callables
# alone.
proc unused_callables(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Captured] {
  let root = test.temp_dir(ctx, name: "unused-callable")?
  let file = fp"{root}/callables.xsh"
  file.write(source)
  lint(file, ["--only", "lint.unused-callable"])
}

# Writes `source` to a fresh file, applies every fix `xsht lint` offers for
# it, and requires the result to check and to be laid out as `xsht fmt`
# prints it.
proc assert_fixes_check_and_stay_formatted(ctx: TestContext, source: Str) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "fixed")?
  let file = fp"{root}/fixed.xsh"
  file.write(source)
  let fixed = lint(file, ["--fix"])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1), fixed.stderr
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stdout + formatted.stderr
}

test test_lint_reports_dead_code_after_all_returning_match { |ctx|
  let source = "enum Tok { TOp(Str), TEOF }\n\npure is_op(t: Tok, name: Str) -> Bool {\n  match t {\n    TOp(s) => return s == name\n    _ => return false\n  }\n\n  return false\n}\n"
  let reported = dead_code(ctx, source)?
  assert headers(reported.stderr) == [unreachable], reported.stderr
  assert "dead.xsh:9:3\n" in reported.stderr, reported.stderr
  assert_fixes_check_and_stay_formatted(ctx, source)
}

test test_lint_reports_dead_code_after_return { |ctx|
  let source = "proc work() {\n  return\n  print \"never\"\n}\n"
  let reported = dead_code(ctx, source)?
  assert headers(reported.stderr) == [unreachable], reported.stderr
  assert "dead.xsh:3:3\n" in reported.stderr, reported.stderr
  assert_fixes_check_and_stay_formatted(ctx, source)
}

test test_lint_reports_one_dead_region_after_loop_branches_exit { |ctx|
  let reported = dead_code(
    ctx,
    "proc work(stop: Bool) {\n  loop {\n    if stop {\n      break\n    } else {\n      return\n    }\n    print \"unreachable\"\n    print \"also unreachable\"\n  }\n  print \"reachable\"\n}\n",
  )?
  # One finding covers the region; it starts at its first statement.
  assert headers(reported.stderr) == [unreachable], reported.stderr
  assert "dead.xsh:8:5\n      print \"unreachable\"\n" in reported.stderr, reported.stderr
}

test test_lint_keeps_following_code_reachable_after_conditional_exit_and_zero_iteration_loop { |ctx|
  let reported = dead_code(
    ctx,
    "proc work(stop: Bool) {\n  return when stop\n  while stop {\n    return\n  }\n  print \"reachable\"\n}\n",
  )?
  assert reported.status.exited_with(0), reported.stderr
  assert headers(reported.stderr) == [], reported.stderr
}

test test_lint_keeps_code_after_loop_break_reachable_but_reports_dead_loop_body { |ctx|
  let reported = dead_code(
    ctx,
    "proc main(stop: Bool) {\n  loop {\n    if stop {\n      break\n    } else {\n      continue\n    }\n    print \"dead in loop body\"\n  }\n  print \"reachable after break\"\n}\n",
  )?
  assert headers(reported.stderr) == [unreachable], reported.stderr
  assert "dead.xsh:8:5\n      print \"dead in loop body\"\n" in reported.stderr, reported.stderr
}

test test_lint_reports_dead_code_after_match_without_a_normal_no_arm_path { |ctx|
  let reported = dead_code(
    ctx,
    "proc main(choice: Int) {\n  match choice {\n    1 => return\n  }\n  print \"unreachable\"\n}\n",
  )?
  assert headers(reported.stderr) == [unreachable], reported.stderr
  assert "dead.xsh:5:3\n" in reported.stderr, reported.stderr
}

test test_lint_reports_dead_code_after_all_with_paths_exit { |ctx|
  let reported = dead_code(
    ctx,
    "proc work() {\n  with value = Ok(1) {\n    return\n  } else {\n    return\n  }\n  print \"unreachable\"\n}\n",
  )?
  assert headers(reported.stderr) == [unreachable], reported.stderr
  assert "dead.xsh:7:3\n" in reported.stderr, reported.stderr
}

test test_lint_reports_dead_code_after_exit { |ctx|
  let source = "proc main() {\n  exit 0\n  print \"unreachable\"\n}\n"
  let reported = dead_code(ctx, source)?
  assert headers(reported.stderr) == [unreachable], reported.stderr
  assert "dead.xsh:3:3\n" in reported.stderr, reported.stderr
  assert_fixes_check_and_stay_formatted(ctx, source)
}

test test_lint_reports_unused_callable_but_keeps_main_calls_and_dynamic_references_live { |ctx|
  let source = "pure direct() -> Str {\n  return \"direct\"\n}\n\npure dynamic() -> Str {\n  return \"dynamic\"\n}\n\npure unused() -> Str {\n  return \"unused\"\n}\n\nproc main() {\n  let callback = dynamic\n  print direct() callback.call()\n}\n"
  let reported = unused_callables(ctx, source)?
  assert headers(reported.stderr) == ["warn[lint.unused-callable]: unused callable `unused`"], reported.stderr
  assert_fixes_check_and_stay_formatted(ctx, source)
}

# `[dead-code] exclude` turns off the reachability rules for the matching
# files and nothing else.
test test_lint_config_disables_dead_code_diagnostics_without_disabling_other_lints { |ctx|
  let root = test.temp_dir(ctx, name: "dead-code-excluded")?
  let file = fp"{root}/main.xsh"
  let source = "pure unused() -> Str {\n  return \"unused\"\n}\n\nproc main() {\n  let unused = 1\n}\n\nproc dead() {\n  return\n  print \"dead\"\n}\n"
  file.write(source)
  let enabled = lint(file, [])?
  assert "warn[lint.dead-code]" in enabled.stderr, enabled.stderr
  assert "warn[lint.unused-callable]" in enabled.stderr, enabled.stderr

  fp"{root}/xsht-config.ini".write("[dead-code]\nexclude = *.xsh\n")
  let disabled = lint(file, [])?
  assert "lint.dead-code" not in disabled.stderr, disabled.stderr
  assert "lint.unused-callable" not in disabled.stderr, disabled.stderr
  assert "warn[lint.unused-local]" in disabled.stderr, disabled.stderr
  assert_fixes_check_and_stay_formatted(ctx, source)
}

# A call inside the local's scope is `check.call-target` (the runtime
# would call the local); after the scope ends the call is the function's.
test test_lint_follows_declared_callable_resolution_past_local_bindings { |ctx|
  let reported = unused_callables(
    ctx,
    "pure helper() -> Str {\n  return \"callable\"\n}\n\nproc main() {\n  if true {\n    let helper = 1\n    print \$helper\n  }\n  print helper()\n}\n",
  )?
  assert reported.status.exited_with(0), reported.stderr
  assert headers(reported.stderr) == [], reported.stderr
}

test test_lint_keeps_native_tests_exports_and_recursive_callables_live { |ctx|
  let source = "export pure public_api() -> Int {\n  return helper()\n}\n\npure helper() -> Int {\n  return recursive_a()\n}\n\npure recursive_a() -> Int {\n  return recursive_b()\n}\n\npure recursive_b() -> Int {\n  return 1\n}\n\ntest test_callable_roots {\n  print public_api()\n}\n"
  let reported = unused_callables(ctx, source)?
  assert reported.status.exited_with(0), reported.stderr
  assert headers(reported.stderr) == [], reported.stderr
  assert_fixes_check_and_stay_formatted(ctx, source)
}

test test_lint_reports_unused_module_callable_without_reporting_exported_module_api { |ctx|
  let root = test.temp_dir(ctx, name: "module-reachability")?
  fp"{root}/helper.xsh".write(
    "##! Helper reachability fixture module.\n## Exposes the reachable public API.\nexport pure public_api() -> Int {\n  return private_helper()\n}\n\npure private_helper() -> Int {\n  return 1\n}\n\npure unused_helper() -> Int {\n  return 2\n}\n",
  )
  let entry = fp"{root}/main.xsh"
  entry.write("use helper\n\nproc main() {\n  print helper.public_api()\n}\n")
  let reported = lint(entry, ["--only", "lint.unused-callable"])?
  assert headers(reported.stderr) == ["warn[lint.unused-callable]: unused callable `unused_helper`"], reported.stderr
  # The finding is located in the module that declares the callable.
  assert "helper.xsh:11:1\n" in reported.stderr, reported.stderr
}

test test_lint_counts_a_record_type_annotation_as_a_type_use { |ctx|
  let root = test.temp_dir(ctx, name: "type-use")?
  let file = fp"{root}/types.xsh"
  let declaration = "type Accum = {total: Int, out: List[Str]}\n"
  file.write(declaration + "let initial = 1\n")
  let unused = lint(file, ["--only", "lint.unused-type"])?
  assert headers(unused.stderr) == ["warn[lint.unused-type]: unused type declaration `Accum`"], unused.stderr

  let source = declaration + "let initial: Accum = {total: 0, out: []}\n"
  file.write(source)
  let reported = lint(file, ["--only", "lint.unused-type"])?
  assert reported.status.exited_with(0), reported.stderr
  assert headers(reported.stderr) == [], reported.stderr
  assert_fixes_check_and_stay_formatted(ctx, source)
}
