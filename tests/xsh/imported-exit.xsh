test test_imported_proc_tail_call_preserves_exit_status { |ctx|
  let root = test.temp_dir(ctx)?
  fp"{root}/finish.xsh".write("""
##! Imported exit fixture.
## Exit when the caller reports a failure.
export proc status(failed: Bool) {
  if failed { exit 7 }
}
""")
  let main = fp"{root}/main.xsh"
  main.write("""
use finish
proc main() { finish.status(true) }
""")
  let xsh = ctx.xsh_bin
  let output = run.capture --text $xsh $main
  assert output.status.exited_with(7), output.stderr
  assert output.stdout == ""
  assert output.stderr == ""
}

test test_imported_proc_exit_unwinds_caller_cleanup { |ctx|
  let root = test.temp_dir(ctx)?
  fp"{root}/finish.xsh".write("""
##! Imported exit fixture.
## Exit after registering cleanup.
export proc status(failed: Bool) {
  defer { print "callee cleanup" }
  if failed { exit 7 }
}
""")
  let main = fp"{root}/main.xsh"
  main.write("""
use finish
proc main() {
  defer { print "caller cleanup" }
  finish.status(true)
  print "unreachable"
}
""")
  let xsh = ctx.xsh_bin
  let output = run.capture --text $xsh $main
  assert output.status.exited_with(7), output.stderr
  assert output.stdout == "callee cleanup\ncaller cleanup\n"
  assert output.stderr == ""
}
