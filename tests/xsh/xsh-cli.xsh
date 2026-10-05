# The `xsh` command line: which words reach the script as `args`, and which
# options the runner itself still takes.

const print_arguments = r"""for arg in args {
  print ${arg}
}
"""

test test_xsh_passes_script_arguments_without_a_separator { |ctx|
  let script = test.temp_file(ctx, name: "argv-no-separator.xsh", contents: bytes.from_text(print_arguments))?
  let output = run.capture --text ${ctx.xsh_bin} $script -f needle
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "-f\nneedle\n"
  assert output.stderr == ""
}

test test_xsh_drops_a_separator_before_script_arguments { |ctx|
  let script = test.temp_file(ctx, name: "argv-with-separator.xsh", contents: bytes.from_text(print_arguments))?
  let output = run.capture --text ${ctx.xsh_bin} $script -- -f needle
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "-f\nneedle\n"
}

test test_xsh_runs_dynamic_record_methods_by_default { |ctx|
  let output = test.expect(
    ctx,
    """proc main(...argv: List[Str]) -> Result[Unit] {
  let exports: Record = {sources: {name: "demo"}}
  let sources = exports.get("sources")?

  if sources.len() != 0 {
    print "non-empty"
  }

  return Ok()
}
""",
    status: 0,
  )?
  assert output.stdout == "non-empty\n"
  assert output.stderr == ""
}

test test_xsh_rejects_the_removed_strict_lower_option { |ctx|
  let output = run.capture --text ${ctx.xsh_bin} --strict-lower
  assert ! output.status.exited_with(0), output.stdout
  assert "unknown xsh option '--strict-lower'" in output.stderr, output.stderr
}

test test_xsh_takes_script_arguments_after_a_leading_separator { |ctx|
  # A shebang line that ends in `--` puts the separator before the script.
  let output = run.capture --text ${ctx.xsh_bin} -- tests/fixtures/runtime/cli-args.xsh one two
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "one\ntwo\n"
  assert output.stderr == ""
}

test test_xsh_help_describes_the_script_runner { |ctx|
  let help = run.capture --text ${ctx.xsh_bin} --help
  assert help.status.exited_with(0), help.stderr
  assert "xsh SCRIPT [ARGS...]" in help.stdout, help.stdout
  assert "--trace" not in help.stdout, help.stdout
}

test test_xsh_rejects_tool_subcommands { |ctx|
  let output = run.capture --text ${ctx.xsh_bin} check tests/fixtures/runtime/cli-simple.xsh
  assert output.status.exited_with(2), output.stderr
  assert "use xsht for tools" in output.stderr, output.stderr
}

test test_xsh_rejects_trace_options { |ctx|
  for option in [["--raw"], ["--trace-format", "jsonl"]] {
    let output = run.capture --text ${ctx.xsh_bin} @option tests/fixtures/runtime/cli-simple.xsh
    assert output.status.exited_with(2), output.stderr
    assert "trace options moved to `xsht trace`" in output.stderr, output.stderr
  }
}

test test_xsh_exits_3_at_a_failed_assertion_and_runs_nothing_after_it { |ctx|
  let output = run.capture --text ${ctx.xsh_bin} tests/fixtures/runtime/assertion-failure.xsh
  assert output.status.exited_with(3), output.stderr
  assert output.stdout == ""
  assert "AssertionError" in output.stderr, output.stderr
  assert "1 == 2" in output.stderr, output.stderr
  assert "assertion-failure.xsh:1" in output.stderr, output.stderr
}

# Runs `source` as a script read from standard input and requires a runtime
# failure that names `fragment` instead of a panic.
proc assert_arithmetic_failure(ctx: TestContext, source: Str, fragment: Str) [process, error] {
  let output = run.capture --text ${ctx.xsh_bin} /dev/stdin < bytes.from_text(source)
  assert output.status.exited_with(3), output.stderr
  assert output.stdout == ""
  assert fragment in output.stderr, output.stderr
  assert "panicked" not in output.stderr, output.stderr
}

test test_integer_division_by_zero_is_a_structured_runtime_failure { |ctx|
  assert_arithmetic_failure(ctx, "1 / 0", "division by zero")
}

test test_integer_remainder_by_zero_is_a_structured_runtime_failure { |ctx|
  assert_arithmetic_failure(ctx, "1 % 0", "division by zero")
}

test test_signed_integer_overflow_is_a_structured_runtime_failure { |ctx|
  assert_arithmetic_failure(ctx, "9223372036854775807 + 1", "integer overflow")
}
