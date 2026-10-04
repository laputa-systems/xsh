let marker = fp"{args[0]}"
let helper = fp"{args[1]}"

on USR1 [fs, error] {
  marker.write("hook")?
  abort(0)
}

defer time.sleep(5s)?
let _sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
