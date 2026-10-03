test test_display_commands_preserve_scalar_domains_and_inherited_output { |ctx|
  let source = r"""proc text(value: Str) [] -> Unit { print $value }
proc integer(value: Int) [] -> Unit { print $value }
proc floating(value: Float) [] -> Unit { print $value }
proc boolean(value: Bool) [] -> Unit { print $value }
proc display_path(value: Path) [] -> Unit { print $value }
proc duration(value: Duration) [] -> Unit { print $value }
proc unsigned(value: UInt) [] -> Unit { print $value }
proc erased(value: Any) [] -> Unit { print $value }
text("word")
integer(7)
floating(1.5)
boolean(true)
display_path(p"relative/../path")
duration(1s)
unsigned(7)
erased("erased")
eprint diagnostic
"""
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  output == "word\n7\n1.5\ntrue\nrelative/../path\n1s\n7\nerased\n"
  diagnostics == "diagnostic\n"
  let inherited = test.run_script(ctx, "print --flush inherited\n")?
  inherited.success
  inherited.stdout == "inherited\n"
}

test test_display_and_wait_reject_invalid_boundaries_before_execution { |ctx|
  for source in [
    r"print ${[7]}" + "\n",
    r"eprint prefix${[7]}suffix" + "\n",
    "let result = wait 7\n",
    "proc denied(handle: ProcessHandle) [] { wait handle }\n",
    "pure denied(handle: ProcessHandle) { wait handle }\n",
  ] {
    let result = test.run_script(ctx, source)?
    let {success: succeeded, stderr: diagnostics, ..} = result
    assert ! succeeded, source
    "check." in diagnostics
  }
}
