let helper = fp"{args[0]}"
let ready = fp"{args[1]}"

on USR1 [process, error] {
  let h = spawn process.command_argv(helper, ["os-probe", "ready-sleep", ready.display()])?
  let _sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
  let _status = wait h?
  exit 0
}

let _initial_sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
time.sleep(5s)?
print "after"
