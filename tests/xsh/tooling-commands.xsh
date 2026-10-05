test test_top_level_help_is_a_complete_hybrid_reference {
  let help = run.capture --text "xsht" -h
  assert help.status.exited_with(0), help.stderr
  for fragment in [
    "xsht -h | --help",
    "Start here:",
    "Command reference:",
    "lint — Run quality checks and optional fixes",
    "--runless",
    "--trace-format FORMAT",
    "--cov-json FILE",
    "xsht grep 'X.len()' .",
  ] {
    assert fragment in help.stdout, fragment
  }

  assert "Run `xsht COMMAND --help`" not in help.stdout, help.stdout
  for command in [
    "check",
    "fmt",
    "lint",
    "ast",
    "highlight",
    "desugar",
    "grammar",
    "trace",
    "api",
    "test",
    "grep",
    "refactor",
  ] {
    assert f"{command} —" in help.stdout, f"missing {command} help"
  }

  # The grep example sits in the grep section, before the refactor section.
  let after_grep = help.stdout.split("grep —")
  assert after_grep.len() == 2, help.stdout
  assert "xsht grep 'X.len()' ." not in after_grep[0], help.stdout
  let after_example = after_grep[1].split("xsht grep 'X.len()' .")
  assert after_example.len() == 2, help.stdout
  assert "refactor —" not in after_example[0], help.stdout
  assert "refactor —" in after_example[1], help.stdout
}

test test_grammar_prints_the_productions {
  let ebnf = run.capture --text "xsht" grammar
  assert ebnf.status.exited_with(0), ebnf.stderr
  assert "(* Expressions *)" in ebnf.stdout, ebnf.stdout
  assert "\nprogram = " in ebnf.stdout, ebnf.stdout

  let as_json = run.capture --text "xsht" grammar --format json
  assert as_json.status.exited_with(0), as_json.stderr
  assert as_json.stdout.starts_with("{\"sections\":["), as_json.stdout

  let invalid = run.capture --text "xsht" grammar --format yaml
  assert ! invalid.status.exited_with(0), invalid.stderr
  assert "must be ebnf or json" in invalid.stderr, invalid.stderr
}

test test_highlight_prints_runs_that_rebuild_the_source { |ctx|
  let script = test.temp_file(
    ctx,
    name: "sample.xsh",
    contents: bytes.from_text(r"""# note
let n = f"{n:>4} \u{41}" ?? null
"""),
  )?
  let highlighted = run.capture --text "xsht" highlight $script
  assert highlighted.status.exited_with(0), highlighted.stderr
  let lines = highlighted.stdout.lines().collect()
  assert lines[0] == r"""{"kind":"comment","text":"# note"}"""
  assert lines[1] == r"""{"kind":"plain","text":"\n"}"""
  assert lines[2] == r"""{"kind":"keyword","text":"let"}"""
  assert r"""{"kind":"interpolation","text":":>4}"}""" in lines, highlighted.stdout
  assert r"""{"kind":"constant","text":"null"}""" in lines, highlighted.stdout
}

test test_highlight_reports_bad_arguments_and_unreadable_files { |ctx|
  let root = test.temp_dir(ctx, name: "highlight-errors")?
  let binary = test.temp_file(ctx, name: "binary.xsh", contents: bytes.from_ints([255, 254])?)?
  for case in [
    {arguments: ["highlight"], message: "requires SCRIPT"},
    {arguments: ["highlight", "a.xsh", "b.xsh"], message: "exactly one SCRIPT"},
    {arguments: ["highlight", "missing.xsh"], message: "failed to read 'missing.xsh'"},
    {arguments: ["highlight", binary.display()], message: "failed to read"},
  ] {
    let arguments = case.arguments
    let rejected = cd (root) {
      run.capture --text "xsht" @arguments
    }?
    assert rejected.status.exited_with(2), f"{case.message}: {rejected.stderr}"
    assert case.message in rejected.stderr, rejected.stderr
  }
}

test test_lint_help_is_subcommand_specific {
  let help = run.capture --text "xsht" lint --help
  assert help.status.exited_with(0), help.stderr
  assert "xsht lint — Run quality checks and optional fixes" in help.stdout, help.stdout
  assert "Usage:\n  xsht lint" in help.stdout, help.stdout
  assert "--fix" in help.stdout, help.stdout
  assert "--runless" in help.stdout, help.stdout
  assert "xsht trace" not in help.stdout, help.stdout
}

test test_grep_help_keeps_examples_with_grep {
  let help = run.capture --text "xsht" grep --help
  assert help.status.exited_with(0), help.stderr
  assert "xsht grep — Search scripts with AST patterns" in help.stdout, help.stdout
  assert "xsht grep 'X.len()' ." in help.stdout, help.stdout
  assert "xsht grep 'for NAME in ITER' ." in help.stdout, help.stdout
  assert "xsht refactor" not in help.stdout, help.stdout
}

test test_help_topic_uses_the_generated_command_catalog {
  let help = run.capture --text "xsht" help grep
  assert help.status.exited_with(0), help.stderr
  assert "xsht grep — Search scripts with AST patterns" in help.stdout, help.stdout
  assert "xsht grep 'X.len()' ." in help.stdout, help.stdout
  assert "Command reference:" not in help.stdout, help.stdout
}

test test_test_help_lists_parallelism_option {
  let help = run.capture --text "xsht" test --help
  assert help.status.exited_with(0), help.stderr
  assert "xsht test [OPTIONS] [FILTER]" in help.stdout, help.stdout
  assert "--jobs N" in help.stdout, help.stdout
  assert "--api" in help.stdout, help.stdout
  assert "--examples" not in help.stdout, help.stdout
  assert "--all" not in help.stdout, help.stdout
}

test test_lint_short_help_is_accepted {
  let help = run.capture --text "xsht" lint -h
  assert help.status.exited_with(0), help.stderr
  assert "xsht lint [--fix] [--runless] [--only RULE[,RULE...]] [--deny-notes] [FILE...]" in help.stdout, help.stdout
}

test test_ast_prints_parser_debug_output {
  let ast = run.capture --text "xsht" ast tests/fixtures/runtime/cli-trace.xsh
  assert ast.status.exited_with(0), ast.stderr
  assert "Program" in ast.stdout
  assert "ProcDef" in ast.stdout
  assert ast.stderr == ""
}

# A failing script still gets its trace summary, after the error and the
# frame it failed in, and exits with the script's status.
test test_trace_of_a_failing_script_reports_the_error_and_a_timed_summary {
  let traced = run.capture --text "xsht" trace tests/fixtures/runtime/cli-trace-error.xsh
  assert traced.status.exited_with(3), traced.stderr
  assert traced.stdout == ""
  assert "err: `false` exited 1" in traced.stderr, traced.stderr
  assert "runtime traceback" not in traced.stderr, traced.stderr
  assert "proc fail at" in traced.stderr, traced.stderr
  assert "trace summary" in traced.stderr, traced.stderr
  assert "script duration" in traced.stderr, traced.stderr
}

test test_raw_trace_events_carry_start_and_duration {
  let traced = run.capture --text "xsht" trace --raw tests/fixtures/runtime/cli-trace.xsh
  assert traced.status.exited_with(0), traced.stderr
  for fragment in ["kind=script.enter", "kind=proc.enter", "kind=core.call", "start_us=", "duration_us="] {
    assert fragment in traced.stderr, traced.stderr
  }
}
