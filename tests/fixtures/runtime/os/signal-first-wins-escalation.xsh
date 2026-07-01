let helper = fp"${ARGV[0]}"

on USR1 [time, error] {
  time.sleep(1s)?
  abort(0)
}

on USR2 [] {
  print "wrong-hook"
  abort(2)
}

let _sender = process.spawn(
  process.command_argv(helper, ["os-probe", "signal-parent-sequence", "USR1", "50", "USR2", "50"]),
)?

time.sleep(5s)?
print "after"
