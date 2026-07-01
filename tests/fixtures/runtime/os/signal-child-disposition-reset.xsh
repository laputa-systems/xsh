let helper = fp"${ARGV[0]}"
let marker = fp"${ARGV[1]}"

on USR1 [] {
  abort(99)
}

let status = process.run(process.command_argv(helper, ["os-probe", "self-signal", "USR1", marker.display()]))?
let usr1 = process.signal("USR1")?
print status.signaled() ${status.signal_number()? == usr1.number} marker.exists()?
