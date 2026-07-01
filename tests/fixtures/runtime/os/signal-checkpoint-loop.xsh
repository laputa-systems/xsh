let helper = fp"${ARGV[0]}"

on USR1 [] {
  print "hook"
  abort(0)
}

let _sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
var count = 0

while true {
  count += 1
}
