let helper = fp"{args[0]}"

on USR1 [] {
  print "hook"
  abort(0)
}

let _ = process.run(process.command_argv(helper, ["os-probe", "signal-parent-then-sleep", "USR1", "50"]))?
print "after"
