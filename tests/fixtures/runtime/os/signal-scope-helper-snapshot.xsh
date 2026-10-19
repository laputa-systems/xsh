let marker = fp"{args[0]}"
let helper = fp"{args[1]}"
var label = "before"

pure captured_label() -> Str { label }
proc set_label(value: Str) { label = value }

on USR1 [fs, error] {
  var output = f"{label} {captured_label()} {args[2]}"
  label = "hook"
  output += f"\n{label} {captured_label()}"
  set_label("helper")
  output += f"\n{label} {captured_label()}"
  marker.write(output)?
  exit 0
}

label = "after"
let _sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
time.sleep(5s)?
print "after"
