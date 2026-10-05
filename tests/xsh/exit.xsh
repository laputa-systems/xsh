test test_exit_ends_the_script_with_its_status { |ctx|
  let output = test.expect(ctx, "print before\nexit 7\nprint after\n", status: 7)?
  assert output.stdout == "before\n"
  # A deliberate exit is not an error: nothing is reported.
  assert output.stderr == "", output.stderr
}

test test_exit_status_is_an_expression { |ctx|
  let source = "proc finish(code: Int) {\n  match code {\n    0 => exit 0\n    _ => exit if code > 100 { 1 } else { code + 1 }\n  }\n}\n\nfinish(41)\n"
  let _ = test.expect(ctx, source, status: 42)?
}

test test_exit_runs_deferred_cleanup_and_is_not_captured { |ctx|
  let source = "proc finish() {\n  defer { print \"cleanup\" }\n  exit 5\n}\n\nlet outcome = try {\n  finish()\n}\nprint \"captured\"\n"
  let output = test.expect(ctx, source, status: 5)?
  assert output.stdout == "cleanup\n", output.stdout
}

test test_exit_takes_a_postfix_guard { |ctx|
  let source = "proc finish(code: Int) {\n  defer { print \"cleanup\" }\n  exit 4 when code > 100\n  exit 5 unless code > 0\n  print \"kept going\"\n}\n\nfinish(1)\nfinish(0)\n"
  let output = test.expect(ctx, source, status: 5)?
  assert output.stdout == "kept going\ncleanup\ncleanup\n", output.stdout
  let first = test.run_script(ctx, "exit 4 when true\nprint \"after\"\n")?
  assert first.status == 4 and first.stdout == "", first.stderr
}

test test_exit_leaves_its_block { |ctx|
  # A block that ends in `exit` has no value of its own and no way out.
  let source = "proc pick(code: Int) -> Int {\n  let value = if code == 0 { 41 } else { exit code }\n  value + 1\n}\n\nproc checked(ready: Bool) -> Int {\n  guard ready else { exit 6 }\n  pick(0)\n}\n\nprint f\"{checked(true)}\"\nprint f\"{pick(7)}\"\n"
  let output = test.expect(ctx, source, status: 7)?
  assert output.stdout == "42\n", output.stdout
}

test test_desugar_keeps_exit_as_written { |ctx|
  let file = test.temp_file(ctx, name: "guarded.xsh", contents: bytes.from_text("exit 4 when true\n"))?
  let shown = run.capture --text "xsht" desugar $file ?
  assert shown.status.exited_with(0), shown.stderr
  assert "exit 4" in shown.stdout and "abort" not in shown.stdout, shown.stdout
}

test test_abort_is_gone_and_names_exit { |ctx|
  let _ = test.expect(
    ctx,
    "abort(9)\n",
    status: 2,
    stderr: [
      "err[check.removed-abort]: `abort` was removed",
      "`abort` was removed; a deliberate exit is the statement `exit STATUS`",
      "-> exit 9",
    ],
  )?
}

test test_abort_statements_are_rewritten_by_lint_fix { |ctx|
  let source = r"""proc stop(code: Int) {
  if code > 3 {
    abort(code + 1)
  }
  abort(2)
}

proc pick(code: Int) -> Int {
  if code > 3 {
    code
  } else {
    abort(4)
  }
}

stop(pick(5))
"""
  let file = test.temp_file(ctx, name: "stops.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --fix --only "check.removed-abort" $file ?
  assert fixed.status.exited_with(0), fixed.stderr
  assert file.read_text()? == source.replace("abort(code + 1)", with: "exit code + 1")
    .replace("abort(2)", with: "exit 2")
    .replace("abort(4)", with: "exit 4")
  let _ = test.expect(ctx, file.read_text()?, status: 6)?
}

test test_abort_in_an_expression_or_with_force_has_no_rewrite { |ctx|
  let operand = test.expect(
    ctx,
    "let status: Int = abort(3)\n",
    status: 2,
    stderr: ["err[check.removed-abort]", "`exit` is a statement: move the exit out of this expression"],
  )?
  assert "-> exit" not in operand.stderr, operand.stderr
  let forced = test.expect(
    ctx,
    "abort(2, force: true)\n",
    status: 2,
    stderr: [
      "err[check.removed-abort]",
      "`force` has no replacement: `exit STATUS` always runs deferred cleanup",
    ],
  )?
  assert "-> exit" not in forced.stderr, forced.stderr
}

test test_exit_is_not_a_reserved_word { |ctx|
  let source = "let exit = 3\nprint f\"{exit + 1}\"\nlet codes = {exit: 2}\nprint f\"{codes.exit}\"\nexit exit\n"
  let output = test.expect(ctx, source, status: 3)?
  assert output.stdout == "4\n2\n", output.stdout
}

test test_exit_status_is_checked { |ctx|
  let wrong_type = test.run_script(ctx, "exit \"two\"\n")?
  assert ! wrong_type.success
  assert "err[check.type-mismatch]" in wrong_type.stderr, wrong_type.stderr
  assert "expected Int, found Str" in wrong_type.stderr, wrong_type.stderr
  let out_of_range = test.run_script(ctx, "exit 300\n")?
  assert "script exit status must be an integer from 0 to 255" in out_of_range.stderr, out_of_range.stderr
  let bare = test.run_script(ctx, "exit\n")?
  assert ! bare.success
  assert "unresolved proc command `exit`" in bare.stderr, bare.stderr
}

test test_fmt_and_highlight_know_the_exit_statement { |ctx|
  let source = "let ok = false\nif ! ok {\n  exit   2\n}\n"
  let file = test.temp_file(ctx, name: "exits.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $file ?
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == "let ok = false\n\nif ! ok {\n  exit 2\n}\n"
  let shown = run.capture --text "xsht" highlight $file ?
  assert r"""{"kind":"keyword","text":"exit"}""" in shown.stdout, shown.stdout
}
