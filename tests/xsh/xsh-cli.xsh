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
