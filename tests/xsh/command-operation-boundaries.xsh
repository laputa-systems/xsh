test test_display_commands_preserve_scalar_domains_and_inherited_output { |ctx|
  let source = r"""proc text(value: Str) [] -> Unit { print $value }
proc integer(value: Int) [] -> Unit { print $value }
proc floating(value: Float) [] -> Unit { print $value }
proc boolean(value: Bool) [] -> Unit { print $value }
proc display_path(value: Path) [] -> Unit { print $value }
proc duration(value: Duration) [] -> Unit { print $value }
proc unsigned(value: UInt) [] -> Unit { print $value }
text("word")
integer(7)
floating(1.5)
boolean(true)
display_path(p"relative/../path")
duration(1s)
unsigned(7)
eprint diagnostic
"""
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  assert output == """word
7
1.5
true
relative/../path
1s
7
"""
  assert diagnostics == """diagnostic
"""
  let inherited = test.expect(
    ctx,
    """print --flush inherited
""",
    status: 0,
  )?
  assert inherited.stdout == """inherited
"""
}

test test_display_and_wait_reject_invalid_boundaries_before_execution { |ctx|
  for source in [
    r"""print ${[7]}
""",
    r"""eprint prefix${[7]}suffix
""",
    """let result = wait 7
""",
    """proc denied(handle: ProcessHandle) [] { wait handle }
""",
    """pure denied(handle: ProcessHandle) { wait handle }
""",
  ] {
    let result = test.run_script(ctx, source)?
    let {success: succeeded, stderr: diagnostics, ..} = result
    assert ! succeeded, source
    assert "check." in diagnostics
  }
}
