let marker = fp"${ARGV[0]}"
let helper = fp"${ARGV[1]}"
var label = "before"

on USR1 [fs, error] {
  marker.write(label)?
  abort(0)
}

label = "after"
let _sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
time.sleep(5s)?
print "after"
